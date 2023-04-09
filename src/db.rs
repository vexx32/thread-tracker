use sqlx::{FromRow, PgPool};

#[derive(FromRow)]
pub(crate) struct TrackedThread {
    pub id: i32,
    pub channel_id: i64,
    pub guild_id: i64,
    pub category: Option<String>,
}

#[derive(FromRow)]
pub(crate) struct ThreadWatcher {
    pub id: i32,
    pub user_id: i64,
    pub message_id: i64,
    pub channel_id: i64,
    pub guild_id: i64,
    pub categories: Option<String>,
}

pub(crate) async fn list_watchers(pool: &PgPool) -> Result<Vec<ThreadWatcher>, sqlx::Error> {
    sqlx::query_as("SELECT id, user_id, message_id, channel_id, guild_id, categories FROM watchers")
        .fetch_all(pool)
        .await
}

pub(crate) async fn add_watcher(
    pool: &PgPool,
    user_id: u64,
    message_id: u64,
    channel_id: u64,
    guild_id: u64,
    categories: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("INSERT INTO watchers (user_id, message_id, channel_id, guild_id, categories) VALUES ($1, $2, $3, $4, $5)")
        .bind(user_id as i64)
        .bind(message_id as i64)
        .bind(channel_id as i64)
        .bind(guild_id as i64)
        .bind(categories)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

pub(crate) async fn remove_watcher(
    pool: &PgPool,
    guild_id: u64,
    channel_id: u64,
    message_id: u64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM watchers WHERE guild_id = $1 AND channel_id = $2 AND message_id = $3")
        .bind(guild_id as i64)
        .bind(channel_id as i64)
        .bind(message_id as i64)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

pub(crate) async fn add_thread(
    pool: &PgPool,
    guild_id: u64,
    channel_id: u64,
    user_id: u64,
    category: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let existing_thread = get_thread(pool, guild_id, user_id, channel_id).await?;

    if existing_thread.is_some() {
        return Ok(false);
    }

    sqlx::query("INSERT INTO threads (channel_id, user_id, guild_id, category) VALUES ($1, $2, $3, $4)")
        .bind(channel_id as i64)
        .bind(user_id as i64)
        .bind(guild_id as i64)
        .bind(category)
        .execute(pool)
        .await?;

    Ok(true)
}

pub(crate) async fn update_thread_category(
    pool: &PgPool,
    guild_id: u64,
    channel_id: u64,
    user_id: u64,
    category: Option<&str>
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("UPDATE threads SET category = $1 WHERE guild_id = $2 AND channel_id = $3 AND user_id = $4")
        .bind(category)
        .bind(guild_id as i64)
        .bind(channel_id as i64)
        .bind(user_id as i64)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

pub(crate) async fn remove_thread(
    pool: &PgPool,
    guild_id: u64,
    channel_id: u64,
    user_id: u64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM threads WHERE channel_id = $1 AND user_id = $2 AND guild_id = $3")
        .bind(channel_id as i64)
        .bind(user_id as i64)
        .bind(guild_id as i64)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

pub(crate) async fn remove_all_threads(pool: &PgPool, guild_id: u64, user_id: u64, category: Option<&str>) -> Result<u64, sqlx::Error> {
    let query = match category {
        Some(c) => sqlx::query("DELETE FROM threads where user_id = $1 AND guild_id = $2 AND category = $3")
            .bind(user_id as i64)
            .bind(guild_id as i64)
            .bind(c),
        None => sqlx::query("DELETE FROM threads where user_id = $1 AND guild_id = $2")
            .bind(user_id as i64)
            .bind(guild_id as i64),
    };

    let result = query.execute(pool).await?;

    Ok(result.rows_affected())
}

pub(crate) async fn list_threads(pool: &PgPool, guild_id: u64, user_id: u64, category: Option<&str>) -> Result<Vec<TrackedThread>, sqlx::Error> {
    let query = match category {
        Some(c) => sqlx::query_as("SELECT channel_id, category, guild_id, id FROM threads WHERE user_id = $1 AND guild_id = $2 AND category = $3 ORDER BY id")
            .bind(user_id as i64)
            .bind(guild_id as i64)
            .bind(c),
        None => sqlx::query_as("SELECT channel_id, category, guild_id, id FROM threads WHERE user_id = $1 AND guild_id = $2 ORDER BY id")
            .bind(user_id as i64)
            .bind(guild_id as i64),
    };

    let threads: Vec<TrackedThread> = query.fetch_all(pool).await?;

    Ok(threads)
}

pub(crate) async fn get_thread(pool: &PgPool, guild_id: u64, user_id: u64, channel_id: u64) -> Result<Option<TrackedThread>, sqlx::Error> {
    let mut thread: Vec<TrackedThread> =
        sqlx::query_as("SELECT channel_id, category, guild_id, id FROM threads WHERE user_id = $1 AND channel_id = $2 AND guild_id = $3 ORDER BY id")
            .bind(user_id as i64)
            .bind(channel_id as i64)
            .bind(guild_id as i64)
            .fetch_all(pool)
            .await?;

    Ok(thread.pop())
}
