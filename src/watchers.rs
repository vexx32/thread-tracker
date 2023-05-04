use std::time::Instant;

use chrono::Utc;
use serenity::{
    model::prelude::*,
    prelude::*,
    utils::{Colour, EmbedMessageBuilding, MessageBuilder},
};
use thiserror::Error;
use tracing::{error, info, warn};
use WatcherError::*;

use crate::{
    cache::MessageCache,
    commands::CommandError::*,
    db::{self, Database},
    messaging::handle_send_result,
    muses,
    threads::{self, TrackedThread},
    todos::{self, Todo},
    utils::{get_channel_name, ChannelMessage, GuildUser},
    EventData,
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
    #[error("Not allowed: {0}")]
    NotAllowed(String),
}

pub(crate) async fn list(event_data: &EventData, bot: &ThreadTrackerBot) -> anyhow::Result<()> {
    info!("listing watchers for {}", event_data.log_user());

    let watchers: Vec<ThreadWatcher> =
        db::list_current_watchers(&bot.database, event_data.user.id.0, event_data.guild_id.0)
            .await?
            .into_iter()
            .map(|tw| tw.into())
            .collect();

    let mut message = MessageBuilder::new();

    for watcher in watchers {
        let url = format!(
            "https://discord.com/channels/{}/{}/{}",
            watcher.guild_id, watcher.channel_id, watcher.message_id
        );
        message
            .push_quote("• Categories: ")
            .push(watcher.categories.as_deref().unwrap_or("All"))
            .push(" - ")
            .push_named_link("Link", url)
            .push_line("");
    }

    handle_send_result(
        event_data.reply_context().send_message_embed("Currently active watchers", message),
        &bot.message_cache,
    )
    .await;

    Ok(())
}

/// Add a new thread watcher and send the initial watcher message.
///
/// ### Arguments
///
/// - `args` - the command arguments
/// - `event_data` - the event data
/// - `bot` - the bot instance
pub(crate) async fn add(
    args: Vec<&str>,
    event_data: &EventData,
    bot: &ThreadTrackerBot,
) -> anyhow::Result<()> {
    info!("adding watcher for {}, categories {:?}", event_data.log_user(), args);
    let arguments = if !args.is_empty() { Some(args.join(" ")) } else { None };

    let message = threads::send_list_with_title(args, "Watching threads", event_data, bot).await?;
    db::add_watcher(
        &bot.database,
        event_data.user.id.0,
        message.id.0,
        event_data.channel_id.0,
        event_data.guild_id.0,
        arguments.as_deref(),
    )
    .await?;

    Ok(())
}

/// Removes a currently active watcher and deletes the watched message.
///
/// ### Arguments
///
/// - `args` - the command arguments
/// - `event_data` - the event data
/// - `bot` - the bot instance
pub(crate) async fn remove(
    args: Vec<&str>,
    event_data: &EventData,
    bot: &ThreadTrackerBot,
) -> anyhow::Result<()> {
    let mut args = args.into_iter().peekable();
    if args.peek().is_none() {
        return Err(MissingArguments(String::from("Please provide a message URL to a watcher message, such as: `tt!unwatch <message url>`.")).into());
    }

    info!("removing watcher for {}, categories {:?}", event_data.log_user(), args);

    let message_url = args.next().unwrap();
    let (watcher_message_id, watcher_channel_id) = parse_message_link(message_url)?;
    let (database, message_cache) = (&bot.database, &bot.message_cache);

    let watcher: ThreadWatcher =
        match db::get_watcher(database, watcher_channel_id, watcher_message_id).await? {
            Some(w) => w.into(),
            None => {
                return Err(NotFound(format!(
                    "Could not find a watcher for the target message: `{}`",
                    message_url
                ))
                .into())
            },
        };

    if watcher.user_id != event_data.user.id {
        return Err(
            NotAllowed(format!("User {} does not own the watcher.", event_data.user.id)).into()
        );
    }

    match db::remove_watcher(database, watcher.id).await? {
        0 => error!("Watcher should have been present in the database, but was missing when removal was attempted: {:?}", watcher),
        _ => {
            event_data.reply_context().send_success_embed("Watcher removed", "Watcher successfully removed.", message_cache).await;
            let channel_message = watcher.message();
            message_cache.get_or_else(
                &channel_message,
                || channel_message.fetch(event_data.http())
            ).await?
                .delete(event_data.http()).await?;
        }
    }

    Ok(())
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

                warn!(
                    "could not find message {} in channel {} for watcher {}: {}. Removing watcher.",
                    watcher.message_id, channel_name, watcher.id, e
                );
                db::remove_watcher(database, watcher.id)
                    .await
                    .map_err(|e| error!("Failed to remove watcher: {}", e))
                    .ok();
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

    let muses = muses::list(database, &user).await?;
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
