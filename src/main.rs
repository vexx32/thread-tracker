use std::sync::Arc;

use anyhow::anyhow;
use cache::MessageCache;
use serenity::{
    async_trait,
    model::{channel::Message, prelude::*},
    prelude::*,
};
use shuttle_secrets::SecretStore;
use sqlx::Executor;
use thiserror::Error;
use tracing::{debug, error, info};

mod background_tasks;
mod cache;
mod consts;
mod db;
mod messaging;
mod muses;
mod stats;
mod threads;
mod todos;
mod utils;
mod watchers;

use background_tasks::*;
use consts::*;
use db::Database;
use messaging::*;
use utils::{error_on_additional_arguments, message_is_command, EventData};

/// Command parsing errors
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
struct ThreadTrackerBot {
    /// Postgres database pool
    database: Database,
    /// Threadsafe memory cache for messages the bot has sent or looked up
    message_cache: MessageCache,
    /// The bot's current user id
    user_id: Arc<RwLock<Option<UserId>>>,
}

impl ThreadTrackerBot {
    /// Create a new bot instance.
    ///
    /// ### Arguments
    ///
    /// - `database` - database pool connection
    fn new(database: Database) -> Self {
        Self { database, message_cache: MessageCache::new(), user_id: Arc::new(RwLock::new(None)) }
    }

    /// Sets the current user ID for the bot
    ///
    /// ### Arguments
    ///
    /// - `id` - the UserId
    async fn set_user(&self, id: UserId) {
        let mut guard = self.user_id.write().await;
        *guard = Some(id);
    }

    /// Gets the current user ID for the bot, if it's been set
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
        match command.to_lowercase().as_str() {
            "tt!add" | "tt!track" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = threads::add(args, &event_data, self).await {
                    reply_context
                        .send_error_embed("Error adding tracked channel(s)", e, &self.message_cache)
                        .await;
                }
            },
            "tt!cat" | "tt!category" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = threads::set_category(args, &event_data, self).await {
                    reply_context
                        .send_error_embed(
                            "Error updating channels' categories",
                            e,
                            &self.message_cache,
                        )
                        .await;
                }
            },
            "tt!remove" | "tt!untrack" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = threads::remove(args, &event_data, self).await {
                    reply_context
                        .send_error_embed(
                            "Error removing tracked channel(s)",
                            e,
                            &self.message_cache,
                        )
                        .await;
                }
            },
            "tt!replies" | "tt!threads" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = threads::send_list(args, &event_data, self).await {
                    reply_context
                        .send_error_embed("Error retrieving thread list", e, &self.message_cache)
                        .await;
                }
            },
            "tt!random" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = threads::send_random_thread(args, &event_data, self).await {
                    reply_context
                        .send_error_embed(
                            "Error retrieving a random thread",
                            e,
                            &self.message_cache,
                        )
                        .await;
                }
            },
            "tt!watch" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = watchers::add(args, &event_data, self).await {
                    reply_context
                        .send_error_embed("Error adding watcher", e, &self.message_cache)
                        .await;
                }
            },
            "tt!unwatch" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = watchers::remove(args, &event_data, self).await {
                    reply_context
                        .send_error_embed("Error removing watcher", e, &self.message_cache)
                        .await;
                }
            },
            "tt!muses" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = error_on_additional_arguments(args) {
                    reply_context
                        .send_error_embed("Too many arguments", e, &self.message_cache)
                        .await;
                }

                if let Err(e) = muses::send_list(&event_data, self).await {
                    reply_context
                        .send_error_embed("Error finding muses", e, &self.message_cache)
                        .await;
                }
            },
            "tt!addmuse" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = muses::add(args, &event_data, self).await {
                    reply_context
                        .send_error_embed("Error adding muse", e, &self.message_cache)
                        .await;
                }
            },
            "tt!removemuse" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = muses::remove(args, &event_data, self).await {
                    reply_context
                        .send_error_embed("Error removing muse", e, &self.message_cache)
                        .await;
                }
            },
            "tt!todo" => {
                if let Err(e) = todos::add(args, &event_data, self).await {
                    reply_context
                        .send_error_embed("Error adding to do-list item", e, &self.message_cache)
                        .await;
                }
            },
            "tt!done" => {
                if let Err(e) = todos::remove(args, &event_data, self).await {
                    reply_context
                        .send_error_embed("Error removing to do-list item", e, &self.message_cache)
                        .await;
                }
            },
            "tt!todos" | "tt!todolist" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = todos::send_list(args, &event_data, self).await {
                    reply_context
                        .send_error_embed("Error getting to do-list", e, &self.message_cache)
                        .await;
                }
            },
            "tt!stats" => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = error_on_additional_arguments(args) {
                    reply_context
                        .send_error_embed("Too many arguments", e, &self.message_cache)
                        .await;
                }

                if let Err(e) = stats::send_statistics(&reply_context, self).await {
                    reply_context
                        .send_error_embed("Error fetching statistics", e, &self.message_cache)
                        .await
                }
            },
            cmd => match HelpMessage::from_command(cmd) {
                Some(help_message) => {
                    let args = args.split_ascii_whitespace().collect();
                    if let Err(e) = error_on_additional_arguments(args) {
                        reply_context
                            .send_error_embed("Too many arguments", e, &self.message_cache)
                            .await;
                    };

                    reply_context.send_help(help_message, &self.message_cache).await;
                },
                None => {
                    send_unknown_command(&reply_context, command, &self.message_cache).await;
                },
            },
        }
    }

    async fn process_direct_message(
        &self,
        user_id: UserId,
        reply_context: ReplyContext,
        message: Message,
    ) {
        if user_id == DEBUG_USER {
            handle_send_result(
                reply_context.send_message_embed("Debug information", format!("{:?}", message)),
                &self.message_cache,
            )
            .await;
        }
        else {
            reply_context
                .send_error_embed(
                    "No direct messages please",
                    "Sorry, Titi is only designed to work in a server currently.",
                    &self.message_cache,
                )
                .await;
        }
    }
}

#[async_trait]
impl EventHandler for ThreadTrackerBot {
    async fn reaction_add(&self, context: Context, reaction: Reaction) {
        let bot_user = self.user().await;
        if reaction.user_id == bot_user {
            // Ignore reactions made by the bot user
            return;
        }

        info!("Received reaction {} on message {}", reaction.emoji, reaction.message_id);

        if DELETE_EMOJI.iter().any(|emoji| reaction.emoji.unicode_eq(emoji)) {
            info!("Deletion action recognised from reaction");

            let channel_message = (reaction.channel_id, reaction.message_id).into();
            if let Ok(message) = self
                .message_cache
                .get_or_else(&channel_message, || channel_message.fetch(&context))
                .await
            {
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

    async fn message(&self, context: Context, message: Message) {
        if !message_is_command(&message.content) {
            return;
        }

        let user_id = message.author.id;
        if Some(user_id) == self.user().await {
            return;
        }

        let message_id = message.id;
        let channel_id = message.channel_id;

        if let Ok(Channel::Guild(guild_channel)) = channel_id.to_channel(&context.http).await {
            let guild_id = guild_channel.guild_id;
            let event_data = EventData { user_id, guild_id, channel_id, message_id, context };

            if let Some(command) = message.content.split_ascii_whitespace().next() {
                info!("[command] processing command `{}` from user `{}`", message.content, user_id);
                self.process_command(
                    event_data,
                    command,
                    message.content[command.len()..].trim_start(),
                )
                .await;
            }
        }
        else {
            let reply_context = ReplyContext::new(channel_id, message_id, &context.http);
            self.process_direct_message(user_id, reply_context, message).await;
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);

        self.set_user(ready.user.id).await;

        run_periodic_tasks(ctx.into(), self).await;
    }
}

#[shuttle_runtime::main]
async fn serenity(
    #[shuttle_shared_db::Postgres(
        //local_uri = "postgres://postgres:{secrets.PASSWORD}@localhost:16695/postgres"
    )]
    database: Database,
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
    database
        .execute(include_str!("../sql/schema.sql"))
        .await
        .context("failed to run migrations")?;

    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_MESSAGE_REACTIONS
        | GatewayIntents::DIRECT_MESSAGES;

    let bot = ThreadTrackerBot::new(database);
    let client =
        Client::builder(&token, intents).event_handler(bot).await.expect("Err creating client");

    client.cache_and_http.cache.set_max_messages(1);

    Ok(client.into())
}
