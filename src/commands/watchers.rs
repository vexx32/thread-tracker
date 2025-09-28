use anyhow::anyhow;
use chrono::Utc;
use serenity::{
    builder::{CreateEmbed, CreateEmbedFooter, EditMessage, EditThread},
    http::CacheHttp,
    model::{prelude::*, Colour},
    utils::{EmbedMessageBuilding, MessageBuilder},
};
use tokio::time::Instant;
use tracing::{error, info, warn};

use super::CommandResult;
use crate::{
    cache::MessageCache,
    commands::{
        muses,
        threads::{self, show_timestamps, UserData},
        todos, CommandContext,
    },
    db::{self, ThreadWatcher, Todo, TrackedThread},
    messaging::{reply, whisper},
    utils::get_channel_name,
    CommandError, Database,
};

/// List currently tracked watchers.
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
        match db::list_current_watchers(&data.database, user.id.get(), guild_id.get()).await {
            Ok(results) => results,
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
    let guild_id = match ctx.guild_id() {
        Some(id) => id,
        None => return Err(CommandError::new("Unable to manage watchers outside of a server")),
    };

    ctx.defer().await?;

    let user = ctx.author();

    let data = ctx.data();

    info!(
        "adding watcher for {} ({}), categories {:?}",
        user.name, user.id, category
    );
    let list = threads::get_threads_and_todos(user, guild_id, category.as_deref(), None, data, &ctx).await?;

    if list.chars().count() > crate::consts::MAX_EMBED_CHARS {
        return Err(CommandError::new(
            "Watched messages cannot span multiple messages. Please use categories to reduce the threads the new watcher must track."
        ));
    } else if list.is_empty() {
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
        user.id.get(),
        watcher_message_id.get(),
        channel_id.get(),
        guild_id.get(),
        category.as_deref(),
    )
    .await;

    match result {
        Ok(true) => {
            whisper(&ctx, "Watcher created", "The requested watcher has been created.").await?;

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
        match db::get_watcher(database, watched_message.channel_id.get(), watched_message.id.get()).await {
            Ok(Some(w)) => w,
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

    if watcher.user_id() != user.id {
        return Err(CommandError::new("You can only remove watchers that you created."));
    }

    info!(
        "removing watcher for {} ({}), (channel: {}, message: {})",
        user.name, user.id, watched_message.channel_id, watched_message.id
    );

    match db::remove_watcher(database, watcher.id).await? {
        0 => error!(
            "Watcher should have been present in the database, but was missing when removal was attempted: {:?}",
            watcher
        ),
        _ => {
            let channel_message = watcher.message();
            let message = message_cache
                .get_or_else(&channel_message, || channel_message.fetch(&ctx))
                .await;

            match message {
                Ok(message) => {
                    if let Err(e) = message.delete(ctx).await {
                        return Err(anyhow!("Unable to delete watched message ({}): {}", message_url, e).into());
                    }
                },
                Err(e) => {
                    return Err(anyhow!("Unable to locate message {}. Perhaps it was already deleted?", e).into())
                },
            }

            whisper(
                &ctx,
                "Watcher removed",
                &format!("Watcher with id {} removed successfully.", watcher.id),
            )
            .await?;
        },
    }

    Ok(())
}

pub(crate) async fn update_watched_message(
    watcher: ThreadWatcher,
    cache_http: impl CacheHttp,
    database: &Database,
    message_cache: &MessageCache,
) -> anyhow::Result<()> {
    info!("updating watched message for {:?}", &watcher);
    let start_time = Instant::now();

    let mut message = match cache_http
        .http()
        .get_message(watcher.channel_id.into(), watcher.message_id.into())
        .await
    {
        Ok(m) => m,
        Err(e) => {
            let channel_name = get_channel_name(watcher.channel_id(), cache_http)
                .await
                .unwrap_or_else(|| "<unavailable channel>".to_owned());

            if cfg!(debug_assertions) {
                warn!(
                    "could not find message {} in channel {} for watcher {}: {}.",
                    watcher.message_id, channel_name, watcher.id, e
                );
            } else {
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

    if let Some(mut channel) = message.channel(&cache_http).await?.guild() {
        // If this is a thread, there will be thread metadata
        if let Some(metadata) = channel.thread_metadata {
            if metadata.archived {
                channel
                    .edit_thread(&cache_http, EditThread::new().archived(false))
                    .await?;
            }
        }
    }

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

    let user_data = UserData {
        id: user.user_id,
        guild_id: user.guild_id,
        muses: muses::get_list(database, user.user_id, user.guild_id).await?,
        show_timestamps: show_timestamps(database, user.user_id).await,
    };

    let threads_content =
        threads::get_formatted_list(threads, todos, None, &cache_http, message_cache, &user_data).await?;

    let edit_result = message
        .edit(
            &cache_http,
            EditMessage::new().add_embed(
                CreateEmbed::new()
                    .colour(Colour::PURPLE)
                    .title("Watching threads")
                    .description(threads_content)
                    .footer(CreateEmbedFooter::new(format!("Last updated: {} UTC", Utc::now()))),
            ),
        )
        .await;
    if let Err(e) = edit_result {
        // If we return here, an error updating one watcher message would prevent the rest from being updated.
        // Simply log these instead.
        error!("Could not edit message: {}", e);
    } else {
        let elapsed = Instant::now() - start_time;
        info!("updated watcher {} in {:.2} ms", watcher.id, elapsed.as_millis());
    }

    Ok(())
}
