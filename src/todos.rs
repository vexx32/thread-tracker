use serenity::{
    model::prelude::*,
    prelude::*,
    utils::{
        ContentModifier::*,
        MessageBuilder,
    },
};

use crate::{
    CommandError::*,

    db::{self, Database},
    messaging::{send_success_embed, send_error_embed},
};

pub(crate) async fn add<'a>(
    entry: &str,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &Database
) -> anyhow::Result<()> {
    if entry.trim().is_empty() {
        return Err(MissingArguments(String::from("Please provide a todo entry, such as: `tt!todo write X a starter`")).into());
    }

    let mut result = MessageBuilder::new();
    result.push("Todo list entry ").push(Italic + entry);
    match db::add_todo(database, guild_id.0, user_id.0, entry).await? {
        true => {
            result.push_line(" added successfully.");
            send_success_embed(&ctx.http, channel_id, "Todo added", result).await;
        },
        false => {
            result.push(" was not added as it is already present.");
            send_error_embed(&ctx.http, channel_id, "Error adding todo", result).await;
        },
    };

    Ok(())
}

pub(crate) async fn remove(
    entry: &str,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &Database
) -> anyhow::Result<()> {
    if entry.trim().is_empty() {
        return Err(MissingArguments(String::from("Please provide a todo entry to remove, such as: `tt!done write X a starter`")).into());
    }

    let mut result = MessageBuilder::new();
    result.push("Todo list entry ").push(Italic + entry);
    match db::remove_todo(database, guild_id.0, user_id.0, entry).await? {
        0 => {
            result.push_line(" was not found.");
            send_error_embed(&ctx.http, channel_id, "Error removing todo", result).await;
        },
        _ => {
            result.push_line(" was successfully removed.");
            send_success_embed(&ctx.http, channel_id, "Todo removed", result).await;
        },
    };

    Ok(())
}

pub(crate) async fn send_list(
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &Database
) -> anyhow::Result<()> {
    let mut result = MessageBuilder::new();
    let todos = list(guild_id, user_id, database).await?;

    result.mention(&user_id).push_line("'s todo list:");

    for item in todos {
        result.push_line(format!("• {}", item));
    }

    if result.0.is_empty() {
        result.push_line("There is nothing on your todo list.");
    }

    send_success_embed(&ctx.http, channel_id, "Todo list", result).await;

    Ok(())
}

pub(crate) async fn list(guild_id: GuildId, user_id: UserId, database: &Database) -> anyhow::Result<Vec<String>> {
    Ok(
        db::list_todos(database, guild_id.0, user_id.0).await?
            .into_iter()
            .map(|t| t.content)
            .collect()
    )
}
