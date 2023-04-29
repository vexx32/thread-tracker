use sqlx::FromRow;

#[derive(FromRow)]
pub(crate) struct TrackedThreadRow {
    #[allow(dead_code)]
    pub id: i32,
    pub channel_id: i64,
    pub guild_id: i64,
    pub category: Option<String>,
}

#[derive(FromRow)]
pub(crate) struct TrackedThreadId {
    pub channel_id: i64,
}

#[derive(FromRow)]
pub(crate) struct ThreadWatcherRow {
    pub id: i32,
    pub user_id: i64,
    pub message_id: i64,
    pub channel_id: i64,
    pub guild_id: i64,
    pub categories: Option<String>,
}

#[derive(FromRow)]
pub(crate) struct MuseRow {
    #[allow(dead_code)]
    pub id: i32,
    pub muse_name: String,
}

#[derive(FromRow)]
pub(crate) struct TodoRow {
    #[allow(dead_code)]
    pub id: i32,
    pub content: String,
    pub category: Option<String>,
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
