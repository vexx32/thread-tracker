use anyhow::anyhow;
use chrono::Utc;
use serenity::{
    http::CacheHttp,
    model::prelude::*,
    utils::{Colour, EmbedMessageBuilding, MessageBuilder},
};
use tokio::time::Instant;
use tracing::{error, info, warn};

use super::CommandResult;
use crate::{
    cache::MessageCache,
    commands::{
        muses,
        threads::{self, TrackedThread},
        todos::{self, Todo}, CommandContext,
    },
    db,
    messaging::{reply, reply_ephemeral},
    utils::{get_channel_name, ChannelMessage, GuildUser},
    CommandError,
    Database,
};

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

/// List current watchers.
#[poise::command(slash_command, guild_only, rename = "tt_watchers", category = "Watchers")]
pub(crate) async fn list(ctx: CommandContext<'_>) -> CommandResult<()> {
    let user = ctx.author();
    info!("listing watchers for {} ({})", user.name, user.id);

    let data = ctx.data();

    let guild_id = match ctx.guild_id() {
        Some(id) => id,
        None => return Err(CommandError::new("Unable to manage watchers outside of a server")),
    };

    let watchers: Vec<ThreadWatcher> =
        match db::list_current_watchers(&data.database, user.id.0, guild_id.0).await {
            Ok(results) => results.into_iter().map(|tw| tw.into()).collect(),
            Err(e) => return Err(CommandError::detailed("Unable to list watchers", e)),
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

    reply(&ctx, "Currently active watchers", &message.build()).await?;

    Ok(())
}

/// Add a new thread watcher and send the initial watcher message.
#[poise::command(slash_command, guild_only, rename = "tt_watch", category = "Watchers")]
pub(crate) async fn add(
    ctx: CommandContext<'_>,
    #[description = "The category to filter the watched threads by"] category: Option<String>,
) -> CommandResult<()> {
    let user = ctx.author();

    let guild_id = match ctx.guild_id() {
        Some(id) => id,
        None => return Err(CommandError::new("Unable to manage watchers outside of a server")),
    };

    let data = ctx.data();

    info!("adding watcher for {} ({}), categories {:?}", user.name, user.id, category);
    let list = threads::get_list(user, guild_id, category.as_deref(), data, ctx.serenity_context())
        .await?;

    if list.len() > crate::consts::MAX_EMBED_CHARS {
        return Err(CommandError::new(
            "Watched messages cannot span multiple messages. Please use categories to reduce the threads the new watcher must track."
        ));
    }
    else if list.is_empty() {
        return Err(CommandError::new("Could not create the watcher message."));
    }

    let channel_id = ctx.channel_id();

    let reply_handle = reply(&ctx, "Watching threads", &list).await?.pop();
    let watcher_message_id = match reply_handle {
        Some(handle) => handle.message().await?.id,
        None => return Err(CommandError::new("Failed to create watcher message")),
    };

    let result = db::add_watcher(
        &data.database,
        user.id.0,
        watcher_message_id.0,
        channel_id.0,
        guild_id.0,
        category.as_deref(),
    )
    .await;

    match result {
        Ok(true) => {
            reply_ephemeral(&ctx, "Watcher created", "The requested watcher has been created.")
                .await?;

            Ok(())
        },
        Ok(false) => Err(CommandError::new(
            "Something went wrong storing the watcher information, the data was not recorded.",
        )),
        Err(e) => Err(CommandError::detailed("Error recording the watcher information", e)),
    }
}

/// Removes a currently active watcher and deletes the watched message.
#[poise::command(slash_command, guild_only, rename = "tt_unwatch", category = "Watchers")]
pub(crate) async fn remove(
    ctx: CommandContext<'_>,
    #[description = "The watched message (enter a link or message ID)"] watched_message: Message,
) -> CommandResult<()> {
    let data = ctx.data();
    let (database, message_cache) = (&data.database, &data.message_cache);

    let user = ctx.author();
    let message_url = watched_message.link();

    let watcher: ThreadWatcher =
        match db::get_watcher(database, watched_message.channel_id.0, watched_message.id.0).await {
            Ok(Some(w)) => w.into(),
            Ok(None) => {
                return Err(CommandError::new(format!(
                    "Could not find a watcher for the target message: `{}`",
                    message_url
                )))
            },
            Err(e) => {
                return Err(CommandError::detailed(
                    format!(
                        "Error looking up watcher for (channel: {}, message: {})",
                        watched_message.channel_id, watched_message.id,
                    ),
                    e,
                ))
            },
        };

    if watcher.user_id != user.id {
        return Err(CommandError::new("You can only remove watchers that you created."));
    }

    info!(
        "removing watcher for {} ({}), (channel: {}, message: {})",
        user.name, user.id, watched_message.channel_id, watched_message.id
    );

    match db::remove_watcher(database, watcher.id).await? {
        0 => error!("Watcher should have been present in the database, but was missing when removal was attempted: {:?}", watcher),
        _ => {
            let channel_message = watcher.message();
            let message = message_cache.get_or_else(
                &channel_message,
                || channel_message.fetch(&ctx)
            ).await;

            match message {
                Ok(message) => if let Err(e) = message.delete(ctx).await {
                    return Err(anyhow!("Unable to delete watched message ({}): {}", message_url, e).into());
                }
                Err(e) => return Err(anyhow!("Unable to locate message {}. Perhaps it was already deleted?", e).into()),
            }

            reply_ephemeral(&ctx, "Watcher removed", &format!("Watcher with id {} removed successfully.", watcher.id)).await?;
        }
    }

    Ok(())
}

pub(crate) async fn update_watched_message(
    watcher: ThreadWatcher,
    cache_http: &impl CacheHttp,
    database: &Database,
    message_cache: &MessageCache,
) -> anyhow::Result<()> {
    info!("updating watched message for {:?}", &watcher);
    let start_time = Instant::now();

    let mut message =
        match cache_http.http().get_message(watcher.channel_id.0, watcher.message_id.0).await {
            Ok(m) => m,
            Err(e) => {
                let channel_name = get_channel_name(watcher.channel_id, cache_http)
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
    let threads_content = threads::get_formatted_list(
        threads,
        todos,
        muses,
        &watcher.user(),
        cache_http,
        message_cache,
    )
    .await?;

    let edit_result = message
        .edit(&cache_http, |msg| {
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
