mod models;

pub(crate) use sqlx::PgPool as Database;
pub(crate) use models::*;

pub(crate) async fn list_watchers(database: &Database) -> Result<Vec<ThreadWatcherRow>, sqlx::Error> {
    sqlx::query_as("SELECT id, user_id, message_id, channel_id, guild_id, categories FROM watchers")
        .fetch_all(database).await
}

pub(crate) async fn get_watcher(database: &Database, channel_id: u64, message_id: u64) -> Result<Option<ThreadWatcherRow>, sqlx::Error> {
    sqlx::query_as("SELECT id, user_id, message_id, channel_id, guild_id, categories FROM watchers WHERE channel_id = $1 AND message_id = $2")
        .bind(channel_id as i64)
        .bind(message_id as i64)
        .fetch_optional(database).await
}

pub(crate) async fn add_watcher(
    database: &Database,
    user_id: u64,
    message_id: u64,
    channel_id: u64,
    guild_id: u64,
    categories: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("INSERT INTO watchers (user_id, message_id, channel_id, guild_id, categories) VALUES ($1, $2, $3, $4, $5)")
        .bind(user_id as i64)
        .bind(message_id as i64)
        .bind(channel_id as i64)
        .bind(guild_id as i64)
        .bind(categories)
        .execute(database).await?;

    Ok(result.rows_affected() > 0)
}

pub(crate) async fn remove_watcher(
    database: &Database,
    watcher_id: i32,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM watchers WHERE id = $1")
        .bind(watcher_id)
        .execute(database).await?;

    Ok(result.rows_affected())
}

pub(crate) async fn add_thread(
    database: &Database,
    guild_id: u64,
    channel_id: u64,
    user_id: u64,
    category: Option<&str>,
) -> Result<bool, sqlx::Error> {
    match get_thread(database, guild_id, user_id, channel_id).await? {
        Some(_) => Ok(false),
        None => {
            sqlx::query("INSERT INTO threads (channel_id, user_id, guild_id, category) VALUES ($1, $2, $3, $4)")
                .bind(channel_id as i64)
                .bind(user_id as i64)
                .bind(guild_id as i64)
                .bind(category)
                .execute(database).await?;

            Ok(true)
        },
    }
}

pub(crate) async fn update_thread_category(
    database: &Database,
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
        .execute(database).await?;

    Ok(result.rows_affected() > 0)
}

pub(crate) async fn remove_thread(
    database: &Database,
    guild_id: u64,
    channel_id: u64,
    user_id: u64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM threads WHERE channel_id = $1 AND user_id = $2 AND guild_id = $3")
        .bind(channel_id as i64)
        .bind(user_id as i64)
        .bind(guild_id as i64)
        .execute(database).await?;

    Ok(result.rows_affected())
}

pub(crate) async fn remove_all_threads(database: &Database, guild_id: u64, user_id: u64, category: Option<&str>) -> Result<u64, sqlx::Error> {
    let query = match category {
        Some(c) => sqlx::query("DELETE FROM threads where user_id = $1 AND guild_id = $2 AND category = $3")
            .bind(user_id as i64)
            .bind(guild_id as i64)
            .bind(c),
        None => sqlx::query("DELETE FROM threads where user_id = $1 AND guild_id = $2")
            .bind(user_id as i64)
            .bind(guild_id as i64),
    };

    let result = query.execute(database).await?;

    Ok(result.rows_affected())
}

pub(crate) async fn list_threads(database: &Database, guild_id: u64, user_id: u64, category: Option<&str>) -> Result<Vec<TrackedThreadRow>, sqlx::Error> {
    let query = match category {
        Some(c) => sqlx::query_as("SELECT channel_id, category, guild_id, id FROM threads WHERE user_id = $1 AND guild_id = $2 AND category = $3 ORDER BY id")
            .bind(user_id as i64)
            .bind(guild_id as i64)
            .bind(c),
        None => sqlx::query_as("SELECT channel_id, category, guild_id, id FROM threads WHERE user_id = $1 AND guild_id = $2 ORDER BY id")
            .bind(user_id as i64)
            .bind(guild_id as i64),
    };

    Ok(query.fetch_all(database).await?)
}

pub(crate) async fn get_thread(database: &Database, guild_id: u64, user_id: u64, channel_id: u64) -> Result<Option<TrackedThreadRow>, sqlx::Error> {
    Ok(sqlx::query_as("SELECT channel_id, category, guild_id, id FROM threads WHERE user_id = $1 AND channel_id = $2 AND guild_id = $3 ORDER BY id")
        .bind(user_id as i64)
        .bind(channel_id as i64)
        .bind(guild_id as i64)
        .fetch_optional(database).await?)
}

pub(crate) async fn add_muse(database: &Database, guild_id: u64, user_id: u64, muse: &str) -> Result<bool, sqlx::Error> {
    match get_muse(database, guild_id, user_id, muse).await? {
        Some(_) => Ok(false),
        None => {
            sqlx::query("INSERT INTO muses (muse_name, user_id, guild_id) VALUES ($1, $2, $3)")
                .bind(muse)
                .bind(user_id as i64)
                .bind(guild_id as i64)
                .execute(database).await?;

            Ok(true)
        }
    }
}

pub(crate) async fn get_muse(database: &Database, guild_id: u64, user_id: u64, muse: &str) -> Result<Option<MuseRow>, sqlx::Error> {
    Ok(
        sqlx::query_as("SELECT id, muse_name FROM muses WHERE user_id = $1 AND guild_id = $2 AND muse_name = $3")
            .bind(user_id as i64)
            .bind(guild_id as i64)
            .bind(muse)
            .fetch_optional(database).await?
    )
}

pub(crate) async fn list_muses(database: &Database, guild_id: u64, user_id: u64) -> Result<Vec<MuseRow>, sqlx::Error> {
    Ok(
        sqlx::query_as("SELECT id, muse_name FROM muses WHERE user_id = $1 AND guild_id = $2")
            .bind(user_id as i64)
            .bind(guild_id as i64)
            .fetch_all(database).await?
    )
}

pub(crate) async fn remove_muse(database: &Database, guild_id: u64, user_id: u64, muse: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM muses WHERE muse_name = $1 AND user_id = $2 AND guild_id = $3")
        .bind(muse)
        .bind(user_id as i64)
        .bind(guild_id as i64)
        .execute(database).await?;

    Ok(result.rows_affected())
}
