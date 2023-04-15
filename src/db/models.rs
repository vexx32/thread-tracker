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
