use serenity::{
    model::prelude::*,
    prelude::*,
};
use sqlx::PgPool;
use thiserror::Error;
use tracing::{error, info};

use crate::{
    db,
    threads::*,
    messaging::*,

    CommandError::*,
};

use WatcherError::*;

type Result<T> = std::result::Result<T, WatcherError>;

#[derive(Debug)]
pub(crate) struct ThreadWatcher {
    pub message_id: MessageId,
    pub channel_id: ChannelId,
    pub guild_id: GuildId,
    pub user_id: UserId,
    pub id: i32,
    pub categories: Option<String>,
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

#[derive(Error, Debug)]
enum WatcherError {
    #[error("Error fetching watcher: {0}")]
    NotFound(String),
    #[error("Not allowed: {0}")]
    NotAllowed(String),
}

pub(crate) async fn add(
    args: Vec<&str>,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &PgPool,
) -> anyhow::Result<()> {
    info!("[threadwatch] Adding watcher for user {}, categories {:?}", user_id, args);
    let arguments = if args.len() > 0 {
        Some(args.join(" "))
    }
    else {
        None
    };

    let message = send_list_with_title(args, guild_id, user_id, channel_id, "Watching threads", ctx, database).await?;
    db::add_watcher(database, user_id.0, message.id.0, channel_id.0, guild_id.0, arguments.as_deref()).await?;

    Ok(())
}

pub(crate) async fn remove(
    args: Vec<&str>,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &PgPool,
) -> anyhow::Result<()> {
    let mut args = args.into_iter().peekable();
    if args.peek().is_none() {
        return Err(MissingArguments(String::from("Please provide a message URL to a watcher message, such as: `tt!unwatch <message url>`.")).into());
    }

    info!("Removing watcher for user {}, categories {:?}", user_id, args);

    let message_url = args.next().unwrap();
    let (watcher_message_id, watcher_channel_id) = parse_message_link(message_url)?;

    let watcher: ThreadWatcher = match db::get_watcher(database, watcher_channel_id, watcher_message_id).await? {
        Some(w) => w.into(),
        None => return Err(NotFound(format!("Could not find a watcher for the target message: `{}`", message_url)).into()),
    };

    if watcher.user_id != user_id {
        return Err(NotAllowed(format!("User {} does not own the watcher.", user_id)).into());
    }

    match db::remove_watcher(database, watcher.id).await? {
        0 => error!("Watcher should have been present in the database, but was missing when removal was attempted: {:?}", watcher),
        _ => {
            send_success_embed(&ctx.http, channel_id, "Watcher removed", "Watcher successfully removed.").await;
            ctx.http.get_message(watcher.channel_id.0, watcher.message_id.0).await?.delete(&ctx.http).await?;
        }
    }

    Ok(())
}

fn parse_message_link(link: &str) -> Result<(u64, u64)> {
    let mut message_url_fragments = link.split('/').rev();
    let mut result: [u64; 2] = [0, 0];

    for index in 0..2 {
        match message_url_fragments.next().and_then(|s| s.parse().ok()) {
            Some(n) => result[index] = n,
            None => return Err(NotFound(format!("Could not parse message ID from `{}`", link))),
        }
    }

    return Ok((result[0], result[1]));
}
