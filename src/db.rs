mod models;

use chrono::{DateTime, Utc};
use serenity::all::{GuildId, UserId};

pub(crate) use models::*;

pub(crate) use sqlx::PgPool as Database;
pub(crate) type Result<T> = std::result::Result<T, sqlx::Error>;

pub(crate) async fn remove_server_nickname<A, B>(
    database: &Database,
    user_id: A,
    guild_id: B,
) -> Result<bool>
where
    A: Into<u64> + Copy,
    B: Into<u64> + Copy,
{
    let result = sqlx::query("DELETE FROM server_nicknames WHERE user_id = $1 AND guild_id = $2")
        .bind(user_id.into() as i64)
        .bind(guild_id.into() as i64)
        .execute(database)
        .await?;

    Ok(result.rows_affected() > 0)
}

/// Set the server nickname for a given user
pub(crate) async fn set_server_nickname<A, B>(
    database: &Database,
    user_id: A,
    guild_id: B,
    nickname: &str,
) -> Result<bool>
where
    A: Into<u64> + Copy,
    B: Into<u64> + Copy,
{
    let query_string = match get_server_nickname(database, user_id, guild_id).await? {
        Some(current_name) => {
            if current_name.nickname == nickname {
                return Ok(false);
            }

            "UPDATE server_nicknames SET nickname = $3 WHERE user_id = $1 AND guild_id = $2"
        },
        None => "INSERT INTO server_nicknames (user_id, guild_id, nickname) VALUES ($1, $2, $3)",
    };

    let result = sqlx::query(query_string)
        .bind(user_id.into() as i64)
        .bind(guild_id.into() as i64)
        .bind(nickname)
        .execute(database)
        .await?;

    Ok(result.rows_affected() > 0)
}

/// Get a server ID from a server nickname
pub(crate) async fn get_server_id_from_nickname(
    database: &Database,
    user_id: impl Into<u64>,
    nickname: &str,
) -> Result<Option<GuildId>> {
    let result: Option<ServerNickname> = sqlx::query_as("SELECT user_id, guild_id, nickname FROM server_nicknames WHERE nickname = $1 AND user_id = $2")
        .bind(nickname)
        .bind(user_id.into() as i64)
        .fetch_optional(database)
        .await?;

    Ok(result.map(|n| n.guild_id()))
}

/// Get a user's set server nickname
pub(crate) async fn get_server_nickname<A, B>(
    database: &Database,
    user_id: A,
    guild_id: B,
) -> Result<Option<ServerNickname>>
where
    A: Into<u64> + Copy,
    B: Into<u64> + Copy,
{
    sqlx::query_as("SELECT user_id, guild_id, nickname FROM server_nicknames WHERE guild_id = $1 AND user_id = $2")
        .bind(guild_id.into() as i64)
        .bind(user_id.into() as i64)
        .fetch_optional(database)
        .await
}

/// Delete a scheduled message completely.
pub(crate) async fn delete_scheduled_message(database: &Database, id: i32) -> Result<bool> {
    match get_scheduled_message(database, id).await? {
        Some(_) => {
            let result = sqlx::query("DELETE FROM scheduled_messages WHERE id = $1")
                .bind(id)
                .execute(database)
                .await?;

            Ok(result.rows_affected() > 0)
        },
        None => Ok(false),
    }
}

/// Flag a scheduled message as archived or already-sent, so that it cannot be sent again in future.
pub(crate) async fn archive_scheduled_message(database: &Database, id: i32) -> Result<bool> {
    let result = sqlx::query("UPDATE scheduled_messages SET archived = TRUE WHERE id = $1")
        .bind(id)
        .execute(database)
        .await?;

    Ok(result.rows_affected() > 0)
}

/// Update an existing scheduled message. Datetime and repeat should be validated before being stored.
pub(crate) async fn update_scheduled_message(
    database: &Database,
    id: i32,
    datetime: Option<DateTime<Utc>>,
    repeat: Option<String>,
    title: Option<String>,
    message: Option<String>,
    channel_id: Option<impl Into<u64>>,
) -> Result<bool> {
    let channel_id = channel_id.map(|cid| cid.into());
    match get_scheduled_message(database, id).await? {
        Some(mut record) => {
            if let Some(datetime) = datetime {
                record.datetime = datetime.to_rfc3339();
                record.archived = false;
            }

            if let Some(repeat) = repeat {
                record.repeat = repeat;
            }

            if let Some(title) = title {
                record.title = title;
            }

            if let Some(message) = message {
                record.message = message;
            }

            if let Some(channel_id) = channel_id {
                record.channel_id = channel_id;
            }

            let result = sqlx::query(
                "UPDATE scheduled_messages SET channel_id = $2, datetime = $3, repeat = $4, title = $5, message = $6, archived = $7 WHERE id = $1")
                .bind(id)
                .bind(record.channel_id as i64)
                .bind(record.datetime)
                .bind(record.repeat)
                .bind(record.title)
                .bind(record.message)
                .bind(record.archived)
                .execute(database)
                .await?;

            Ok(result.rows_affected() > 0)
        },
        None => Ok(false),
    }
}

/// Add a new scheduled message
pub(crate) async fn add_scheduled_message(
    database: &Database,
    user_id: impl Into<u64>,
    datetime: DateTime<Utc>,
    repeat: &str,
    title: &str,
    message: &str,
    channel_id: impl Into<u64>,
) -> Result<bool> {
    let result = sqlx::query(
        "INSERT INTO scheduled_messages (user_id, channel_id, datetime, repeat, title, message, archived) VALUES ($1, $2, $3, $4, $5, $6, FALSE)")
        .bind(user_id.into() as i64)
        .bind(channel_id.into() as i64)
        .bind(datetime.to_rfc3339())
        .bind(repeat)
        .bind(title)
        .bind(message)
        .execute(database)
        .await?;

    Ok(result.rows_affected() > 0)
}

/// Gets all currently set scheduled messages
pub(crate) async fn get_all_scheduled_messages(
    database: &Database,
) -> Result<Vec<ScheduledMessage>> {
    sqlx::query_as("SELECT id, user_id, channel_id, datetime, repeat, title, message, archived from scheduled_messages")
        .fetch_all(database)
        .await
}

/// Gets a list of all scheduled messages for a given user
pub(crate) async fn list_scheduled_messages_for_user(
    database: &Database,
    user_id: impl Into<u64>,
) -> Result<Vec<ScheduledMessageSummary>> {
    sqlx::query_as(
        "SELECT id, channel_id, datetime, repeat, title FROM scheduled_messages WHERE user_id = $1",
    )
    .bind(user_id.into() as i64)
    .fetch_all(database)
    .await
}

/// Get a scheduled message
pub(crate) async fn get_scheduled_message(
    database: &Database,
    id: i32,
) -> Result<Option<ScheduledMessage>> {
    sqlx::query_as("SELECT id, user_id, channel_id, datetime, repeat, title, message, archived FROM scheduled_messages WHERE id = $1")
        .bind(id)
        .fetch_optional(database)
        .await
}

/// Add or update a user setting in the user_settings table
pub(crate) async fn update_user_setting<Id>(
    database: &Database,
    user_id: Id,
    name: &str,
    value: &str,
) -> Result<bool>
where
    Id: Into<u64> + Copy,
{
    let query_string = match get_user_setting(database, user_id, name).await? {
        Some(entry) => {
            if entry.value == value {
                return Ok(false);
            }

            "UPDATE user_settings SET value = $3 WHERE user_id = $1 AND name = $2"
        },
        None => "INSERT INTO user_settings (user_id, name, value) VALUES ($1, $2, $3)",
    };

    let result = sqlx::query(query_string)
        .bind(user_id.into() as i64)
        .bind(name)
        .bind(value)
        .execute(database)
        .await?;

    Ok(result.rows_affected() > 0)
}

/// Retrieve a stored user setting from the user_settings table
pub(crate) async fn get_user_setting<Id>(
    database: &Database,
    user_id: Id,
    name: &str,
) -> Result<Option<UserSetting>>
where
    Id: Into<u64> + Copy,
{
    sqlx::query_as(
        "SELECT user_id, name, value FROM user_settings WHERE user_id = $1 AND name = $2",
    )
    .bind(user_id.into() as i64)
    .bind(name)
    .fetch_optional(database)
    .await
}

/// Store an entry in the Subscriptions table
pub(crate) async fn add_subscriber<Id>(database: &Database, user_id: Id) -> Result<bool>
where
    Id: Into<u64> + Copy,
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

/// Retrieve an entry from the Subscriptions table by UserId.
pub(crate) async fn get_subscriber<Id>(
    database: &Database,
    user_id: Id,
) -> Result<Option<Subscription>>
where
    Id: Into<u64> + Copy,
{
    sqlx::query_as("SELECT id, user_id FROM subscriptions WHERE user_id = $1")
        .bind(user_id.into() as i64)
        .fetch_optional(database)
        .await
}

/// Retrieve all entries from the Subscriptions table.
pub(crate) async fn list_subscribers(database: &Database) -> Result<Vec<Subscription>> {
    sqlx::query_as("SELECT id, user_id FROM subscriptions ORDER BY id").fetch_all(database).await
}

/// Delete an entry from the Subscriptions table.
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

/// Get all entries from the watchers table.
pub(crate) async fn list_watchers(database: &Database) -> Result<Vec<ThreadWatcher>> {
    sqlx::query_as("SELECT id, user_id, message_id, channel_id, guild_id, categories FROM watchers")
        .fetch_all(database)
        .await
}

/// Get all entries from the watchers table associated with a given UserId and GuildId.
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

/// Get an entry from the watchers table by channel and message ID.
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

/// Add a new entry to the watchers table.
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

/// Remove an entry from the watchers table.
pub(crate) async fn remove_watcher(database: &Database, watcher_id: i32) -> Result<u64> {
    let result = sqlx::query("DELETE FROM watchers WHERE id = $1")
        .bind(watcher_id)
        .execute(database)
        .await?;

    Ok(result.rows_affected())
}

/// Add a new entry to the threads table.
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

/// Update the category of an entry in the threads table.
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

/// Remove an entry from the threads table.
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

/// Remove all entries from the threads table for a given user and guild ID.
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

/// Get all entries from the threads table.
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

/// Get an entry from the threads table with a specific channel ID and user ID.
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

/// Get all users tracking a specific thread.
pub(crate) async fn get_users_tracking_thread(
    database: &Database,
    guild_id: impl Into<u64>,
    channel_id: impl Into<u64>,
) -> Result<Vec<UserId>> {
    let result: Vec<TrackedThreadUser> = sqlx::query_as(
        "SELECT user_id FROM threads WHERE channel_id = $1 AND guild_id = $2 ORDER BY id",
    )
    .bind(channel_id.into() as i64)
    .bind(guild_id.into() as i64)
    .fetch_all(database)
    .await?;

    Ok(result.into_iter().map(|user| user.into()).collect())
}

/// Get all unique channel_ids from tracked threads (globally).
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
