use std::collections::BTreeMap;

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
    messaging::{send_success_embed, send_error_embed}, utils::partition_into_map,
};

pub(crate) struct Todo {
    pub content: String,
    pub category: Option<String>,
}

impl From<db::TodoRow> for Todo {
    fn from(value: db::TodoRow) -> Self {
        Self {
            content: value.content,
            category: value.category,
        }
    }
}

pub(crate) async fn add<'a>(
    args: &str,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &Database
) -> anyhow::Result<()> {
    let mut entry = args.trim();
    if entry.is_empty() {
        return Err(MissingArguments(String::from("Please provide a to do entry, such as: `tt!todo write X a starter`")).into());
    }

    let category = match entry.split_ascii_whitespace().next() {
        Some(s) if s.starts_with('!') => {
            // Remove the category name from the todo entry
            entry = entry[s.len()..].trim_start();

            // Strip the leading ! from the category name
            Some(&s[1..])
        },
        _ => None,
    };

    let mut result = MessageBuilder::new();
    result.push("To do list entry ").push(Italic + entry);
    match db::add_todo(database, guild_id.0, user_id.0, entry, category).await? {
        true => {
            if let Some(c) = category {
                result.push(" added to category ")
                    .push(Bold + Underline + c)
                    .push_line(" successfully.");
            }
            else {
                result.push_line(" added successfully.");
            }
            send_success_embed(&ctx.http, channel_id, "To do list entry added", result).await;
        },
        false => {
            result.push(" was not added as it is already present.");
            send_error_embed(&ctx.http, channel_id, "Error adding to do list entry", result).await;
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
    result.push("To do list entry ").push(Italic + entry);
    match db::remove_todo(database, guild_id.0, user_id.0, entry).await? {
        0 => {
            result.push_line(" was not found.");
            send_error_embed(&ctx.http, channel_id, "Error removing to do list entry", result).await;
        },
        _ => {
            result.push_line(" was successfully removed.");
            send_success_embed(&ctx.http, channel_id, "To do list entry removed", result).await;
        },
    };

    Ok(())
}

pub(crate) async fn send_list(
    args: Vec<&str>,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &Database
) -> anyhow::Result<()> {
    let mut args = args.into_iter().peekable();

    let todos = if args.peek().is_some() {
        list(database, guild_id, user_id, Some(args.collect())).await?
    }
    else {
        list(database, guild_id, user_id, None).await?
    };


    let mut message = MessageBuilder::new();

    if !todos.is_empty() {
        let categories = categorise(todos);
        message.mention(&user_id).push_line("'s to do list:");

        for (name, todos) in categories {
            if let Some(n) = name {
                message.push_line(Bold + Underline + n)
                    .push_line("");
            }

            for item in todos {
                push_todo_line(&mut message, &item);
            }

            message.push_line("");
        }
    }
    else {
        message.push_line("There is nothing on your to do list.");
    }

    send_success_embed(&ctx.http, channel_id, "To Do list", message).await;

    Ok(())
}

pub(crate) fn categorise(todos: Vec<Todo>) -> BTreeMap<Option<String>, Vec<Todo>> {
    partition_into_map(todos, |t| t.category.clone())
}

pub(crate) async fn list(database: &Database, guild_id: GuildId, user_id: UserId, categories: Option<Vec<&str>>) -> anyhow::Result<Vec<Todo>> {
    let mut result = Vec::new();

    match categories {
        Some(cats) => {
            for category in cats {
                result.extend(enumerate(database, guild_id, user_id, Some(category)).await?);
            }
        },
        None => result.extend(enumerate(database, guild_id, user_id, None).await?),
    }

    Ok(result)
}

async fn enumerate(database: &Database, guild_id: GuildId, user_id: UserId, category: Option<&str>) -> anyhow::Result<impl Iterator<Item = Todo>> {
    Ok(
        db::list_todos(database, guild_id.0, user_id.0, category).await?
            .into_iter()
            .map(|t| t.into())
    )
}

pub(crate) fn push_todo_line<'a>(message: &'a mut MessageBuilder, todo: &Todo) -> &'a mut MessageBuilder {
    message.push_quote_line(format!("• {}", &todo.content))
}
