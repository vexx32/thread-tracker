use std::collections::BTreeMap;

use serenity::{
    utils::{
        ContentModifier::*,
        MessageBuilder,
    },
};

use crate::{
    CommandError::*,
    error_on_additional_arguments,

    db::{self, Database},
    utils::partition_into_map, EventData, GuildUser,
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

pub(crate) async fn add<'a>(args: &str, event_data: &EventData, database: &Database) -> anyhow::Result<()> {
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

    let reply_context = event_data.reply_context();
    let mut result = MessageBuilder::new();
    result.push("To do list entry ").push(Italic + entry);
    match db::add_todo(database, event_data.guild_id.0, event_data.user_id.0, entry, category).await? {
        true => {
            if let Some(c) = category {
                result.push(" added to category ")
                    .push(Bold + Underline + c)
                    .push_line(" successfully.");
            }
            else {
                result.push_line(" added successfully.");
            }
            reply_context.send_success_embed("To do list entry added", result).await;
        },
        false => {
            result.push(" was not added as it is already present.");
            reply_context.send_error_embed("Error adding to do list entry", result).await;
        },
    };

    Ok(())
}

pub(crate) async fn remove(entry: &str, event_data: &EventData, database: &Database) -> anyhow::Result<()> {
    let mut entry = entry.trim();
    if entry.is_empty() {
        return Err(MissingArguments(String::from(
            "Please provide a todo entry or category to remove, such as: `tt!done write X a starter` or: `tt!done !category`, or simply `tt!done !all`"
        )).into());
    }

    let category = match entry.split_ascii_whitespace().next() {
        Some(cat) if cat.starts_with('!') => {
            entry = entry[cat.len()..].trim();
            // After a category name / !all, no more arguments are recognised / allowed.
            error_on_additional_arguments(entry.trim().split_ascii_whitespace().collect())?;

            Some(&cat[1..])
        },
        _ => None,
    };

    let mut message = MessageBuilder::new();

    let result = match category {
        Some("all") => {
            message.push("To do-list entries were ");
            db::remove_all_todos(database, event_data.guild_id.0, event_data.user_id.0, None).await?
        },
        Some(cat) => {
            message.push(format!("To do list entries in category `{}` were ", cat));
            db::remove_all_todos(database, event_data.guild_id.0, event_data.user_id.0, Some(cat)).await?
        },
        None => {
            message.push("To do list entry was ").push(Italic + entry);
            db::remove_todo(database, event_data.guild_id.0, event_data.user_id.0, entry).await?
        },
    };

    let reply_context = event_data.reply_context();
    match result {
        0 => {
            message.push_line(" not found.");
            reply_context.send_error_embed("Error updating to do-list", message).await;
        },
        num => {
            message.push_line(format!(" successfully removed. {} entries deleted.", num));
            reply_context.send_success_embed("To do-list updated", message).await;
        },
    };

    Ok(())
}

pub(crate) async fn send_list(args: Vec<&str>, event_data: &EventData, database: &Database) -> anyhow::Result<()> {
    let mut args = args.into_iter().peekable();

    let todos = if args.peek().is_some() {
        list(database, &event_data.user(), Some(args.collect())).await?
    }
    else {
        list(database, &event_data.user(), None).await?
    };


    let mut message = MessageBuilder::new();

    if !todos.is_empty() {
        let categories = categorise(todos);
        message.mention(&event_data.user_id).push_line("'s to do list:");

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

    event_data.reply_context().send_success_embed("To Do list", message).await;

    Ok(())
}

pub(crate) fn categorise(todos: Vec<Todo>) -> BTreeMap<Option<String>, Vec<Todo>> {
    partition_into_map(todos, |t| t.category.clone())
}

pub(crate) async fn list(database: &Database, user: &GuildUser, categories: Option<Vec<&str>>) -> anyhow::Result<Vec<Todo>> {
    let mut result = Vec::new();

    match categories {
        Some(cats) => {
            for category in cats {
                result.extend(enumerate(database, user, Some(category)).await?);
            }
        },
        None => result.extend(enumerate(database, user, None).await?),
    }

    Ok(result)
}

async fn enumerate(database: &Database, user: &GuildUser, category: Option<&str>) -> anyhow::Result<impl Iterator<Item = Todo>> {
    Ok(
        db::list_todos(database, user.guild_id.0, user.user_id.0, category).await?
            .into_iter()
            .map(|t| t.into())
    )
}

pub(crate) fn push_todo_line<'a>(message: &'a mut MessageBuilder, todo: &Todo) -> &'a mut MessageBuilder {
    message.push_quote_line(format!("• {}", &todo.content))
}
