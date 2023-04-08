use std::time::Duration;

use anyhow::{anyhow};
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

mod db;

const HEARTBEAT_INTERVAL_SECONDS: u64 = 255;

struct TrackedThread {
    pub channel_id: ChannelId,
    pub guild_id: GuildId,
    pub id: i32,
}

impl From<&db::TrackedThread> for TrackedThread {
    fn from(thread: &db::TrackedThread) -> Self {
        Self {
            channel_id: ChannelId(thread.channel_id as u64),
            guild_id: GuildId(thread.guild_id as u64),
            id: thread.id,
        }
    }
}

struct Bot
{
    database: PgPool,
}

impl Bot {
    async fn process_command<'a, I>(&self, ctx: &Context, channel_id: ChannelId, guild_id: GuildId, user_id: UserId, command: &str, args: I)
    where
        I: Iterator<Item = &'a str>
    {
        match command {
            "tt!help" => {
                error_on_additional_arguments(&ctx, args, channel_id).await;
                help_message(channel_id, &ctx).await;
            },
            "tt!add" => {
                if let Err(e) = add_channels(args, guild_id, user_id, channel_id, &ctx, &self.database).await {
                    send_error_embed(&ctx.http, channel_id, "Error adding tracked channel(s): {:}", e).await;
                }
            },
            "tt!remove" => {
                if let Err(e) = remove_channels(args, guild_id, user_id, channel_id, &ctx, &self.database).await {
                    send_error_embed(&ctx.http, channel_id, "Error removing tracked channel(s)", e).await;
                }
            },
            "tt!replies" => {
                error_on_additional_arguments(&ctx, args, channel_id).await;
                if let Err(e) = list_threads(guild_id, user_id, channel_id, &ctx, &self.database).await {
                    send_error_embed(&ctx.http, channel_id, "Error retrieving thread list", e).await;
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
            self.process_command(&ctx, channel_id, guild_id, author_id, command, command_args).await;
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);

        keep_alive(ctx, HEARTBEAT_INTERVAL_SECONDS).await;
    }
}

async fn keep_alive(ctx: Context, heartbeat_interval_seconds: u64) {
    let mut interval = tokio::time::interval(Duration::from_secs(heartbeat_interval_seconds));

    loop {
        interval.tick().await;
        ctx.set_presence(Some(Activity::watching("over your threads (tt!help)")), OnlineStatus::Online).await;
        info!("[heartbeat] Keep-alive heartbeat set_presence request completed")
    }
}

async fn help_message(channel_id: ChannelId, ctx: &Context) {
    let help_message = r#"
`tt!help`
This is the command that reaches this help message. You can use it if you ever have any questions about the current functionality of Thread Tracker.

`tt!add`
This is the command that adds channels and threads to your tracker. After “add”, write a space or linebreak and then paste the URL of a channel (found under “Copy Link” when you right click or long-press on the channel). If you wish to paste more than one channel, make sure there’s a space between each.

`tt!replies`
This command shows you, in a list, who responded last to each channel.

`tt!remove`
Use this in conjunction with a channel or thread URL to remove that URL from your list, or simply say “all” to remove all channels and threads.
"#;
    send_message_embed(&ctx.http, channel_id, "Thread Tracker help", help_message).await
}

async fn error_on_additional_arguments<'a, I>(ctx: &Context, unrecognised_args: I, channel_id: ChannelId)
where
    I: Iterator<Item = &'a str>
{
    let mut unrecognised_args = unrecognised_args.peekable();
    if let Some(_) = unrecognised_args.peek() {
        let args: Vec<_> = unrecognised_args.collect();
        send_error_embed(&ctx.http, channel_id, "Unrecognised arguments", args.join(", ")).await;
    }
}

async fn remove_channels<'a, I>(
    args: I,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &PgPool
) -> Result<(), anyhow::Error>
where
    I: Iterator<Item = &'a str>
{
    let mut args = args.peekable();

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
        match db::remove_all(database, guild_id.0 as i64, user_id.0 as i64).await {
            Ok(_) => send_success_embed(
                &ctx.http,
                channel_id,
                "Tracked threads removed",
                &format!("All registered threads for user {:} removed.", user_id.mention())
            ).await,
            Err(e) => return Err(e.into()),
        };

        return Ok(());
    }

    let mut threads_removed = String::new();
    let mut errors = String::new();
    for thread_id in args {
        if let Some(Ok(target_channel_id)) = thread_id.split("/").last().and_then(|x| Some(x.parse())) {
            let thread = ChannelId(target_channel_id);
            match thread.to_channel(&ctx.http).await {
                Ok(_) => match db::remove(database, guild_id.0 as i64, target_channel_id as i64, user_id.0 as i64).await {
                    Ok(_) => threads_removed.push_str(&format!("• {:}\n", thread.mention())),
                    Err(e) => errors.push_str(&format!("• Failed to unregister thread {}: {}\n", thread.mention(), e)),
                },
                Err(e) => errors.push_str(&format!("• Cannot access channel {}: {}\n", thread_id, e)),
            }
        }
        else {
            errors.push_str(&format!("• Could not parse channel ID: {}\n", thread_id));
        }
    }

    if errors.len() > 0 {
        error!("Errors handling thread removal:\n{}", errors);
        send_error_embed(&ctx.http, channel_id, "Error removing tracked threads", errors).await;
    }

    send_success_embed(&ctx.http, channel_id, "Tracked threads removed", threads_removed).await;

    Ok(())
}

async fn add_channels<'a, I>(
    args: I,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &PgPool
) -> Result<(), anyhow::Error>
where
    I: Iterator<Item = &'a str>
{
    let mut args = args.peekable();
    if args.peek().is_none() {
        send_error_embed(
            &ctx.http,
            channel_id,
            "Invalid arguments provided",
            &format!("Please provide a thread or channel URL, for example: `tt!add {:}`", channel_id.mention())
        ).await;
        return Ok(());
    }

    let mut threads_added = String::new();
    let mut errors = String::new();
    for thread_id in args {
        if let Some(Ok(target_channel_id)) = thread_id.split("/").last().and_then(|x| Some(x.parse())) {
            let thread = ChannelId(target_channel_id);
            match thread.to_channel(&ctx.http).await {
                Ok(_) => match db::add(database, guild_id.0 as i64, target_channel_id as i64, user_id.0 as i64).await {
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

    send_success_embed(&ctx.http, channel_id, "Tracked threads added", threads_added).await;

    Ok(())
}

async fn list_threads(
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &PgPool
) -> Result<(), anyhow::Error> {
    let threads: Vec<TrackedThread> = db::list(database, guild_id.0 as i64, user_id.0 as i64).await?
        .iter()
        .map(|t| t.into())
        .collect();

    let mut response = String::new();
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
                    if let Some(nick) = guild_channel.guild_id.member(&ctx.http, &last_message.author).await.ok().and_then(|member| member.nick) {
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

    if response.len() == 0 {
        response.push_str("No threads are currently being tracked.");
    }

    send_message_embed(&ctx.http, channel_id, "Currently tracked threads", &response).await;

    Ok(())
}

async fn send_success_embed(http: impl AsRef<Http>, channel: ChannelId, title: impl ToString, body: impl ToString) {
    send_embed(http, channel, title, body, Some(Colour::FABLED_PINK)).await
}

async fn send_error_embed(http: impl AsRef<Http>, channel: ChannelId, title: impl ToString, body: impl ToString) {
    send_embed(http, channel, title, body, Some(Colour::DARK_ORANGE)).await;
}

async fn send_message_embed(http: impl AsRef<Http>, channel: ChannelId, title: impl ToString, body: impl ToString) {
    send_embed(http, channel, title, body, None).await;
}

async fn send_embed(http: impl AsRef<Http>, channel: ChannelId, title: impl ToString, body: impl ToString, colour: Option<Colour>) {
    info!("Sending embed `{}` with content `{}`", title.to_string(), body.to_string());
    handle_send_error(
        channel.send_message(http, |msg| {
            msg.embed(|embed| {
                embed.title(title).description(body).colour(colour.unwrap_or(Colour::PURPLE))
            })
        }).await
    );
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
