use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{anyhow};
use chrono::Utc;
use serenity::http::Http;
use serenity::{async_trait};
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use serenity::model::prelude::*;
use serenity::utils::Colour;
use shuttle_secrets::SecretStore;
use sqlx::{PgPool, Executor};
use tracing::{error, info};
use lazy_static::lazy_static;
use regex::Regex;

mod db;

const HEARTBEAT_INTERVAL_SECONDS: u32 = 255;
const WATCHER_UPDATE_INTERVAL_SECONDS: u32 = 120;

lazy_static!{
    static ref URL_REGEX: Regex = Regex::new("^https://discord.com/channels/").unwrap();
}

struct TrackedThread {
    pub channel_id: ChannelId,
    pub category: Option<String>,
    pub guild_id: GuildId,
    pub id: i32,
}

impl From<db::TrackedThreadRow> for TrackedThread {
    fn from(thread: db::TrackedThreadRow) -> Self {
        Self {
            channel_id: ChannelId(thread.channel_id as u64),
            category: thread.category,
            guild_id: GuildId(thread.guild_id as u64),
            id: thread.id,
        }
    }
}

#[derive(Debug)]
struct ThreadWatcher {
    pub message_id: MessageId,
    pub channel_id: ChannelId,
    pub guild_id: GuildId,
    pub user_id: UserId,
    pub id: i32,
    pub categories: Option<String>,
}

impl From<db::ThreadWatcherRow> for ThreadWatcher {
    fn from(watcher: db::ThreadWatcherRow) -> Self {
        Self {
            channel_id: ChannelId(watcher.channel_id as u64),
            message_id: MessageId(watcher.message_id as u64),
            guild_id: GuildId(watcher.guild_id as u64),
            user_id: UserId(watcher.user_id as u64),
            id: watcher.id,
            categories: watcher.categories,
        }
    }
}

struct Bot
{
    database: PgPool,
}

impl Bot {
    async fn process_command(
        &self,
        ctx: &Context,
        channel_id: ChannelId,
        guild_id: GuildId,
        user_id: UserId,
        command: &str,
        args: Vec<&str>
    ) {
        match command {
            "tt!help" => {
                error_on_additional_arguments(&ctx, args, channel_id).await;
                help_message(channel_id, &ctx).await;
            },
            "tt!add" => {
                if let Err(e) = add_threads(args, guild_id, user_id, channel_id, &ctx, &self.database).await {
                    send_error_embed(&ctx.http, channel_id, "Error adding tracked channel(s): {:}", e).await;
                }
            },
            "tt!cat" => {
                if let Err(e) = set_threads_category(args, guild_id, user_id, channel_id, &ctx, &self.database).await {
                    send_error_embed(&ctx.http, channel_id, "Error updating channels' categories", e).await;
                }
            },
            "tt!remove" => {
                if let Err(e) = remove_threads(args, guild_id, user_id, channel_id, &ctx, &self.database).await {
                    send_error_embed(&ctx.http, channel_id, "Error removing tracked channel(s)", e).await;
                }
            },
            "tt!replies" => {
                if let Err(e) = list_threads(args.into_iter().map(|s| s.to_owned()).collect(), guild_id, user_id, channel_id, &ctx, &self.database).await {
                    send_error_embed(&ctx.http, channel_id, "Error retrieving thread list", e).await;
                }
            },
            "tt!watch" => {
                if let Err(e) = add_watcher(args.into_iter().map(|s| s.to_owned()).collect(), guild_id, user_id, channel_id, &ctx, &self.database).await {
                    send_error_embed(&ctx.http, channel_id, "Error adding watcher", e).await;
                }
            },
            "tt!unwatch" => {
                if let Err(e) = remove_watcher(args, user_id, channel_id, &ctx, &self.database).await {
                    send_error_embed(&ctx.http, channel_id, "Error removing watcher", e).await;
                }
            },
            _ => {
                info!("[command] {} was not recognised.", command);
                return;
            }
        }
    }
}


#[async_trait]
impl EventHandler for Bot {
    async fn message(&self, ctx: Context, msg: Message) {
        let author_id = msg.author.id;
        let channel_id = msg.channel_id;
        let guild_id = if let Some(guild_channel) = channel_id.to_channel(&ctx.http).await.ok().and_then(|response| response.guild()) {
            guild_channel.guild_id
        }
        else {
            error!("Error: Not currently in a server.");
            return;
        };

        if !msg.content.starts_with("tt!") {
            return;
        }

        let mut command_args = msg.content.split_ascii_whitespace();
        if let Some(command) = command_args.next() {
            info!("[command] processing command `{}` from user `{}`", msg.content, author_id);
            self.process_command(&ctx, channel_id, guild_id, author_id, command, command_args.collect()).await;
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);

        run_periodic_tasks(&ctx, &self.database).await;
    }
}

async fn run_periodic_tasks(context: &Context, database: &PgPool) {
    let ctx = context.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS.into()));

        loop {
            interval.tick().await;
            heartbeat(&ctx).await;
        }
    });

    let ctx = context.clone();
    let db = database.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(WATCHER_UPDATE_INTERVAL_SECONDS.into()));

        loop {
            interval.tick().await;
            if let Err(e) = update_watchers(&ctx, &db).await {
                error!("Error updating watchers: {}", e);
            }
        }
    });
}

async fn heartbeat(ctx: &Context) {
    ctx.set_presence(Some(Activity::watching("over your threads (tt!help)")), OnlineStatus::Online).await;
    info!("[heartbeat] Keep-alive heartbeat set_presence request completed")
}

async fn update_watchers(ctx: &Context, database: &PgPool) -> Result<(), anyhow::Error> {
    let watchers: Vec<ThreadWatcher> = db::list_watchers(database).await?
        .into_iter()
        .map(|w| w.into())
        .collect();

    for watcher in watchers {
        let mut message = ctx.http.get_message(watcher.channel_id.0, watcher.message_id.0).await?;

        let mut threads: Vec<TrackedThread> = Vec::new();
        match watcher.categories.as_deref() {
            Some("") | None => threads.extend(
            db::list_threads(database, watcher.guild_id.0, watcher.user_id.0, None).await?
                .into_iter()
                .map(|t| t.into())
            ),
            Some(cats) => {
                for category in cats.split(" ") {
                    threads.extend(
                        db::list_threads(database, watcher.guild_id.0, watcher.user_id.0, Some(category)).await?
                            .into_iter()
                            .map(|t| t.into())
                    );
                }
            },
        }

        let threads_content = get_thread_list_content(threads, ctx).await?;

        message.edit(&ctx, |msg| msg.embed(|embed|
                embed.title("Watching threads")
                    .description(threads_content)
                    .footer(|footer| footer.text(format!("Last updated: {}", Utc::now())))))
            .await
            .err()
            .map(|e| error!("Could not edit message: {}", e));
    }

    Ok(())
}

async fn help_message(channel_id: ChannelId, ctx: &Context) {
    const HELP_MESSAGE: &'static str = r#"
`tt!help`
This is the command that reaches this help message. You can use it if you ever have any questions about the current functionality of Thread Tracker. To report bugs or make feature requests, go to: <https://github.com/vexx32/thread-tracker>

`tt!add`
This is the command that adds channels and threads to your tracker. After `add`, write a space or linebreak and then paste the URL of a channel (found under `Copy Link` when you right click or long-press on the channel). If you wish to paste more than one channel, make sure there's a space or linebreak between each. To add channels to a specific category, use `tt!add categoryname` followed by the channels you want to add to that category. Category names cannot contain spaces.

`tt!cat`
This command will let you change an already-tracked thread's category. Specify the category name first, and then thread URLs to change those threads' categories. Use `unset` or `none` as the category name to make the thread(s) uncategorised. If you want to specify more than one thread, make sure there's a space between each. Category names cannot contain spaces.

`tt!remove`
Use this in conjunction with a channel or thread URL to remove that URL from your list, one or more category names to remove all threads in those categories, or simply `all` to remove all tracked threads.

`tt!replies`
This command shows you, in a list, who responded last to each channel, with each category grouped together. Specify one or more category names to list only the threads in those categories.

`tt!watch`
This command is similar to `tt!replies`, but once the list has been generated, the bot will periodically re-check the threads and update the same message rather than sending additional messages.

`tt!unwatch`
Copy the message URL from an existing watcher message (with the title "Watching threads") and use it with this command to remove the watcher and its associated message.
"#;
    handle_send_error(send_message_embed(&ctx.http, channel_id, "Thread Tracker help", HELP_MESSAGE).await);
}

async fn error_on_additional_arguments(ctx: &Context, unrecognised_args: Vec<&str>, channel_id: ChannelId) {
    if unrecognised_args.len() > 0 {
        send_error_embed(&ctx.http, channel_id, "Unrecognised arguments", unrecognised_args.join(", ")).await;
    }
}

async fn add_watcher(
    args: Vec<String>,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &PgPool,
) -> Result<(), anyhow::Error> {
    let arguments = if args.len() > 0 {
        Some(args.join(" "))
    }
    else {
        None
    };

    let message = list_threads_with_title(args, guild_id, user_id, channel_id, "Watching threads", ctx, database).await?;

    if let Err(e) = db::add_watcher(database, user_id.0, message.id.0, channel_id.0, guild_id.0, arguments.as_deref()).await {
        send_error_embed(&ctx.http, channel_id, "Error adding thread watcher", e).await;
    }

    Ok(())
}

async fn remove_watcher(
    args: Vec<&str>,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &PgPool,
) -> Result<(), anyhow::Error> {
    let mut args = args.into_iter().peekable();
    if args.peek().is_none() {
        send_error_embed(
            &ctx.http,
            channel_id,
            "Insufficient arguments provided",
            "Please provide a message URL to a watcher message, such as: `tt!unwatch <message url>`."
        ).await;

        return Ok(());
    }

    let message_url = args.next().unwrap();
    let mut message_url_fragments = message_url.split('/').rev();
    let watcher_message_id: u64 = match message_url_fragments.next().and_then(|s| s.parse().ok()) {
        Some(n) => n,
        None => {
            send_error_embed(&ctx.http, channel_id, "Error fetching watcher", &format!("Could not parse message ID from `{}`", message_url)).await;
            return Ok(());
        }
    };
    let watcher_channel_id: u64 = match message_url_fragments.next().and_then(|s| s.parse().ok()) {
        Some(n) => n,
        None => {
            send_error_embed(&ctx.http, channel_id, "Error fetching watcher", &format!("Could not parse channel ID from `{}`", message_url)).await;
            return Ok(())
        }
    };

    let watcher: ThreadWatcher = match db::get_watcher(database, watcher_channel_id, watcher_message_id).await? {
        Some(w) => w.into(),
        None => {
            send_error_embed(&ctx.http, channel_id, "Error fetching watcher", &format!("Could not find a watcher for the target message: `{}`", message_url)).await;
            return Ok(())
        }
    };

    if watcher.user_id != user_id {
        send_error_embed(&ctx.http, channel_id, "Action not permitted", &format!("User {} does not own the watcher.", user_id)).await;
        return Ok(())
    }

    match db::remove_watcher(database, watcher.guild_id.0, watcher.channel_id.0, watcher.message_id.0).await? {
        0 => error!("Watcher should have been present in the database, but was missing when removal was attempted: {:?}", watcher),
        _ => {
            handle_send_error(send_success_embed(&ctx.http, channel_id, "Watcher removed", "Watcher successfully removed.").await);
            ctx.http.get_message(watcher.channel_id.0, watcher.message_id.0).await?.delete(&ctx.http).await?;
        }
    }

    Ok(())
}

async fn add_threads<'a>(
    args: Vec<&str>,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &PgPool
) -> Result<(), anyhow::Error> {
    let mut args = args.into_iter().peekable();

    let mut threads_added = String::new();
    let mut errors = String::new();

    let category = if !URL_REGEX.is_match(args.peek().unwrap_or(&"")) {
        args.next()
    }
    else {
        None
    };

    if args.peek().is_none() {
        send_error_embed(
            &ctx.http,
            channel_id,
            "Insufficient arguments provided",
            &format!("Please provide a thread or channel URL, such as: `tt!add {channel}`, optionally alongside a category name: `tt!add category {channel}`", channel = channel_id.mention())
        ).await;

        return Ok(());
    }

    for thread_id in args {
        if let Some(Ok(target_channel_id)) = thread_id.split("/").last().and_then(|x| Some(x.parse())) {
            let thread = ChannelId(target_channel_id);
            match thread.to_channel(&ctx.http).await {
                Ok(_) => match db::add_thread(database, guild_id.0, target_channel_id, user_id.0, category.as_deref()).await {
                    Ok(true) => threads_added.push_str(&format!("• {:}\n", thread.mention())),
                    Ok(false) => threads_added.push_str(&format!("• Skipped {:} as it is already being tracked\n", thread.mention())),
                    Err(e) => errors.push_str(&format!("• Failed to register thread {}: {}\n", thread.mention(), e)),
                },
                Err(e) => errors.push_str(&format!("• Cannot access channel {}: {}\n", thread.mention(), e)),
            }
        }
        else {
            errors.push_str(&format!("• Could not parse channel ID: {}\n", thread_id));
        }
    }

    if errors.len() > 0 {
        error!("Errors handling thread registration:\n{}", errors);
        send_error_embed(&ctx.http, channel_id, "Error adding tracked threads", errors).await;
    }

    let title = match category {
        Some(name) => format!("Tracked threads added to `{}`", name),
        None => "Tracked threads added".to_owned(),
    };

    handle_send_error(send_success_embed(&ctx.http, channel_id, title, threads_added).await);

    Ok(())
}

async fn set_threads_category(
    args: Vec<&str>,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &PgPool
) -> Result<(), anyhow::Error> {
    let mut args = args.into_iter().peekable();

    let mut threads_updated = String::new();
    let mut errors = String::new();

    let category = match args.next() {
        Some("unset" | "none") => None,
        Some(cat) => Some(cat),
        None => {
            send_error_embed(
                &ctx.http,
                channel_id,
                "Insufficient arguments provided",
                &format!("Please provide a category name and a thread or channel URL, such as: `tt!cat category {}`", channel_id.mention())
            ).await;

            return Ok(());
        },
    };

    for thread_id in args {
        if let Some(Ok(target_channel_id)) = thread_id.split("/").last().and_then(|x| Some(x.parse())) {
            let thread = ChannelId(target_channel_id);
            match thread.to_channel(&ctx.http).await {
                Ok(_) => match db::update_thread_category(database, guild_id.0, target_channel_id, user_id.0, category.as_deref()).await {
                    Ok(true) => threads_updated.push_str(&format!("• {:}\n", thread.mention())),
                    Ok(false) => threads_updated.push_str(&format!("• Skipped {:} as it is not currently being tracked\n", thread.mention())),
                    Err(e) => errors.push_str(&format!("• Failed to update thread category {}: {}\n", thread.mention(), e)),
                },
                Err(e) => errors.push_str(&format!("• Cannot access channel {}: {}\n", thread.mention(), e)),
            }
        }
        else {
            errors.push_str(&format!("• Could not parse channel ID: {}\n", thread_id));
        }
    }

    if errors.len() > 0 {
        error!("Errors updating thread categories:\n{}", errors);
        send_error_embed(&ctx.http, channel_id, "Error updating thread category", errors).await;
    }

    let title = match category {
        Some(name) => format!("Tracked threads' category set to `{}`", name),
        None => "Tracked threads' categories removed".to_owned(),
    };

    handle_send_error(send_success_embed(&ctx.http, channel_id, title, threads_updated).await);

    Ok(())
}

async fn remove_threads(
    args: Vec<&str>,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &PgPool
) -> Result<(), anyhow::Error> {
    let mut args = args.into_iter().peekable();

    if args.peek().is_none() {
        send_error_embed(
            &ctx.http,
            channel_id,
            "Invalid arguments provided",
            &format!("Please provide a thread or channel URL, for example: `tt!remove {:}` -- or use `tt!remove all` to untrack all threads.", channel_id.mention())
        ).await;
        return Ok(());
    }

    if let Some(&"all") = args.peek() {
        match db::remove_all_threads(database, guild_id.0, user_id.0, None).await {
            Ok(_) => handle_send_error(send_success_embed(
                &ctx.http,
                channel_id,
                "Tracked threads removed",
                &format!("All registered threads for user {:} removed.", user_id.mention())
            ).await),
            Err(e) => return Err(e.into()),
        };

        return Ok(());
    }

    let mut threads_removed = String::new();
    let mut errors = String::new();

    for thread_or_category in args {
        if !URL_REGEX.is_match(thread_or_category) {
            match db::remove_all_threads(database, guild_id.0, user_id.0, Some(thread_or_category)).await {
                Ok(0) => errors.push_str(&format!("• No threads in category {} to remove", thread_or_category)),
                Ok(count) => threads_removed.push_str(&format!("• All {} threads in category `{}` removed", count, thread_or_category)),
                Err(e) => errors.push_str(&format!("• Unable to remove threads in category `{}`: {}", thread_or_category, e)),
            }
        }
        else if let Some(Ok(target_channel_id)) = thread_or_category.split("/").last().and_then(|x| Some(x.parse())) {
            let thread = ChannelId(target_channel_id);
            match thread.to_channel(&ctx.http).await {
                Ok(_) => match db::remove_thread(database, guild_id.0, target_channel_id, user_id.0).await {
                    Ok(_) => threads_removed.push_str(&format!("• {:}\n", thread.mention())),
                    Err(e) => errors.push_str(&format!("• Failed to unregister thread {}: {}\n", thread.mention(), e)),
                },
                Err(e) => errors.push_str(&format!("• Cannot access channel {}: {}\n", thread_or_category, e)),
            }
        }
        else {
            errors.push_str(&format!("• Could not parse channel ID: {}\n", thread_or_category));
        }
    }

    if errors.len() > 0 {
        error!("Errors handling thread removal:\n{}", errors);
        send_error_embed(&ctx.http, channel_id, "Error removing tracked threads", errors).await;
    }

    handle_send_error(send_success_embed(&ctx.http, channel_id, "Tracked threads removed", threads_removed).await);

    Ok(())
}

async fn list_threads(
    args: Vec<String>,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &PgPool
) -> Result<Message, anyhow::Error> {
    list_threads_with_title(args, guild_id, user_id, channel_id, "Currently tracked threads", ctx, database).await
}

async fn list_threads_with_title(
    args: Vec<String>,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    embed_title: impl ToString,
    ctx: &Context,
    database: &PgPool
) -> Result<Message, anyhow::Error> {
    let mut args = args.into_iter().peekable();

    let mut threads: Vec<TrackedThread> = Vec::new();

    if args.peek().is_some() {
        for category in args {
            threads.extend(
                db::list_threads(database, guild_id.0, user_id.0, Some(category.as_str())).await?
                    .into_iter()
                    .map(|t| t.into())
            );
        }
    }
    else {
        threads.extend(
            db::list_threads(database, guild_id.0, user_id.0, None).await?
                .into_iter()
                .map(|t| t.into())
        );
    }

    let response = get_thread_list_content(threads, ctx).await?;

    Ok(send_message_embed(&ctx.http, channel_id, embed_title, &response).await?)
}

async fn get_thread_list_content(threads: Vec<TrackedThread>, ctx: &Context) -> Result<String, SerenityError> {
    let mut response = String::new();
    let mut categories: BTreeMap<Option<String>, Vec<TrackedThread>> = BTreeMap::new();

    for thread in threads {
        categories.entry(thread.category.clone()).or_default().push(thread);
    }

    for (name, threads) in categories {
        match name {
            Some(n) => response.push_str(&format!("__**{}**__\n\n", n)),
            None => {},
        }

        for thread in threads {
            // Default behaviour for retriever is to get most recent messages
            let messages = thread.channel_id.messages(&ctx.http, |retriever| retriever.limit(1)).await?;
            let author = if let Some(last_message) = messages.first() {
                if last_message.author.bot {
                    last_message.author.name.clone()
                }
                else {
                    let mut author = last_message.author.name.clone();

                    if let Some(guild_channel) = last_message.channel(&ctx.http).await?.guild() {
                        if guild_channel.thread_metadata.map(|thread| thread.archived).unwrap_or(false) {
                            guild_channel.edit_thread(&ctx.http, |thread| thread.archived(false)).await
                                .map(|t| info!("Un-archived thread `{}`", t))
                                .err()
                                .map(|e| error!("Unable to un-archive thread `{}`: {}", guild_channel.name, e));
                        }

                        if let Ok(Some(nick)) = guild_channel.guild_id.member(&ctx.http, &last_message.author).await.map(|member| member.nick) {
                            author = nick;
                        }
                    }

                    author
                }
            }
            else {
                String::from("No replies yet")
            };

            response.push_str(&format!("• {} — {}\n", thread.channel_id.mention(), author))
        }

        response.push_str("\n");
    }

    if response.len() == 0 {
        response.push_str("No threads are currently being tracked.");
    }

    Ok(response)
}

async fn send_success_embed(http: impl AsRef<Http>, channel: ChannelId, title: impl ToString, body: impl ToString) -> Result<Message, SerenityError> {
    send_embed(http, channel, title, body, Some(Colour::FABLED_PINK)).await
}

async fn send_error_embed(http: impl AsRef<Http>, channel: ChannelId, title: impl ToString, body: impl ToString) {
    handle_send_error(send_embed(http, channel, title, body, Some(Colour::DARK_ORANGE)).await);
}

async fn send_message_embed(http: impl AsRef<Http>, channel: ChannelId, title: impl ToString, body: impl ToString) -> Result<Message, SerenityError> {
    send_embed(http, channel, title, body, None).await
}

async fn send_embed(
    http: impl AsRef<Http>,
    channel: ChannelId,
    title: impl ToString,
    body: impl ToString,
    colour: Option<Colour>
) -> Result<Message, SerenityError> {
    info!("Sending embed `{}` with content `{}`", title.to_string(), body.to_string());

    channel.send_message(http, |msg| {
        msg.embed(|embed| {
            embed.title(title).description(body).colour(colour.unwrap_or(Colour::PURPLE))
        })
    }).await
}

fn handle_send_error(result: Result<Message, SerenityError>) {
    if let Err(err) = result {
        error!("Error sending message: {:?}", err);
    }
}

#[shuttle_runtime::main]
async fn serenity(
    #[shuttle_shared_db::Postgres(
        //local_uri = "postgres://postgres:{secrets.PASSWORD}@localhost:16695/postgres"
    )] pool: PgPool,
    #[shuttle_secrets::Secrets] secret_store: SecretStore,
) -> shuttle_serenity::ShuttleSerenity {
    use anyhow::Context;

    // Get the discord token set in `Secrets.toml`
    let token = if let Some(token) = secret_store.get("DISCORD_TOKEN") {
        token
    } else {
        return Err(anyhow!("'DISCORD_TOKEN' was not found").into());
    };

    // Run the schema migration
    pool.execute(include_str!("../schema.sql"))
        .await
        .context("failed to run migrations")?;

    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

    let bot = Bot { database: pool };
    let client = Client::builder(&token, intents)
        .event_handler(bot)
        .await
        .expect("Err creating client");

    Ok(client.into())
}
