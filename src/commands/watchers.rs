use chrono::Utc;
use serenity::{
    builder::CreateApplicationCommands,
    model::prelude::{
        command::{CommandOptionType, CommandType},
        interaction::application_command::ApplicationCommandInteraction,
        *,
    },
    prelude::*,
    utils::{Colour, EmbedMessageBuilding, MessageBuilder},
};
use thiserror::*;
use tokio::time::Instant;
use tracing::{error, info, warn};
use WatcherError::*;

use crate::{
    cache::MessageCache,
    commands::{
        muses,
        threads::{self, TrackedThread},
        todos::{self, Todo},
    },
    db,
    messaging::{InteractionResponse, ReplyContext},
    utils::{find_string_option, get_channel_name, ChannelMessage, GuildUser},
    Database,
    ThreadTrackerBot,
};

type Result<T> = std::result::Result<T, WatcherError>;

/// Stores all necessary information for updating watched thread lists.
#[derive(Debug)]
pub(crate) struct ThreadWatcher {
    pub message_id: MessageId,
    pub channel_id: ChannelId,
    pub guild_id: GuildId,
    pub user_id: UserId,
    pub id: i32,
    pub categories: Option<String>,
}

impl ThreadWatcher {
    /// Get the guild and user for this thread watcher.
    pub fn user(&self) -> GuildUser {
        self.into()
    }

    /// Get the channel and message for this thread watcher.
    pub fn message(&self) -> ChannelMessage {
        (self.channel_id, self.message_id).into()
    }
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

/// Errors encountered while handling watchers.
#[derive(Error, Debug)]
enum WatcherError {
    #[error("Error fetching watcher: {0}")]
    NotFound(String),
    // #[error("Not allowed: {0}")]
    // NotAllowed(String),
}

pub fn register_commands(
    commands: &mut CreateApplicationCommands,
) -> &mut CreateApplicationCommands {
    commands
        .create_application_command(|command| {
            command
                .name("tt_watch")
                .description(
                    "Get the list of tracked threads and have Titi periodically update the list",
                )
                .kind(CommandType::ChatInput)
                .create_option(|option| {
                    option
                        .name("category")
                        .description("The specific category or categories to list threads from")
                        .kind(CommandOptionType::String)
                })
        })
        .create_application_command(|command| {
            command
                .name("tt_unwatch")
                .description("Stop updating a watched thread list and delete the message")
                .kind(CommandType::ChatInput)
                .create_option(|option| {
                    option
                        .name("message_link")
                        .description("A link to the watched message")
                        .kind(CommandOptionType::String)
                        .required(true)
                })
        })
        .create_application_command(|command| {
            command
                .name("tt_watching")
                .description("Show all watched messages Titi is tracking for you")
                .kind(CommandType::ChatInput)
        })
}

/// List current watchers.
///
/// ### Arguments
///
/// - `command` - the slash command interaction data
/// - `bot` - the bot instance
pub(crate) async fn list(
    command: &ApplicationCommandInteraction,
    bot: &ThreadTrackerBot,
) -> Vec<InteractionResponse> {
    info!("listing watchers for {} ({})", command.user.name, command.user.id);
    const ERROR_TITLE: &str = "Error listing watchers";

    let guild_id = match command.guild_id {
        Some(id) => id,
        None => {
            return InteractionResponse::error(
                ERROR_TITLE,
                "Unable to manage watchers outside of a server",
            )
        },
    };

    let watchers: Vec<ThreadWatcher> =
        match db::list_current_watchers(&bot.database, command.user.id.0, guild_id.0).await {
            Ok(results) => results.into_iter().map(|tw| tw.into()).collect(),
            Err(e) => {
                return InteractionResponse::error(
                    ERROR_TITLE,
                    format!("Unable to list watchers: {}", e),
                )
            },
        };

    let mut message = MessageBuilder::new();

    for watcher in watchers {
        let url = format!(
            "https://discord.com/channels/{}/{}/{}",
            watcher.guild_id, watcher.channel_id, watcher.message_id
        );
        message
            .push_quote("- Categories: ")
            .push(watcher.categories.as_deref().unwrap_or("All"))
            .push(" - ")
            .push_named_link("Link", url)
            .push_line("");
    }

    InteractionResponse::reply("Currently active watchers", message.build())
}

/// Add a new thread watcher and send the initial watcher message.
///
/// ### Arguments
///
/// - `command` - the slash command interaction data
/// - `bot` - the bot instance
/// - `context` - the interaction context
pub(crate) async fn add(
    command: &ApplicationCommandInteraction,
    bot: &ThreadTrackerBot,
    context: &Context,
) -> Vec<InteractionResponse> {
    const ERROR_TITLE: &str = "Error creating watcher";

    let guild_id = match command.guild_id {
        Some(id) => id,
        None => {
            return InteractionResponse::error(
                ERROR_TITLE,
                "Unable to manage watchers outside of a server",
            )
        },
    };

    let category = find_string_option(&command.data.options, "category");
    info!(
        "adding watcher for {} ({}), categories {:?}",
        command.user.name, command.user.id, category
    );
    let messages = threads::get_list_with_title(
        "Watching threads".into(),
        &command.user,
        guild_id,
        category,
        bot,
        context,
    )
    .await;

    if messages.len() > 1 {
        return InteractionResponse::error(ERROR_TITLE, "Watched messages cannot span multiple messages. Please use categories to reduce the threads the new watcher must track.");
    }
    else if messages.is_empty() {
        return InteractionResponse::error(ERROR_TITLE, "Could not create the watcher message.");
    }

    let message = messages.first().unwrap();
    let reply_context = ReplyContext::new(command.channel_id, context.clone());
    let watcher_message =
        match reply_context.send_message_embed(message.title(), message.content()).await {
            Ok(m) => m,
            Err(e) => {
                return InteractionResponse::error(
                    ERROR_TITLE,
                    format!("Error creating watched message: {}", e),
                )
            },
        };

    let result = db::add_watcher(
        &bot.database,
        command.user.id.0,
        watcher_message.id.0,
        command.channel_id.0,
        guild_id.0,
        category,
    )
    .await;

    match result {
        Ok(true) => InteractionResponse::ephemeral_reply(
            "Watcher created",
            "The requested watcher has been created.",
        ),
        Ok(false) => InteractionResponse::error(
            ERROR_TITLE,
            "Something went wrong storing the watcher information, the data was not recorded.",
        ),
        Err(e) => InteractionResponse::error(
            ERROR_TITLE,
            format!("Error recording the watcher information: {}", e),
        ),
    }
}

/// Removes a currently active watcher and deletes the watched message.
///
/// ### Arguments
///
/// - `command` - the slash command interaction data
/// - `bot` - the bot instance
/// - `context` - the interaction context
pub(crate) async fn remove(
    command: &ApplicationCommandInteraction,
    bot: &ThreadTrackerBot,
    context: &Context,
) -> Vec<InteractionResponse> {
    const ERROR_TITLE: &str = "Error removing watcher";
    let message_link_option = find_string_option(&command.data.options, "message_link");
    let (database, message_cache) = (&bot.database, &bot.message_cache);

    if let Some(message_url) = message_link_option {
        let (watcher_message_id, watcher_channel_id) = match parse_message_link(message_url) {
            Ok(data) => data,
            Err(_) => return InteractionResponse::error(
                ERROR_TITLE,
                format!(
                    "Error parsing message link, please verify that `{}` is a valid message URL",
                    message_url
                ),
            ),
        };

        let watcher: ThreadWatcher =
            match db::get_watcher(database, watcher_channel_id, watcher_message_id).await {
                Ok(Some(w)) => w.into(),
                Ok(None) => {
                    return InteractionResponse::error(
                        ERROR_TITLE,
                        format!(
                            "Could not find a watcher for the target message: `{}`",
                            message_url
                        ),
                    )
                },
                Err(e) => {
                    return InteractionResponse::error(
                        ERROR_TITLE,
                        format!(
                            "Error looking up watcher for (channel: {}, message: {}): {}",
                            watcher_channel_id, watcher_message_id, e
                        ),
                    )
                },
            };

        if watcher.user_id != command.user.id {
            return InteractionResponse::ephemeral_error(
                ERROR_TITLE,
                "You can only remove watchers that you created.",
            );
        }

        info!(
            "removing watcher for {} ({}), (channel: {}, message: {})",
            command.user.name, command.user.id, watcher_channel_id, watcher_message_id
        );

        let mut responses = Vec::new();
        match db::remove_watcher(database, watcher.id).await {
            Ok(0) => error!("Watcher should have been present in the database, but was missing when removal was attempted: {:?}", watcher),
            Ok(_) => {
                responses.extend(InteractionResponse::ephemeral_reply("Watcher removed", format!("Watcher with id {} removed successfully.", watcher.id)));
                let channel_message = watcher.message();
                let message = message_cache.get_or_else(
                    &channel_message,
                    || channel_message.fetch(context)
                ).await;

                match message {
                    Ok(message) => if let Err(e) = message.delete(context).await {
                        responses.extend(InteractionResponse::error(ERROR_TITLE, format!("Unable to delete watched message ({}): {}", message_url, e)));
                    }
                    Err(e) => responses.extend(InteractionResponse::ephemeral_error(
                        ERROR_TITLE,
                        format!("Unable to locate message {}. Perhaps it was already deleted?", e))),
                }
            },
            Err(e) => {
                responses.extend(InteractionResponse::error(ERROR_TITLE, format!("Error removing watcher: {}", e)));
            }
        }

        responses
    }
    else {
        error!("Missing required option `message_link` for tt_unwatch");
        Vec::new()
    }
}

pub(crate) async fn update_watched_message(
    watcher: ThreadWatcher,
    context: &Context,
    database: &Database,
    message_cache: &MessageCache,
) -> anyhow::Result<()> {
    info!("updating watched message for {:?}", &watcher);
    let start_time = Instant::now();

    let mut message =
        match context.http.get_message(watcher.channel_id.0, watcher.message_id.0).await {
            Ok(m) => m,
            Err(e) => {
                let channel_name = get_channel_name(watcher.channel_id, context)
                    .await
                    .unwrap_or_else(|| "<unavailable channel>".to_owned());

                if cfg!(debug_assertions) {
                    warn!(
                        "could not find message {} in channel {} for watcher {}: {}.",
                        watcher.message_id, channel_name, watcher.id, e
                    );
                }
                else {
                    warn!(
                        "could not find message {} in channel {} for watcher {}: {}. Removing watcher.",
                        watcher.message_id, channel_name, watcher.id, e
                    );
                    db::remove_watcher(database, watcher.id)
                        .await
                        .map_err(|e| error!("Failed to remove watcher: {}", e))
                        .ok();
                }

                return Ok(());
            },
        };

    let user = watcher.user();

    let mut threads: Vec<TrackedThread> = Vec::new();
    let mut todos: Vec<Todo> = Vec::new();

    match watcher.categories.as_deref() {
        Some("") | None => {
            threads.extend(threads::enumerate(database, &user, None).await?);
            todos.extend(todos::enumerate(database, &user, None).await?);
        },
        Some(cats) => {
            for category in cats.split(' ') {
                threads.extend(threads::enumerate(database, &user, Some(category)).await?);
                todos.extend(todos::enumerate(database, &user, Some(category)).await?);
            }
        },
    }

    let muses = muses::get_list(database, user.user_id, user.guild_id).await?;
    let threads_content =
        threads::get_formatted_list(threads, todos, muses, &watcher.user(), context, message_cache)
            .await?;

    let edit_result = message
        .edit(&context.http, |msg| {
            msg.embed(|embed| {
                embed
                    .colour(Colour::PURPLE)
                    .title("Watching threads")
                    .description(threads_content)
                    .footer(|footer| footer.text(format!("Last updated: {}", Utc::now())))
            })
        })
        .await;
    if let Err(e) = edit_result {
        // If we return here, an error updating one watcher message would prevent the rest from being updated.
        // Simply log these instead.
        error!("Could not edit message: {}", e);
    }
    else {
        let elapsed = Instant::now() - start_time;
        info!("updated watcher {} in {:.2} ms", watcher.id, elapsed.as_millis());
    }

    Ok(())
}

/// Parse a message link to retrieve the message and channel IDs.
fn parse_message_link(link: &str) -> Result<(u64, u64)> {
    let mut result: Vec<u64> = Vec::with_capacity(2);
    let message_url_fragments = link.split('/').rev().take(2).map(|s| s.parse().ok());

    for parsed in message_url_fragments {
        match parsed {
            Some(n) => result.push(n),
            None => return Err(NotFound(format!("Could not parse message ID from `{}`", link))),
        }
    }

    Ok((result[0], result[1]))
}
