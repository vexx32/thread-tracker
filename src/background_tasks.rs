use std::{
    fmt::Display,
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::Utc;
use serenity::{model::prelude::*, prelude::*, utils::Colour};
use tracing::{error, info};

use crate::{
    cache::MessageCache,
    consts::*,
    db::{self, Database},
    muses,
    threads::{self, TrackedThread},
    todos::{self, Todo},
    watchers::ThreadWatcher,
    ThreadTrackerBot,
};

/// Core task spawning function. Creates a set of periodically recurring tasks on their own threads.
///
/// ### Arguments
///
/// - `context` - the Serenity context to delegate to tasks
/// - `bot` - the bot instance to delegate to tasks
pub(crate) async fn run_periodic_tasks(context: Arc<Context>, bot: &ThreadTrackerBot) {
    let c = Arc::clone(&context);
    spawn_task_loop(HEARTBEAT_INTERVAL, move || heartbeat(Arc::clone(&c)));

    let c = Arc::clone(&context);
    let d = Arc::new(bot.database.clone());
    let m = Arc::new(bot.message_cache.clone());
    spawn_result_task_loop(WATCHER_UPDATE_INTERVAL, move || {
        update_watchers(Arc::clone(&c), Arc::clone(&d), Arc::clone(&m))
    });

    let c = Arc::new(bot.message_cache.clone());
    spawn_task_loop(CACHE_TRIM_INTERVAL, move || purge_expired_cache_entries(Arc::clone(&c)));
}

/// Spawns a task which loops indefinitely, with a wait period between each iteration.
///
/// ### Arguments
///
/// - `period` - the length of time between each task run
/// - `task` - the Future representing the task to run
fn spawn_task_loop<F, Ft>(period: Duration, mut task: F)
where
    F: FnMut() -> Ft + Send + 'static,
    Ft: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(period);

        loop {
            interval.tick().await;
            task().await;
        }
    });
}

/// Spawns a task which loops indefinitely, with a wait period between each iteration. If the task
/// errors, ensures the error is logged.
///
/// ### Arguments
///
/// - `period` - the length of time between each task run
/// - `task` - the Future representing the task to run
fn spawn_result_task_loop<F, T, U, Ft>(period: Duration, mut task: F)
where
    F: FnMut() -> Ft + Send + 'static,
    Ft: Future<Output = Result<T, U>> + Send + 'static,
    U: Display,
{
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(period);

        loop {
            interval.tick().await;

            if let Err(e) = task().await {
                error!("Error running periodic task: {}", e);
            }
        }
    });
}

/// Performs a set_presence request to ensure the Activity is set correctly.
pub(crate) async fn heartbeat(ctx: Arc<Context>) {
    ctx.set_presence(Some(Activity::watching("over your threads (tt!help)")), OnlineStatus::Online)
        .await;
    info!("heartbeat set_presence request completed");
}

/// Updates all recorded watchers and edits their referenced messages with the new content.
pub(crate) async fn update_watchers(
    context: Arc<Context>,
    database: Arc<Database>,
    message_cache: Arc<MessageCache>,
) -> Result<(), anyhow::Error> {
    let watchers: Vec<ThreadWatcher> =
        db::list_watchers(&database).await?.into_iter().map(|w| w.into()).collect();

    for watcher in watchers {
        info!("updating watched message for {:?}", &watcher);
        let start_time = Instant::now();

        let mut message = match context
            .http
            .get_message(watcher.channel_id.0, watcher.message_id.0)
            .await
        {
            Ok(m) => m,
            Err(e) => {
                let channel_name = watcher
                    .channel_id
                    .to_channel(&context.http)
                    .await
                    .map_or(None, |c| c.guild())
                    .map_or_else(|| "<unavailable channel>".to_owned(), |gc| gc.name);

                error!(
                    "could not find message {} in channel {} for watcher {}: {}. Removing watcher.",
                    watcher.message_id, channel_name, watcher.id, e
                );
                db::remove_watcher(&database, watcher.id)
                    .await
                    .map_err(|e| error!("Failed to remove watcher: {}", e))
                    .ok();
                continue;
            },
        };

        let user = watcher.user();

        let mut threads: Vec<TrackedThread> = Vec::new();
        let mut todos: Vec<Todo> = Vec::new();

        match watcher.categories.as_deref() {
            Some("") | None => {
                threads.extend(threads::enumerate(&database, &user, None).await?);
                todos.extend(todos::enumerate(&database, &user, None).await?);
            },
            Some(cats) => {
                for category in cats.split(' ') {
                    threads.extend(threads::enumerate(&database, &user, Some(category)).await?);
                    todos.extend(todos::enumerate(&database, &user, Some(category)).await?);
                }
            },
        }

        let muses = muses::list(&database, &user).await?;
        let threads_content = threads::get_formatted_list(
            threads,
            todos,
            muses,
            &watcher.user(),
            &context,
            &message_cache,
        )
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
    }

    Ok(())
}

/// Purge any expired entries in the message cache.
///
/// ### Arguments
///
/// - `cache` - the message cache
async fn purge_expired_cache_entries(cache: Arc<MessageCache>) {
    info!("purging any expired cache entries");
    cache.purge_expired().await;
}
