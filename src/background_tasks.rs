use std::time::Duration;

use chrono::Utc;
use serenity::{
    model::prelude::*,
    prelude::*,
    utils::Colour,
};
use sqlx::PgPool;
use tracing::{error, info};

use crate::{
    db,
    threads::*,
    watchers::*,
};

const HEARTBEAT_INTERVAL_SECONDS: u32 = 255;
const WATCHER_UPDATE_INTERVAL_SECONDS: u32 = 120;

pub(crate) async fn run_periodic_tasks(context: &Context, database: &PgPool) {
    let ctx = context.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS.into()));

        loop {
            interval.tick().await;
            heartbeat(&ctx).await;
        }
    });

    let ctx = context.clone();
    let db = database.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(WATCHER_UPDATE_INTERVAL_SECONDS.into()));

        loop {
            interval.tick().await;
            if let Err(e) = update_watchers(&ctx, &db).await {
                error!("Error updating watchers: {}", e);
            }
        }
    });
}

pub(crate) async fn heartbeat(ctx: &Context) {
    ctx.set_presence(Some(Activity::watching("over your threads (tt!help)")), OnlineStatus::Online).await;
    info!("[heartbeat] Keep-alive heartbeat set_presence request completed")
}

pub(crate) async fn update_watchers(ctx: &Context, database: &PgPool) -> Result<(), anyhow::Error> {
    info!("[threadwatch] Updating watchers");
    let watchers: Vec<ThreadWatcher> = db::list_watchers(database).await?
        .into_iter()
        .map(|w| w.into())
        .collect();

    for watcher in watchers {
        info!("[threadwatch] Updating watcher {:?}", watcher);
        let mut message = match ctx.http.get_message(watcher.channel_id.0, watcher.message_id.0).await {
            Ok(m) => m,
            Err(e) => {
                let channel_name = watcher.channel_id
                    .to_channel(&ctx.http).await
                    .map_or(None, |c| c.guild())
                    .map_or_else(|| "<unavailable channel>".to_owned(), |gc| gc.name);

                error!("[threadwatch] Could not find message {} in channel {}: {}. Removing watcher.", watcher.message_id, channel_name, e);
                db::remove_watcher(database, watcher.id).await
                    .map_err(|e| error!("Failed to remove watcher: {}", e))
                    .ok();
                continue;
            },
        };

        let mut threads: Vec<TrackedThread> = Vec::new();
        match watcher.categories.as_deref() {
            Some("") | None => threads.extend(
            db::list_threads(database, watcher.guild_id.0, watcher.user_id.0, None).await?
                .into_iter()
                .map(|t| t.into())
            ),
            Some(cats) => {
                for category in cats.split(" ") {
                    threads.extend(
                        db::list_threads(database, watcher.guild_id.0, watcher.user_id.0, Some(category)).await?
                            .into_iter()
                            .map(|t| t.into())
                    );
                }
            },
        }

        let threads_content = get_formatted_list(threads, ctx).await?;

        message.edit(&ctx, |msg| msg.embed(|embed|
                embed.colour(Colour::PURPLE)
                    .title("Watching threads")
                    .description(threads_content)
                    .footer(|footer| footer.text(format!("Last updated: {}", Utc::now())))))
            .await
            .err()
            .map(|e| error!("Could not edit message: {}", e));
    }

    Ok(())
}
