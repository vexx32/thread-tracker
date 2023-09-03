use std::{
    fmt::Display,
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

use serenity::{model::prelude::*, prelude::*};
use tokio::task::JoinSet;
use tracing::{error, info};

use crate::{
    cache::MessageCache,
    commands::watchers::{self, ThreadWatcher},
    consts::*,
    db::{self, Database},
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
) -> anyhow::Result<()> {
    let task_start = Instant::now();
    let watchers: Vec<Vec<ThreadWatcher>> = {
        let mut vec = Vec::new();
        let list: Vec<ThreadWatcher> =
            db::list_watchers(&database).await?.into_iter().map(|w| w.into()).collect();
        let batch_size = list.len() / MAX_WATCHER_UPDATE_TASKS;

        let mut list = list.into_iter().peekable();
        while list.peek().is_some() {
            vec.push(list.by_ref().take(batch_size).collect());
        }

        vec
    };

    let mut stagger_interval = tokio::time::interval(Duration::from_millis(100));

    let mut tasks = JoinSet::new();
    for watcher_batch in watchers {
        stagger_interval.tick().await;
        let context = Arc::clone(&context);
        let database = Arc::clone(&database);
        let message_cache = Arc::clone(&message_cache);
        tasks.spawn(async move {
            for watcher in watcher_batch {
                let id = watcher.id;
                let result =
                    watchers::update_watched_message(watcher, &context, &database, &message_cache)
                        .await;
                if let Err(e) = result {
                    error!("error updating watcher {}: {}", id, e);
                }
            }
        });
    }

    while let Some(res) = tasks.join_next().await {
        if let Err(e) = res {
            error!("{}", e);
        }
    }

    let task_duration = Instant::now() - task_start;
    info!(
        "updating all watchers completed in {:.2} s ({} ms)",
        task_duration.as_secs_f32(),
        task_duration.as_millis()
    );

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
