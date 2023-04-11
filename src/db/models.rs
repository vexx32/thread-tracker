use sqlx::FromRow;

#[derive(FromRow)]
pub(crate) struct TrackedThreadRow {
    pub id: i32,
    pub channel_id: i64,
    pub guild_id: i64,
    pub category: Option<String>,
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
