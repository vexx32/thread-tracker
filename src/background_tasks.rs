use std::{
    fmt::Display,
    time::Duration,
    future::Future,
    sync::Arc,
};

use chrono::Utc;
use serenity::{
    model::prelude::*,
    prelude::*,
    utils::Colour,
};
use tracing::{error, info};

use crate::{
    db::{self, Database},
    threads::{self, TrackedThread},
    muses,
    todos::{self, Todo},
    watchers::ThreadWatcher,
};

const HEARTBEAT_INTERVAL_SECONDS: u32 = 295;
const WATCHER_UPDATE_INTERVAL_SECONDS: u32 = 600;

/// Core task spawning function. Creates a set of periodically recurring tasks on their own threads.
pub(crate) async fn run_periodic_tasks(context: Arc<Context>, database: Arc<Database>) {
    spawn_task_loop(
        context.clone(),
        database.clone(),
        Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS.into()),
        |c, _| heartbeat(c));

    spawn_result_task_loop(
        context,
        database,
        Duration::from_secs(WATCHER_UPDATE_INTERVAL_SECONDS.into()),
        update_watchers
    );
}

/// Spawns a task which loops indefinitely, with a wait period between each iteration.
///
/// ### Arguments
///
/// - `context` - the bot's serenity context
/// - `database` - the bot's database connection
/// - `period` - the length of time between each task run
/// - `task` - the Future representing the task to run
fn spawn_task_loop<F, Ft>(context: Arc<Context>, database: Arc<Database>, period: Duration, mut task: F)
where
    F: FnMut(Arc<Context>, Arc<Database>) -> Ft + Send + 'static,
    Ft: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(period);

        loop {
            interval.tick().await;
            task(Arc::clone(&context), Arc::clone(&database)).await;
        }
    });
}

/// Spawns a task which loops indefinitely, with a wait period between each iteration. If the task
/// errors, ensures the error is logged.
///
/// ### Arguments
///
/// - `context` - the bot's serenity context
/// - `database` - the bot's database connection
/// - `period` - the length of time between each task run
/// - `task` - the Future representing the task to run
fn spawn_result_task_loop<F, T, U, Ft>(context: Arc<Context>, database: Arc<Database>, period: Duration, mut task: F)
where
    F: FnMut(Arc<Context>, Arc<Database>) -> Ft + Send + 'static,
    Ft: Future<Output = Result<T, U>> + Send + 'static,
    U: Display,
{
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(period);

        loop {
            interval.tick().await;

            if let Err(e) = task(Arc::clone(&context), Arc::clone(&database)).await {
                error!("Error running periodic task: {}", e)
            }
        }
    });
}

/// Performs a set_presence request to ensure the Activity is set correctly.
pub(crate) async fn heartbeat(ctx: Arc<Context>) {
    ctx.set_presence(Some(Activity::watching("over your threads (tt!help)")), OnlineStatus::Online).await;
    info!("[heartbeat] Keep-alive heartbeat set_presence request completed")
}

/// Updates all recorded watchers and edits their referenced messages with the new content.
pub(crate) async fn update_watchers(context: Arc<Context>, database: Arc<Database>) -> Result<(), anyhow::Error> {
    info!("[threadwatch] Updating watchers");
    let watchers: Vec<ThreadWatcher> = db::list_watchers(&database).await?
        .into_iter()
        .map(|w| w.into())
        .collect();

    for watcher in watchers {
        info!("[threadwatch] Updating watcher {:?}", watcher);
        let mut message = match context.http.get_message(watcher.channel_id.0, watcher.message_id.0).await {
            Ok(m) => m,
            Err(e) => {
                let channel_name = watcher.channel_id
                    .to_channel(&context.http).await
                    .map_or(None, |c| c.guild())
                    .map_or_else(|| "<unavailable channel>".to_owned(), |gc| gc.name);

                error!("[threadwatch] Could not find message {} in channel {}: {}. Removing watcher.", watcher.message_id, channel_name, e);
                db::remove_watcher(&database, watcher.id).await
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
        let threads_content = threads::get_formatted_list(threads, todos, muses, &watcher.user(), &context).await?;

        let edit_result = message.edit(
            &context.http,
            |msg| msg.embed(|embed|
                embed.colour(Colour::PURPLE)
                    .title("Watching threads")
                    .description(threads_content)
                    .footer(|footer| footer.text(format!("Last updated: {}", Utc::now())))
            )
        ).await;
        if let Err(e) = edit_result {
            // If we return here, an error updating one watcher message would prevent the rest from being updated.
            // Simply log these instead.
            error!("Could not edit message: {}", e);
        }
    }

    Ok(())
}
