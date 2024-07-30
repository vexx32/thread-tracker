use std::{
    cmp,
    sync::Arc,
    time::{Duration, Instant},
};

use serenity::{model::prelude::*, prelude::*, gateway::ActivityData};
use tokio::{task::JoinSet, sync::mpsc::{Receiver, Sender}};
use tracing::{error, info};

use crate::{
    cache::MessageCache,
    commands::{threads::send_reply_notification, watchers},
    consts::*,
    db::{self, Database, ThreadWatcher},
    Data,
};

/// Task dispatch type, carrying messages and any data required for them to complete the associated task.
#[derive(Clone)]
pub(crate) enum Task {
    /// Handle notifications for new thread replies, if any are needed.
    Notify(Message),
    /// Update discord status and ensure it is set to online for the given shard context.
    Heartbeat(Arc<Context>),
    /// Kick off a watcher update thread.
    UpdateWatchers,
    /// Purge expired cache entries.
    PurgeCache,
}

/// Start a new thread which listens for `Task` messages and running the appropriate actions for each task.
pub(crate) fn listen_for_background_tasks(mut receiver: Receiver<Task>, data: Arc<RwLock<Data>>, context: Arc<impl CacheHttp + 'static>) {
    use Task::*;

    info!("Starting background task listening thread");

    tokio::spawn(async move {
        let data = data.read().await;
        let database = &data.database;
        let cache = &data.message_cache;

        while let Some(task) = receiver.recv().await {
            match task {
                Notify(message) => send_reply_notification(message, database.clone(), context.clone()).await,
                Heartbeat(context) => heartbeat(&context).await,
                UpdateWatchers => start_watcher_update_thread(context.clone(), database.clone(), cache.clone()),
                PurgeCache => purge_expired_cache_entries(Arc::new(cache.clone())).await,
            };
        }
    });
}

/// Core task spawning function for per-shard tasks.
pub(crate) fn run_periodic_shard_tasks(context: &Context, sender: &Sender<Task>) {
    info!("Starting periodic per-shard tasks");
    let c = Arc::new(context.clone());
    spawn_task_loop(sender.clone(), HEARTBEAT_INTERVAL, false, move || Task::Heartbeat(c.clone()));
}

/// Core task spawning function. Creates a set of periodically recurring tasks on their own threads.
pub(crate) fn start_periodic_tasks(sender: &Sender<Task>) {
    info!("Starting periodic global tasks");
    spawn_task_loop(sender.clone(), CACHE_TRIM_INTERVAL, true, || Task::PurgeCache);
    spawn_task_loop(sender.clone(), WATCHER_UPDATE_INTERVAL, true, || Task::UpdateWatchers);
}

/// Spawns a task which loops indefinitely, with a wait period between each iteration.
fn spawn_task_loop<F>(sender: Sender<Task>, period: Duration, delay: bool, mut task: F)
where
    F: FnMut() -> Task + Send + 'static,
{
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(period);

        if delay {
            // Skip immediate first tick
            interval.tick().await;
        }

        loop {
            interval.tick().await;
            if let Err(e) = sender.send(task()).await {
                error!("Error creating background task: {}", e);
            }
        }
    });
}

/// Performs a set_presence request to ensure the Activity is set correctly.
pub(crate) async fn heartbeat(ctx: &Context) {
    ctx.set_presence(
        Some(ActivityData::watching("over your threads (/tt_help)")),
        OnlineStatus::Online,
    );
    info!("heartbeat set_presence request completed for shard ID {}", ctx.shard_id);
}

fn start_watcher_update_thread(context: Arc<impl CacheHttp + 'static>, database: Database, cache: MessageCache) {
    tokio::spawn(async move {
        if let Err(e) = update_watchers(context, database, cache).await {
            error!("Error updating watchers: {}", e);
        }
    });
}

/// Updates all recorded watchers and edits their referenced messages with the new content.
pub(crate) async fn update_watchers(
    cache_http: Arc<impl CacheHttp + 'static>,
    database: Database,
    message_cache: MessageCache,
) -> anyhow::Result<()> {
    let task_start = Instant::now();
    info!("Watcher update loop started");

    let mut stagger_interval = tokio::time::interval(Duration::from_millis(100));
    let batches = get_watcher_batches(&database).await?;
    let context = Arc::clone(&cache_http);

    let mut tasks = JoinSet::new();
    for watcher_batch in batches {
        stagger_interval.tick().await;
        let database = database.clone();
        let ctx = Arc::clone(&context);
        let message_cache = message_cache.clone();
        tasks.spawn(async move {
            for watcher in watcher_batch {
                let id = watcher.id;
                let result =
                    watchers::update_watched_message(watcher, &ctx, &database, &message_cache)
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

/// Retrieves the list of watchers in the database, subdivided into batches of at least 10.
async fn get_watcher_batches(database: &Database) -> sqlx::Result<Vec<Vec<ThreadWatcher>>> {
    let list: Vec<ThreadWatcher> = db::list_watchers(database).await?;
    let batch_size = cmp::min(MIN_WATCHER_BATCH_SIZE, list.len() / MAX_WATCHER_UPDATE_TASKS);

    let mut result = Vec::new();
    let mut chunk = Vec::new();
    for watcher in list {
        if chunk.len() >= batch_size {
            result.push(chunk);
            chunk = Vec::new();
        }

        chunk.push(watcher);
    }

    result.push(chunk);

    Ok(result)
}

/// Purge any expired entries in the message cache.
async fn purge_expired_cache_entries(cache: Arc<MessageCache>) {
    info!("purging any expired cache entries");
    cache.purge_expired().await;
}
