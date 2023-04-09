use sqlx::{FromRow, PgPool};

#[derive(FromRow)]
pub(crate) struct TrackedThread {
    pub id: i32,
    pub channel_id: i64,
    pub guild_id: i64,
    pub category: Option<String>,
}

pub(crate) async fn add(
    pool: &PgPool,
    guild_id: i64,
    channel_id: i64,
    user_id: i64,
    category: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let existing_thread = get(pool, guild_id, user_id, channel_id).await?;

    if existing_thread.is_some() {
        return Ok(false);
    }

    sqlx::query("INSERT INTO threads (channel_id, user_id, guild_id, category) VALUES ($1, $2, $3, $4)")
        .bind(channel_id)
        .bind(user_id)
        .bind(guild_id)
        .bind(category)
        .execute(pool)
        .await?;

    Ok(true)
}

pub(crate) async fn update_category(
    pool: &PgPool,
    guild_id: i64,
    channel_id: i64,
    user_id: i64,
    category: Option<&str>
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("UPDATE threads SET category = $1 WHERE guild_id = $2 AND channel_id = $3 AND user_id = $4")
        .bind(category)
        .bind(guild_id)
        .bind(channel_id)
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

pub(crate) async fn remove(
    pool: &PgPool,
    guild_id: i64,
    channel_id: i64,
    user_id: i64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM threads WHERE channel_id = $1 AND user_id = $2 AND guild_id = $3")
        .bind(channel_id)
        .bind(user_id)
        .bind(guild_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

pub(crate) async fn remove_all(pool: &PgPool, guild_id: i64, user_id: i64, category: Option<&str>) -> Result<u64, sqlx::Error> {
    let query = match category {
        Some(c) => sqlx::query("DELETE FROM threads where user_id = $1 AND guild_id = $2 AND category = $3")
            .bind(user_id)
            .bind(guild_id)
            .bind(c),
        None => sqlx::query("DELETE FROM threads where user_id = $1 AND guild_id = $2")
            .bind(user_id)
            .bind(guild_id),
    };

    let result = query.execute(pool).await?;

    Ok(result.rows_affected())
}

pub(crate) async fn list(pool: &PgPool, guild_id: i64, user_id: i64, category: Option<&str>) -> Result<Vec<TrackedThread>, sqlx::Error> {
    let query = match category {
        Some(c) => sqlx::query_as("SELECT channel_id, category, guild_id, id FROM threads WHERE user_id = $1 AND guild_id = $2 AND category = $3 ORDER BY id")
            .bind(user_id)
            .bind(guild_id)
            .bind(c),
        None => sqlx::query_as("SELECT channel_id, category, guild_id, id FROM threads WHERE user_id = $1 AND guild_id = $2 ORDER BY id")
            .bind(user_id)
            .bind(guild_id),
    };

    let threads: Vec<TrackedThread> = query.fetch_all(pool).await?;

    Ok(threads)
}

pub(crate) async fn get(pool: &PgPool, guild_id: i64, user_id: i64, channel_id: i64) -> Result<Option<TrackedThread>, sqlx::Error> {
    let mut thread: Vec<TrackedThread> =
        sqlx::query_as("SELECT channel_id, category, guild_id, id FROM threads WHERE user_id = $1 AND channel_id = $2 AND guild_id = $3 ORDER BY id")
            .bind(user_id)
            .bind(channel_id)
            .bind(guild_id)
            .fetch_all(pool)
            .await?;

    Ok(thread.pop())
}
