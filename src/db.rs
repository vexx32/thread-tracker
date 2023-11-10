mod models;

pub(crate) use models::*;
use poise::serenity_prelude::UserId;
pub(crate) use sqlx::PgPool as Database;

type Result<T> = std::result::Result<T, sqlx::Error>;

/// Store an entry in the Subscriptions table
pub(crate) async fn add_subscriber<U>(database: &Database, user_id: U) -> Result<bool>
where
    U: Into<u64> + Copy,
{
    match get_subscriber(database, user_id).await? {
        Some(_) => Ok(false),
        None => {
            let result = sqlx::query("INSERT INTO subscriptions (user_id) VALUES ($1)")
                .bind(user_id.into() as i64)
                .execute(database)
                .await?;

            Ok(result.rows_affected() > 0)
        },
    }
}

pub(crate) async fn get_subscriber<U>(
    database: &Database,
    user_id: U,
) -> Result<Option<Subscription>>
where
    U: Into<u64> + Copy,
{
    sqlx::query_as("SELECT id, user_id FROM subscriptions WHERE user_id = $1")
        .bind(user_id.into() as i64)
        .fetch_optional(database)
        .await
}

pub(crate) async fn list_subscribers(database: &Database) -> Result<Vec<Subscription>> {
    sqlx::query_as("SELECT id, user_id FROM subscriptions ORDER BY id")
        .fetch_all(database)
        .await
}

pub(crate) async fn remove_subscriber(
    database: &Database,
    user_id: impl Into<u64>,
) -> Result<bool> {
    let result = sqlx::query("DELETE FROM subscriptions WHERE user_id = $1")
        .bind(user_id.into() as i64)
        .execute(database)
        .await?;

    Ok(result.rows_affected() > 0)
}

/// Get all entries from the watchers table
pub(crate) async fn list_watchers(database: &Database) -> Result<Vec<ThreadWatcher>> {
    sqlx::query_as("SELECT id, user_id, message_id, channel_id, guild_id, categories FROM watchers")
        .fetch_all(database)
        .await
}

pub(crate) async fn list_current_watchers(
    database: &Database,
    user_id: u64,
    guild_id: u64,
) -> Result<Vec<ThreadWatcher>> {
    sqlx::query_as("SELECT id, user_id, message_id, channel_id, guild_id, categories FROM watchers WHERE user_id = $1 AND guild_id = $2")
        .bind(user_id as i64)
        .bind(guild_id as i64)
        .fetch_all(database)
        .await
}

/// Get an entry from the watchers table by channel and message ID
pub(crate) async fn get_watcher(
    database: &Database,
    channel_id: u64,
    message_id: u64,
) -> Result<Option<ThreadWatcher>> {
    sqlx::query_as("SELECT id, user_id, message_id, channel_id, guild_id, categories FROM watchers WHERE channel_id = $1 AND message_id = $2")
        .bind(channel_id as i64)
        .bind(message_id as i64)
        .fetch_optional(database).await
}

/// Add a new entry to the watchers table
pub(crate) async fn add_watcher(
    database: &Database,
    user_id: u64,
    message_id: u64,
    channel_id: u64,
    guild_id: u64,
    categories: Option<&str>,
) -> Result<bool> {
    let result = sqlx::query("INSERT INTO watchers (user_id, message_id, channel_id, guild_id, categories) VALUES ($1, $2, $3, $4, $5)")
        .bind(user_id as i64)
        .bind(message_id as i64)
        .bind(channel_id as i64)
        .bind(guild_id as i64)
        .bind(categories)
        .execute(database).await?;

    Ok(result.rows_affected() > 0)
}

/// Remove an entry from the watchers table
pub(crate) async fn remove_watcher(database: &Database, watcher_id: i32) -> Result<u64> {
    let result = sqlx::query("DELETE FROM watchers WHERE id = $1")
        .bind(watcher_id)
        .execute(database)
        .await?;

    Ok(result.rows_affected())
}

/// Add a new entry to the threads table
pub(crate) async fn add_thread(
    database: &Database,
    guild_id: u64,
    channel_id: u64,
    user_id: u64,
    category: Option<&str>,
) -> Result<bool> {
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

/// Update the category of an entry in the threads table
pub(crate) async fn update_thread_category(
    database: &Database,
    guild_id: u64,
    channel_id: u64,
    user_id: u64,
    category: Option<&str>,
) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE threads SET category = $1 WHERE guild_id = $2 AND channel_id = $3 AND user_id = $4",
    )
    .bind(category)
    .bind(guild_id as i64)
    .bind(channel_id as i64)
    .bind(user_id as i64)
    .execute(database)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Remove an entry from the threads table
pub(crate) async fn remove_thread(
    database: &Database,
    guild_id: u64,
    channel_id: u64,
    user_id: u64,
) -> Result<u64> {
    let result =
        sqlx::query("DELETE FROM threads WHERE channel_id = $1 AND user_id = $2 AND guild_id = $3")
            .bind(channel_id as i64)
            .bind(user_id as i64)
            .bind(guild_id as i64)
            .execute(database)
            .await?;

    Ok(result.rows_affected())
}

/// Remove all entries from the threads table for a given user and guild ID
pub(crate) async fn remove_all_threads(
    database: &Database,
    guild_id: u64,
    user_id: u64,
    category: Option<&str>,
) -> Result<u64> {
    let query = match category {
        Some(c) => sqlx::query(
            "DELETE FROM threads where user_id = $1 AND guild_id = $2 AND category = $3",
        )
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

/// Get all entries from the threads table
pub(crate) async fn list_threads(
    database: &Database,
    guild_id: u64,
    user_id: u64,
    category: Option<&str>,
) -> Result<Vec<TrackedThread>> {
    let query = match category {
        Some(c) => sqlx::query_as("SELECT channel_id, category, guild_id, id FROM threads WHERE user_id = $1 AND guild_id = $2 AND lower(category) = lower($3) ORDER BY id")
            .bind(user_id as i64)
            .bind(guild_id as i64)
            .bind(c),
        None => sqlx::query_as("SELECT channel_id, category, guild_id, id FROM threads WHERE user_id = $1 AND guild_id = $2 ORDER BY id")
            .bind(user_id as i64)
            .bind(guild_id as i64),
    };

    query.fetch_all(database).await
}

/// Get an entry from the threads table with a specific channel ID and user ID
pub(crate) async fn get_thread(
    database: &Database,
    guild_id: u64,
    user_id: u64,
    channel_id: u64,
) -> Result<Option<TrackedThread>> {
    sqlx::query_as("SELECT channel_id, category, guild_id, id FROM threads WHERE user_id = $1 AND channel_id = $2 AND guild_id = $3 ORDER BY id")
        .bind(user_id as i64)
        .bind(channel_id as i64)
        .bind(guild_id as i64)
        .fetch_optional(database)
        .await
}

/// Get all users tracking a specific thread
pub(crate) async fn get_users_tracking_thread(
    database: &Database,
    guild_id: impl Into<u64>,
    channel_id: impl Into<u64>,
) -> Result<Vec<UserId>> {
    let result: Vec<TrackedThreadUser> = sqlx::query_as("SELECT user_id FROM threads WHERE channel_id = $1 AND guild_id = $2 ORDER BY id")
        .bind(channel_id.into() as i64)
        .bind(guild_id.into() as i64)
        .fetch_all(database)
        .await?;

    Ok(result.into_iter().map(|user| user.into()).collect())
}

/// Get all unique channel_ids from tracked threads (globally)
pub(crate) async fn get_global_tracked_thread_ids(
    database: &Database,
) -> Result<Vec<TrackedThreadId>> {
    sqlx::query_as("SELECT DISTINCT channel_id FROM threads").fetch_all(database).await
}

/// Add an entry to the muses table
pub(crate) async fn add_muse(
    database: &Database,
    guild_id: u64,
    user_id: u64,
    muse: &str,
) -> Result<bool> {
    match get_muse(database, guild_id, user_id, muse).await? {
        Some(_) => Ok(false),
        None => {
            sqlx::query("INSERT INTO muses (muse_name, user_id, guild_id) VALUES ($1, $2, $3)")
                .bind(muse)
                .bind(user_id as i64)
                .bind(guild_id as i64)
                .execute(database)
                .await?;

            Ok(true)
        },
    }
}

/// Get an entry from the muses table by name
pub(crate) async fn get_muse(
    database: &Database,
    guild_id: u64,
    user_id: u64,
    muse: &str,
) -> Result<Option<Muse>> {
    sqlx::query_as(
        "SELECT id, muse_name FROM muses WHERE user_id = $1 AND guild_id = $2 AND lower(muse_name) = lower($3)",
    )
    .bind(user_id as i64)
    .bind(guild_id as i64)
    .bind(muse)
    .fetch_optional(database)
    .await
}

/// Get all entries from the muses table for a given user and guild ID
pub(crate) async fn list_muses(
    database: &Database,
    guild_id: u64,
    user_id: u64,
) -> Result<Vec<Muse>> {
    sqlx::query_as("SELECT id, muse_name FROM muses WHERE user_id = $1 AND guild_id = $2")
        .bind(user_id as i64)
        .bind(guild_id as i64)
        .fetch_all(database)
        .await
}

/// Remove an entry from the muses table
pub(crate) async fn remove_muse(
    database: &Database,
    guild_id: u64,
    user_id: u64,
    muse: &str,
) -> Result<u64> {
    let result = sqlx::query(
        "DELETE FROM muses WHERE lower(muse_name) = lower($1) AND user_id = $2 AND guild_id = $3",
    )
    .bind(muse)
    .bind(user_id as i64)
    .bind(guild_id as i64)
    .execute(database)
    .await?;

    Ok(result.rows_affected())
}

/// Add an entry to the todos table
pub(crate) async fn add_todo(
    database: &Database,
    guild_id: u64,
    user_id: u64,
    content: &str,
    category: Option<&str>,
) -> Result<bool> {
    match get_todo(database, guild_id, user_id, content).await? {
        Some(t) if t.category.as_deref() != category => {
            sqlx::query("UPDATE todos SET category = $1 WHERE user_id = $2 AND guild_id = $3 AND lower(content) = lower($4)")
                .bind(category)
                .bind(user_id as i64)
                .bind(guild_id as i64)
                .bind(content)
                .execute(database).await?;

            Ok(true)
        },
        Some(_) => Ok(false),
        None => {
            sqlx::query(
                "INSERT INTO todos (content, category, user_id, guild_id) VALUES ($1, $2, $3, $4)",
            )
            .bind(content)
            .bind(category)
            .bind(user_id as i64)
            .bind(guild_id as i64)
            .execute(database)
            .await?;

            Ok(true)
        },
    }
}

/// Get an entry from the todos table by its content
pub(crate) async fn get_todo(
    database: &Database,
    guild_id: u64,
    user_id: u64,
    content: &str,
) -> Result<Option<Todo>> {
    sqlx::query_as("SELECT id, content, category FROM todos WHERE user_id = $1 AND guild_id = $2 AND lower(content) = lower($3)")
        .bind(user_id as i64)
        .bind(guild_id as i64)
        .bind(content)
        .fetch_optional(database).await
}

/// Get all entries from the todos table for a given user and guild ID
pub(crate) async fn list_todos(
    database: &Database,
    guild_id: u64,
    user_id: u64,
    category: Option<&str>,
) -> Result<Vec<Todo>> {
    let query = match category {
        Some(cat) => sqlx::query_as("SELECT id, content, category FROM todos WHERE lower(category) = lower($1) AND user_id = $2 AND guild_id = $3")
            .bind(cat),
        None => sqlx::query_as("SELECT id, content, category FROM todos WHERE user_id = $1 AND guild_id = $2"),
    };

    query.bind(user_id as i64).bind(guild_id as i64).fetch_all(database).await
}

/// Remove an entry from the todos table
pub(crate) async fn remove_todo(
    database: &Database,
    guild_id: u64,
    user_id: u64,
    content: &str,
) -> Result<u64> {
    let result = sqlx::query(
        "DELETE FROM todos WHERE lower(content) = lower($1) AND user_id = $2 AND guild_id = $3",
    )
    .bind(content)
    .bind(user_id as i64)
    .bind(guild_id as i64)
    .execute(database)
    .await?;

    Ok(result.rows_affected())
}

/// Remove all entries from the todos table that match the given user and guild IDs
pub(crate) async fn remove_all_todos(
    database: &Database,
    guild_id: u64,
    user_id: u64,
    category: Option<&str>,
) -> Result<u64> {
    let query = match category {
        Some(cat) => {
            sqlx::query("DELETE FROM todos WHERE lower(category) = lower($1) AND user_id = $2 AND guild_id = $3")
                .bind(cat)
        },
        None => sqlx::query("DELETE FROM todos WHERE user_id = $1 AND guild_id = $2"),
    };

    let result = query.bind(user_id as i64).bind(guild_id as i64).execute(database).await?;

    Ok(result.rows_affected())
}

/// Query for overall statistics from the database
pub(crate) async fn statistics(database: &Database) -> Result<Statistics> {
    sqlx::query_as(include_str!("../sql/queries/stats.sql")).fetch_one(database).await
}
