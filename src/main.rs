use std::sync::Arc;

use anyhow::anyhow;
use cache::MessageCache;
use serenity::{
    async_trait,
    model::{
        channel::Message,
        prelude::*,
    },
    prelude::*,
};
use shuttle_secrets::SecretStore;
use sqlx::Executor;
use thiserror::Error;
use tracing::{error, info};

mod cache;
mod db;
mod background_tasks;
mod threads;
mod messaging;
mod muses;
mod todos;
mod utils;
mod watchers;

use background_tasks::*;
use db::Database;
use messaging::*;
use utils::{EventData, error_on_additional_arguments};

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
Finds a random tracked thread that was last replied to by someone other than you. Optionally, provide a category name to limit the selection to that category.

`tt!todos` // `tt!todolist`
List all to do-list entries.

`tt!todo`
Adds a to do-list item. Optionally specify a category as `!categoryname` before the to do-list entry itself, for example: `tt!todo !mycategory do the thing`

`tt!done`
Crosses off and removes a to do-list item. Add `!categoryname` to remove all entries from that category, or `!all` to remove all to do entries.

Titi's responses can be deleted by the user that triggered the request reacting with :no_entry_sign: or :wastebasket: — this will not work if the message that Titi's responding to has been deleted.
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

/// Primary bot state struct that will be passed to all event handlers.
struct ThreadTrackerBot
{
    /// Postgres database pool
    database: Database,
    message_cache: MessageCache,
    user_id: Arc<RwLock<Option<UserId>>>,
}

impl ThreadTrackerBot {
    /// Create a new bot instance.
    fn new(database: Database) -> Self {
        Self {
            database,
            message_cache: MessageCache::new(),
            user_id: Arc::new(RwLock::new(None)),
        }
    }

    async fn set_user(&self, id: UserId) {
        let mut guard = self.user_id.write().await;
        *guard = Some(id);
    }

    async fn user(&self) -> Option<UserId> {
        *self.user_id.read().await
    }

    /// Handles processing commands received by the bot.
    ///
    /// ### Arguments
    ///
    /// - `event_data` - information about the context and metadata of the message that triggered the command.
    /// - `command` - string slice containing the command keyword, which should start with `tt!`
    /// - `args` - string slice containing the rest of the message that follows the command
    async fn process_command(&self, event_data: EventData, command: &str, args: &str) {
        let reply_context = event_data.reply_context();
        match command {
            "tt!help" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = error_on_additional_arguments(args) {
                    reply_context.send_error_embed("Too many arguments", e, &self.message_cache).await;
                };

                help_message(reply_context, &self.message_cache).await;
            },
            "tt!add" | "tt!track" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = threads::add(args, &event_data, self).await {
                    reply_context.send_error_embed("Error adding tracked channel(s): {:}", e, &self.message_cache).await;
                }
            },
            "tt!cat" | "tt!category" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = threads::set_category(args, &event_data, self).await {
                    reply_context.send_error_embed("Error updating channels' categories", e, &self.message_cache).await;
                }
            },
            "tt!rm" | "tt!remove" | "tt!untrack" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = threads::remove(args, &event_data, self).await {
                    reply_context.send_error_embed("Error removing tracked channel(s)", e, &self.message_cache).await;
                }
            },
            "tt!replies" | "tt!threads" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = threads::send_list(args, &event_data, self).await {
                    reply_context.send_error_embed("Error retrieving thread list", e, &self.message_cache).await;
                }
            },
            "tt!random" | "tt!rng" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = threads::send_random_thread(args, &event_data, self).await {
                    reply_context.send_error_embed("Error retrieving a random thread", e, &self.message_cache).await;
                }
            },
            "tt!watch" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = watchers::add(args, &event_data, self).await {
                    reply_context.send_error_embed("Error adding watcher", e, &self.message_cache).await;
                }
            },
            "tt!unwatch" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = watchers::remove(args, &event_data, self).await {
                    reply_context.send_error_embed("Error removing watcher", e, &self.message_cache).await;
                }
            },
            "tt!muses" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = error_on_additional_arguments(args) {
                    reply_context.send_error_embed("Too many arguments", e, &self.message_cache).await;
                }

                if let Err(e) = muses::send_list(&event_data, self).await {
                    reply_context.send_error_embed("Error finding muses", e, &self.message_cache).await;
                }
            },
            "tt!addmuse" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = muses::add(args, &event_data, self).await {
                    reply_context.send_error_embed("Error adding muse", e, &self.message_cache).await;
                }
            },
            "tt!removemuse" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = muses::remove(args, &event_data, self).await {
                    reply_context.send_error_embed("Error removing muse", e, &self.message_cache).await;
                }
            },
            "tt!todo" => {
                if let Err(e) = todos::add(args, &event_data, self).await {
                    reply_context.send_error_embed("Error adding to do-list item", e, &self.message_cache).await;
                }
            },
            "tt!done" => {
                if let Err(e) = todos::remove(args, &event_data, self).await {
                    reply_context.send_error_embed("Error removing to do-list item", e, &self.message_cache).await;
                }
            },
            "tt!todos" | "tt!todolist" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = todos::send_list(args, &event_data, self).await {
                    reply_context.send_error_embed("Error getting to do-list", e, &self.message_cache).await;
                }
            },
            other => {
                info!("Unknown command received: {}", other);
                reply_context.send_error_embed("Unknown command", UnknownCommand(String::from(other)), &self.message_cache).await;
            }
        }
    }
}

#[async_trait]
impl EventHandler for ThreadTrackerBot {
    async fn reaction_add(&self, context: Context, reaction: Reaction)  {
        const DELETE_EMOJI: [&str; 2] = ["🚫", "🗑️"];

        let bot_user = self.user().await;
        if reaction.user_id == bot_user {
            // Ignore reactions made by the bot user
            return;
        }

        info!("Received reaction {} on message {}", reaction.emoji, reaction.message_id);

        if DELETE_EMOJI.iter().any(|emoji| reaction.emoji.unicode_eq(emoji)) {
            info!("Deletion action recognised from reaction");

            let channel_message = (reaction.channel_id, reaction.message_id).into();
            if let Ok(message) = self.message_cache.get_or_else(&channel_message, || channel_message.fetch(&context)).await {
                if Some(message.author.id) != bot_user {
                    // Ignore reactions to messages not sent by the bot.
                    return;
                }

                if let Some(referenced_message) = &message.referenced_message {
                    if Some(referenced_message.author.id) == reaction.user_id {
                        if let Err(e) = message.delete(&context).await {
                            error!("Unable to delete message {:?}: {}", message, e);
                        }
                        else {
                            info!("Message deleted successfully!");
                            self.message_cache.remove(&channel_message).await;
                        }
                    }
                }
                else {
                    error!("Could not find referenced message to check requesting user ID against")
                }
            }
        }
    }

    async fn message(&self, context: Context, msg: Message) {
        let user_id = msg.author.id;

        if Some(user_id) == self.user().await  {
            return;
        }

        let message_id = msg.id;
        let channel_id = msg.channel_id;
        let guild_id = if let Ok(Channel::Guild(guild_channel)) = channel_id.to_channel(&context.http).await {
            guild_channel.guild_id
        }
        else {
            error!("Error: Not currently in a server.");

            ReplyContext::new(channel_id, message_id, &context.http)
                .send_error_embed("No direct messages please", "Sorry, Titi is only designed to work in a server currently.", &self.message_cache).await;
            return;
        };

        let event_data = EventData { user_id, guild_id, channel_id, message_id, context };

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

        self.set_user(ready.user.id).await;

        run_periodic_tasks(ctx.into(), self).await;
    }
}

/// Sends the bot's help message to the channel.
///
/// ### Arguments
///
/// - `reply_context` - the bot context and channel to reply to
async fn help_message(reply_context: ReplyContext, message_cache: &MessageCache) {
    handle_send_result(reply_context.send_message_embed("Thread Tracker help", HELP_MESSAGE), message_cache).await;
}

#[shuttle_runtime::main]
async fn serenity(
    #[shuttle_shared_db::Postgres(
        //local_uri = "postgres://postgres:{secrets.PASSWORD}@localhost:16695/postgres"
    )] database: Database,
    #[shuttle_secrets::Secrets] secret_store: SecretStore,
) -> shuttle_serenity::ShuttleSerenity {
    use anyhow::Context;

    // Get the discord token set in `Secrets.toml`
    let token = if let Some(token) = secret_store.get("DISCORD_TOKEN") {
        token
    }
    else {
        return Err(anyhow!("'DISCORD_TOKEN' was not found").into());
    };

    // Run the schema migration
    database.execute(include_str!("../schema.sql"))
        .await
        .context("failed to run migrations")?;

    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_MESSAGE_REACTIONS
        | GatewayIntents::DIRECT_MESSAGES;

    let bot = ThreadTrackerBot::new(database);
    let client = Client::builder(&token, intents)
        .event_handler(bot)
        .await
        .expect("Err creating client");

    client.cache_and_http.cache.set_max_messages(1);

    Ok(client.into())
}
