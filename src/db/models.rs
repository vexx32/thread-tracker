#![allow(dead_code)]

use poise::serenity_prelude::{ChannelId, GuildId, MessageId, UserId};
use sqlx::FromRow;

use crate::utils::{ChannelMessage, GuildUser};

#[derive(FromRow)]
pub(crate) struct TrackedThread {
    #[allow(dead_code)]
    pub id: i32,
    #[sqlx(try_from = "i64")]
    pub channel_id: u64,
    #[sqlx(try_from = "i64")]
    pub guild_id: u64,
    pub category: Option<String>,
}

impl TrackedThread {
    /// Get the ChannelId for this thread.
    pub fn channel_id(&self) -> ChannelId {
        self.channel_id.into()
    }

    /// Get the GuildId for this thread.
    pub fn guild_id(&self) -> GuildId {
        self.guild_id.into()
    }
}

#[derive(FromRow)]
#[repr(transparent)]
pub(crate) struct TrackedThreadId {
    #[sqlx(try_from = "i64")]
    pub channel_id: u64,
}

#[derive(FromRow)]
#[repr(transparent)]
pub(crate) struct TrackedThreadUser {
    #[sqlx(try_from = "i64")]
    pub user_id: u64,
}

impl From<TrackedThreadUser> for UserId {
    fn from(value: TrackedThreadUser) -> Self {
        value.user_id.into()
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct ThreadWatcher {
    pub id: i32,
    #[sqlx(try_from = "i64")]
    pub user_id: u64,
    #[sqlx(try_from = "i64")]
    pub message_id: u64,
    #[sqlx(try_from = "i64")]
    pub channel_id: u64,
    #[sqlx(try_from = "i64")]
    pub guild_id: u64,
    pub categories: Option<String>,
}

impl ThreadWatcher {
    /// Get the guild and user for this thread watcher.
    pub fn user(&self) -> GuildUser {
        self.into()
    }

    /// Get the UserId for this thread watcher.
    pub fn user_id(&self) -> UserId {
        self.user_id.into()
    }

    /// Get the MessageId for this thread watcher.
    pub fn message_id(&self) -> MessageId {
        self.message_id.into()
    }

    /// Get the ChannelId for this thread watcher.
    pub fn channel_id(&self) -> ChannelId {
        self.channel_id.into()
    }

    /// Get the GuildId for this thread watcher.
    pub fn guild_id(&self) -> GuildId {
        self.guild_id.into()
    }

    /// Get the channel and message for this thread watcher.
    pub fn message(&self) -> ChannelMessage {
        (self.channel_id(), self.message_id()).into()
    }
}

#[derive(FromRow)]
pub(crate) struct Muse {
    #[allow(dead_code)]
    pub id: i32,
    pub muse_name: String,
}

#[derive(FromRow)]
pub(crate) struct Todo {
    #[allow(dead_code)]
    pub id: i32,
    pub content: String,
    pub category: Option<String>,
}

#[derive(FromRow)]
pub(crate) struct Subscription {
    pub id: i32,
    #[sqlx(try_from = "i64")]
    pub user_id: u64,
}

impl Subscription {
    pub(crate) fn user_id(&self) -> UserId {
        self.user_id.into()
    }
}

#[derive(FromRow)]
pub(crate) struct Statistics {
    pub users: i64,
    pub servers: i64,
    pub threads_distinct: i64,
    pub threads_total: i64,
    pub muses: i64,
    pub todos: i64,
    pub watchers: i64,
}

#[derive(FromRow)]
pub(crate) struct UserSetting {
    #[sqlx(try_from = "i64")]
    pub user_id: u64,
    pub name: String,
    pub value: String,
}

impl UserSetting {
    pub(crate) fn user_id(&self) -> UserId {
        self.user_id.into()
    }
}

#[derive(FromRow)]
pub(crate) struct ScheduledMessage {
    pub id: i32,
    #[sqlx(try_from = "i64")]
    pub user_id: u64,
    #[sqlx(try_from = "i64")]
    pub channel_id: u64,
    pub datetime: String,
    pub repeat: String,
    pub title: String,
    pub message: String,
    pub archived: bool,
}

impl ScheduledMessage {
    pub(crate) fn user_id(&self) -> UserId {
        self.user_id.into()
    }

    pub(crate) fn channel_id(&self) -> ChannelId {
        self.channel_id.into()
    }
}

#[derive(FromRow)]
pub(crate) struct ScheduledMessageSummary {
    pub id: i32,
    #[sqlx(try_from = "i64")]
    pub channel_id: u64,
    pub datetime: String,
    pub repeat: String,
    pub title: String,
}

impl ScheduledMessageSummary {
    pub(crate) fn channel_id(&self) -> ChannelId {
        self.channel_id.into()
    }
}
