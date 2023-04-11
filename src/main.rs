use anyhow::{anyhow};
use serenity::{
    async_trait,
    model::{
        channel::Message,
        gateway::Ready,
        prelude::*,
    },
    prelude::*,
};
use shuttle_secrets::SecretStore;
use sqlx::Executor;
use thiserror::Error;
use tracing::{error, info};

mod db;
mod background_tasks;
mod threads;
mod messaging;
mod watchers;

use db::Database;
use background_tasks::*;
use messaging::*;

use CommandError::*;

const HELP_MESSAGE: &'static str = r#"
`tt!help`
This is the command that reaches this help message. You can use it if you ever have any questions about the current functionality of Thread Tracker. To report bugs or make feature requests, go to: <https://github.com/vexx32/thread-tracker>

`tt!add` // `tt!track`
This is the command that adds channels and threads to your tracker. After `add`, write a space or linebreak and then paste the URL of a channel (found under `Copy Link` when you right click or long-press on the channel). If you wish to paste more than one channel, make sure there's a space or linebreak between each. To add channels to a specific category, use `tt!add categoryname` followed by the channels you want to add to that category. Category names cannot contain spaces.

`tt!cat` // `tt!category`
This command will let you change an already-tracked thread's category. Specify the category name first, and then thread URLs to change those threads' categories. Use `unset` or `none` as the category name to make the thread(s) uncategorised. If you want to specify more than one thread, make sure there's a space between each. Category names cannot contain spaces.

`tt!rm` // `tt!remove` // `tt!untrack`
Use this in conjunction with a channel or thread URL to remove that URL from your list, one or more category names to remove all threads in those categories, or simply `all` to remove all tracked threads.

`tt!replies` // `tt!threads`
This command shows you, in a list, who responded last to each channel, with each category grouped together. Specify one or more category names to list only the threads in those categories.

`tt!watch`
This command is similar to `tt!replies`, but once the list has been generated, the bot will periodically re-check the threads and update the same message rather than sending additional messages.

`tt!unwatch`
Copy the message URL from an existing watcher message (with the title "Watching threads") and use it with this command to remove the watcher and its associated message.
"#;

#[derive(Debug, Error)]
pub(crate) enum CommandError {
    #[error("Additional arguments are required. {0}")]
    MissingArguments(String),

    #[error("Unrecognised arguments: {0}")]
    UnrecognisedArguments(String),

    #[error("Unknown command `{0}`. Use `tt!help` for a list of commands.")]
    UnknownCommand(String),
}

struct Bot
{
    database: Database,
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
                if let Err(e) = error_on_additional_arguments(args) {
                    send_error_embed(&ctx.http, channel_id, "Too many arguments", e).await;
                };
                help_message(channel_id, &ctx).await;
            },
            "tt!add" | "tt!track" => {
                if let Err(e) = threads::add(args, guild_id, user_id, channel_id, &ctx, &self.database).await {
                    send_error_embed(&ctx.http, channel_id, "Error adding tracked channel(s): {:}", e).await;
                }
            },
            "tt!cat" | "tt!category" => {
                if let Err(e) = threads::set_category(args, guild_id, user_id, channel_id, &ctx, &self.database).await {
                    send_error_embed(&ctx.http, channel_id, "Error updating channels' categories", e).await;
                }
            },
            "tt!rm" | "tt!remove" | "tt!untrack" => {
                if let Err(e) = threads::remove(args, guild_id, user_id, channel_id, &ctx, &self.database).await {
                    send_error_embed(&ctx.http, channel_id, "Error removing tracked channel(s)", e).await;
                }
            },
            "tt!replies" | "tt!threads" => {
                if let Err(e) = threads::send_list(args, guild_id, user_id, channel_id, &ctx, &self.database).await {
                    send_error_embed(&ctx.http, channel_id, "Error retrieving thread list", e).await;
                }
            },
            "tt!watch" => {
                if let Err(e) = watchers::add(args, guild_id, user_id, channel_id, &ctx, &self.database).await {
                    send_error_embed(&ctx.http, channel_id, "Error adding watcher", e).await;
                }
            },
            "tt!unwatch" => {
                if let Err(e) = watchers::remove(args, user_id, channel_id, &ctx, &self.database).await {
                    send_error_embed(&ctx.http, channel_id, "Error removing watcher", e).await;
                }
            },
            other => {
                info!("Unknown command received: {}", other);
                send_error_embed(&ctx.http, channel_id, "Unknown command", UnknownCommand(String::from(other))).await;
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

        run_periodic_tasks(ctx.into(), self.database.clone().into()).await;
    }
}

async fn help_message(channel_id: ChannelId, ctx: &Context) {
    log_send_errors(send_message_embed(&ctx.http, channel_id, "Thread Tracker help", HELP_MESSAGE).await);
}

fn error_on_additional_arguments(unrecognised_args: Vec<&str>) -> Result<(), CommandError> {
    if unrecognised_args.len() > 0 {
        return Err(UnrecognisedArguments(unrecognised_args.join(", ")));
    }

    Ok(())
}

#[shuttle_runtime::main]
async fn serenity(
    #[shuttle_shared_db::Postgres(
        //local_uri = "postgres://postgres:{secrets.PASSWORD}@localhost:16695/postgres"
    )] pool: Database,
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

    client.cache_and_http.cache.set_max_messages(1);

    Ok(client.into())
}
