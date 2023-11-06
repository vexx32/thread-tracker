use std::{
    fmt::Display,
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

use serenity::{model::prelude::*, prelude::*, CacheAndHttp};
use tokio::task::JoinSet;
use tracing::{error, info};

use crate::{
    cache::MessageCache,
    commands::watchers,
    consts::*,
    db::{self, Database, ThreadWatcher},
    Data,
};

pub(crate) fn run_periodic_shard_tasks(context: Context) {
    let ctx = Arc::new(context);
    spawn_task_loop(HEARTBEAT_INTERVAL, move || heartbeat(Arc::clone(&ctx)));
}

/// Core task spawning function. Creates a set of periodically recurring tasks on their own threads.
///
/// ### Arguments
///
/// - `context` - the Serenity context to delegate to tasks
/// - `bot` - the bot instance to delegate to tasks
pub(crate) fn run_periodic_tasks(cache_http: Arc<CacheAndHttp>, data: &Data) {
    let c = Arc::new(data.message_cache.clone());
    spawn_task_loop(CACHE_TRIM_INTERVAL, move || purge_expired_cache_entries(Arc::clone(&c)));

    let database = data.database.clone();
    let cache = data.message_cache.clone();

    spawn_result_task_loop(WATCHER_UPDATE_INTERVAL, move || {
        update_watchers(cache_http.clone(), database.clone(), cache.clone())
    });
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
    ctx.set_presence(
        Some(Activity::watching("over your threads (/tt_help)")),
        OnlineStatus::Online,
    )
    .await;
    info!("heartbeat set_presence request completed for shard ID {}", ctx.shard_id);
}

async fn get_watcher_batches(database: &Database) -> sqlx::Result<Vec<Vec<ThreadWatcher>>> {
    let mut vec = Vec::new();
    let list: Vec<ThreadWatcher> = db::list_watchers(database).await?;
    let batch_size = list.len() / MAX_WATCHER_UPDATE_TASKS;

    let mut list = list.into_iter().peekable();
    while list.peek().is_some() {
        vec.push(list.by_ref().take(batch_size).collect());
    }

    Ok(vec)
}

/// Updates all recorded watchers and edits their referenced messages with the new content.
pub(crate) async fn update_watchers(
    cache_http: Arc<CacheAndHttp>,
    database: Database,
    message_cache: MessageCache,
) -> anyhow::Result<()> {
    let task_start = Instant::now();
    info!("Watcher update loop started");

    let mut stagger_interval = tokio::time::interval(Duration::from_millis(100));
    let batches = get_watcher_batches(&database).await?;

    let mut tasks = JoinSet::new();
    for watcher_batch in batches {
        stagger_interval.tick().await;
        let context = cache_http.clone();
        let database = database.clone();
        let message_cache = message_cache.clone();
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
