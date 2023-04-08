use anyhow::{anyhow};
use serenity::{async_trait};
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use serenity::model::prelude::*;
use shuttle_secrets::SecretStore;
use sqlx::{PgPool, Executor};
use tracing::{error, info};

mod db;

struct Bot
{
    database: PgPool,
}

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

#[async_trait]
impl EventHandler for Bot {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.content == "!hello" {
            handle_send_error(msg.channel_id.say(&ctx.http, "world!").await);
        }

        let author_id = msg.author.id;
        let channel_id = msg.channel_id;
        let guild_id = if let Some(guild_channel) = channel_id.to_channel(&ctx.http).await.ok().and_then(|response| response.guild()) {
            guild_channel.guild_id
        }
        else {
            handle_send_error(msg.channel_id.say(&ctx.http, "Error: Not currently in a server.").await);
            return;
        };

        if msg.content.starts_with("tt!add ") {
            let command_args = msg.content.split_ascii_whitespace().skip(1);
            if let Err(e) = add_channels(command_args, guild_id, author_id, channel_id, &ctx, &self.database).await {
                handle_send_error(msg.channel_id.say(&ctx.http, format!("Error adding tracked channel(s): {:}", e)).await);
            }
        }
        else if msg.content.starts_with("tt!remove ") {
            let command_args = msg.content.split_ascii_whitespace().skip(1);
            if let Err(e) = remove_channels(command_args, guild_id, author_id, channel_id, &ctx, &self.database).await {
                handle_send_error(msg.channel_id.say(&ctx.http, format!("Error removing tracked channel(s): {:}", e)).await);
            }
        }

        if msg.content == "tt!replies" {
            if let Err(e) = list_threads(guild_id, author_id, channel_id, &ctx, &self.database).await {
                handle_send_error(msg.channel_id.say(&ctx.http, format!("Error: {:}", e)).await);
            }
        }

        if msg.content == "tt!help" {
            handle_send_error(help_message(channel_id, &ctx).await);
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);
    }
}

async fn help_message(channel_id: ChannelId, ctx: &Context) -> Result<Message, SerenityError> {
    let help_message = r#"
`tt!help`
This is the command that reaches this help message. You can use it if you ever have any questions about the current functionality of Thread Tracker.

`tt!add`
This is the command that adds channels and threads to your tracker. After “add”, write a space and then paste the URL of a channel (found under “Copy Link” when you right click or long-press on the channel). If you wish to paste more than one channel, make sure there’s a space between each.

`tt!replies`
This command shows you, in a list, who responded last to each channel.

`tt!remove`
Use this in conjunction with a channel or thread URL to remove that URL from your list, or simply say “all” to remove all channels and threads.
"#;
    channel_id.say(&ctx.http, help_message).await
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
        channel_id.say(
            &ctx.http,
            format!("Please provide a thread or channel URL, for example: `tt!remove {:}` -- or use `tt!remove all` to untrack all threads.",channel_id.mention())
        ).await?;
        return Ok(());
    }

    if let Some(&"all") = args.peek() {
        match db::remove_all(database, guild_id.0 as i64, user_id.0 as i64).await {
            Ok(_) => channel_id.say(&ctx.http, format!("All registered threads for user {:} removed.", user_id.mention())).await?,
            Err(e) => return Err(e.into()),
        };

        return Ok(());
    }

    let mut threads_removed = String::new();
    for thread_id in args {
        if let Some(Ok(target_channel_id)) = thread_id.split("/").last().and_then(|x| Some(x.parse())) {
            let thread = ChannelId(target_channel_id);
            if thread.to_channel(&ctx.http).await.is_ok() {
                match db::remove(database, guild_id.0 as i64, target_channel_id as i64, user_id.0 as i64).await {
                    Ok(_) => threads_removed.push_str(&format!("{:}\n", thread.mention())),
                    Err(e) => return Err(e.into()),
                };
            }
        }
    }

    channel_id.say(&ctx.http, format!("Removed the following channels from your threads list:\n{:}", threads_removed)).await?;

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
        channel_id.say(&ctx.http, format!("Please provide a thread or channel URL, for example: `tt!add {:}`", channel_id)).await?;
        return Ok(());
    }

    let mut threads_added = String::new();
    for thread_id in args {
        if let Some(Ok(target_channel_id)) = thread_id.split("/").last().and_then(|x| Some(x.parse())) {
            let thread = ChannelId(target_channel_id);
            if thread.to_channel(&ctx.http).await.is_ok() {
                match db::add(database, guild_id.0 as i64, target_channel_id as i64, user_id.0 as i64).await {
                    Ok(_) => threads_added.push_str(&format!("{:}\n", thread.mention())),
                    Err(e) => return Err(e.into()),
                };
            }
        }
    }

    channel_id.say(&ctx.http, format!("Added the following channels to your threads list:\n{:}", threads_added)).await?;

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
            let mut author = last_message.author.name.clone();

            if let Some(guild_channel) = last_message.channel(&ctx.http).await?.guild() {
                if let Some(nick) = guild_channel.guild_id.member(&ctx.http, &last_message.author).await.ok().and_then(|member| member.nick) {
                    author = nick;
                }
            }

            author
        }
        else {
            String::from("No replies yet")
        };

        response.push_str(&format!("{:} — {:}\n", thread.channel_id.mention(), author))
    }

    if response.len() == 0 {
        response.push_str("No threads are currently being tracked.");
    }

    channel_id.say(&ctx.http, &response).await?;

    Ok(())
}

fn handle_send_error(result: Result<Message, SerenityError>) {
    if let Err(err) = result {
        error!("Error sending message: {:?}", err);
    }
}

#[shuttle_runtime::main]
async fn serenity(
    #[shuttle_shared_db::Postgres(
        local_uri = "postgres://postgres:{secrets.PASSWORD}@localhost:16695/postgres"
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
