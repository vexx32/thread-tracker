use anyhow::{anyhow};
use serenity::{
    async_trait,
    model::{
        channel::Message,
        gateway::Ready,
        prelude::*,
    },
    prelude::*, http::Http,
};
use shuttle_secrets::SecretStore;
use sqlx::Executor;
use thiserror::Error;
use tracing::{error, info};

mod db;
mod background_tasks;
mod threads;
mod messaging;
mod muses;
mod todos;
mod utils;
mod watchers;

use db::Database;
use background_tasks::*;
use messaging::*;

use CommandError::*;

const HELP_MESSAGE: &str = r#"
`tt!help`
This is the command that reaches this help message. You can use it if you ever have any questions about the current functionality of Thread Tracker. To report bugs or make feature requests, go to: <https://github.com/vexx32/thread-tracker>

`tt!add` // `tt!track`
This is the command that adds channels and threads to your tracker. After `add`, write a space or linebreak and then paste the URL of a channel (found under `Copy Link` when you right click or long-press on the channel). If you wish to paste more than one channel, make sure there's a space or linebreak between each. To add channels to a specific category, use `tt!add categoryname` followed by the channels you want to add to that category. Category names cannot contain spaces.

`tt!cat` // `tt!category`
This command will let you change an already-tracked thread's category. Specify the category name first, and then thread URLs to change those threads' categories. Use `unset` or `none` as the category name to make the thread(s) uncategorised. If you want to specify more than one thread, make sure there's a space between each. Category names cannot contain spaces.

`tt!rm` // `tt!remove` // `tt!untrack`
Use this in conjunction with a channel or thread URL to remove that URL from your list, one or more category names to remove all threads in those categories, or simply `all` to remove all tracked threads.

`tt!replies` // `tt!threads`
This command shows you, in a list, who responded last to each channel, with each category grouped together along with any to do-list items in those categories. Specify one or more category names to list only the threads and to do-list items in those categories.

`tt!addmuse`
Register a muse name. Registered muses determine which respondents should be considered you when using bots like Tupper. Thread Tracker will list the last respondent to a thread in bold if it is not you or a registered muse.

`tt!removemuse`
Remove a registered muse name.

`tt!muses`
List the currently registered muse names.

`tt!watch`
This command is similar to `tt!replies`, but once the list has been generated, the bot will periodically re-check the threads and update the same message rather than sending additional messages.

`tt!unwatch`
Copy the message URL from an existing watcher message (with the title "Watching threads") and use it with this command to remove the watcher and its associated message.

`tt!random` // `tt!rng`
Finds a random tracked thread that was last replied to by someone other than you.

`tt!todos` // `tt!todolist`
List all to do-list entries.

`tt!todo`
Adds a to do-list item. Optionally specify a category as `!categoryname` before the to do-list entry itself, for example: `tt!todo !mycategory do the thing`

`tt!done`
Crosses off and removes a to do-list item. Add `!categoryname` to remove all entries from that category, or `!all` to remove all to do entries.
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

struct ThreadTrackerBot
{
    database: Database,
}

struct GuildUser {
    user_id: UserId,
    guild_id: GuildId,
}

impl From<&EventData> for GuildUser {
    fn from(value: &EventData) -> Self {
        Self {
            user_id: value.user_id,
            guild_id: value.guild_id,
        }
    }
}

struct EventData {
    user_id: UserId,
    guild_id: GuildId,
    channel_id: ChannelId,
    context: Context,
}

impl EventData {
    fn http(&self) -> &Http {
        &self.context.http
    }

    fn reply_context(&self) -> ReplyContext {
        self.into()
    }

    fn user(&self) -> GuildUser {
        self.into()
    }
}

impl ThreadTrackerBot {
    async fn process_command(
        &self,
        event_data: EventData,
        command: &str,
        args: &str,
    ) {
        let reply_context = event_data.reply_context();
        match command {
            "tt!help" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = error_on_additional_arguments(args) {
                    reply_context.send_error_embed("Too many arguments", e).await;
                };

                help_message(reply_context).await;
            },
            "tt!add" | "tt!track" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = threads::add(args, &event_data, &self.database).await {
                    reply_context.send_error_embed("Error adding tracked channel(s): {:}", e).await;
                }
            },
            "tt!cat" | "tt!category" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = threads::set_category(args, &event_data, &self.database).await {
                    reply_context.send_error_embed("Error updating channels' categories", e).await;
                }
            },
            "tt!rm" | "tt!remove" | "tt!untrack" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = threads::remove(args, &event_data, &self.database).await {
                    reply_context.send_error_embed("Error removing tracked channel(s)", e).await;
                }
            },
            "tt!replies" | "tt!threads" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = threads::send_list(args, &event_data, &self.database).await {
                    reply_context.send_error_embed("Error retrieving thread list", e).await;
                }
            },
            "tt!random" | "tt!rng" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = error_on_additional_arguments(args) {
                    reply_context.send_error_embed("Too many arguments", e).await;
                }

                if let Err(e) = threads::send_random_thread(&event_data, &self.database).await {
                    reply_context.send_error_embed("Error retrieving a random thread", e).await;
                }
            },
            "tt!watch" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = watchers::add(args, &event_data, &self.database).await {
                    reply_context.send_error_embed("Error adding watcher", e).await;
                }
            },
            "tt!unwatch" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = watchers::remove(args, &event_data, &self.database).await {
                    reply_context.send_error_embed("Error removing watcher", e).await;
                }
            },
            "tt!muses" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = error_on_additional_arguments(args) {
                    reply_context.send_error_embed("Too many arguments", e).await;
                }

                if let Err(e) = muses::send_list(&event_data, &self.database).await {
                    reply_context.send_error_embed("Error finding muses", e).await;
                }
            },
            "tt!addmuse" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = muses::add(args, &event_data, &self.database).await {
                    reply_context.send_error_embed("Error adding muse", e).await;
                }
            },
            "tt!removemuse" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = muses::remove(args, &event_data, &self.database).await {
                    reply_context.send_error_embed("Error removing muse", e).await;
                }
            },
            "tt!todo" => {
                if let Err(e) = todos::add(args, &event_data, &self.database).await {
                    reply_context.send_error_embed("Error adding to do-list item", e).await;
                }
            },
            "tt!done" => {
                if let Err(e) = todos::remove(args, &event_data, &self.database).await {
                    reply_context.send_error_embed("Error removing to do-list item", e).await;
                }
            },
            "tt!todos" | "tt!todolist" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = todos::send_list(args, &event_data, &self.database).await {
                    reply_context.send_error_embed("Error getting to do-list", e).await;
                }
            },
            other => {
                info!("Unknown command received: {}", other);
                reply_context.send_error_embed("Unknown command", UnknownCommand(String::from(other))).await;
            }
        }
    }
}

#[async_trait]
impl EventHandler for ThreadTrackerBot {
    async fn message(&self, context: Context, msg: Message) {
        let user_id = msg.author.id;
        let channel_id = msg.channel_id;
        let guild_id = if let Ok(Channel::Guild(guild_channel)) = channel_id.to_channel(&context.http).await {
            guild_channel.guild_id
        }
        else {
            error!("Error: Not currently in a server.");

            ReplyContext::new(channel_id, &context.http)
                .send_error_embed("No direct messages please", "Sorry, Titi is only designed to work in a server currently.").await;
            return;
        };

        let event_data = EventData { user_id, guild_id, channel_id, context };

        if !msg.content.starts_with("tt!") {
            return;
        }

        if let Some(command) = msg.content.split_ascii_whitespace().next() {
            info!("[command] processing command `{}` from user `{}`", msg.content, user_id);
            self.process_command(event_data, command, msg.content[command.len()..].trim_start()).await;
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);

        run_periodic_tasks(ctx.into(), self.database.clone().into()).await;
    }
}

async fn help_message(reply_context: ReplyContext) {
    log_send_errors(reply_context.send_message_embed("Thread Tracker help", HELP_MESSAGE).await);
}

fn error_on_additional_arguments(unrecognised_args: Vec<&str>) -> Result<(), CommandError> {
    if !unrecognised_args.is_empty() {
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

    let bot = ThreadTrackerBot { database: pool };
    let client = Client::builder(&token, intents)
        .event_handler(bot)
        .await
        .expect("Err creating client");

    client.cache_and_http.cache.set_max_messages(1);

    Ok(client.into())
}
