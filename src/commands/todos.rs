use std::collections::BTreeMap;

use anyhow::anyhow;
use serenity::utils::{ContentModifier::*, MessageBuilder};
use tracing::{error, info};

use super::CommandResult;
use crate::{
    commands::CommandContext,
    db::{self, Todo},
    messaging::reply,
    utils::*,
    Database,
};

/// Add a new to do list entry.
#[poise::command(slash_command, guild_only, rename = "tt_todo", category = "Todo list")]
pub(crate) async fn add(
    ctx: CommandContext<'_>,
    #[description = "The content of the todo list item"] entry: String,
    #[description = "The category to track the todo list item under"] category: Option<String>,
) -> CommandResult<()> {
    let guild_id = match ctx.guild_id() {
        Some(id) => id,
        None => return Err(anyhow!("Unable to manage todo list items outside of a server").into()),
    };

    let data = ctx.data();
    let database = &data.database;
    let user = ctx.author();

    info!("adding todo list entry `{}` for {} ({})", entry, user.name, user.id);

    let mut result = MessageBuilder::new();
    let mut errors = MessageBuilder::new();
    result.push("Todo list entry ").push(Italic + &entry);
    match db::add_todo(database, guild_id.get(), user.id.get(), &entry, category.as_deref()).await {
        Ok(true) => {
            result.push_line(" added successfully.");
            reply(&ctx, "To do list entry added", &result.build()).await?;
            Ok(())
        },
        Ok(false) => {
            result.push(" was not added as it is already on your todo list.");
            Err(anyhow!(result.build()).into())
        },
        Err(e) => {
            error!("Error adding todo list item for {}: {}", user.name, e);
            errors.push_line(e.as_database_error().map(|x| x.to_string()).unwrap_or(String::from("unknown error")));
            Err(anyhow!(errors.build()).into())
        },
    }
}

/// Remove an existing to do list entry.
#[poise::command(slash_command, guild_only, rename = "tt_done", category = "Todo list")]
pub(crate) async fn remove(
    ctx: CommandContext<'_>,
    #[description = "The content of the todo list item to remove"] entry: Option<String>,
    #[description = "The category to remove all todo list items from"] category: Option<String>,
) -> CommandResult<()> {
    let guild_id = match ctx.guild_id() {
        Some(id) => id,
        None => return Err(anyhow!("Unable to manage todo list items outside of a server").into()),
    };

    let user = ctx.author();

    let data = ctx.data();
    let database = &data.database;
    let mut message = MessageBuilder::new();

    let result = if let Some(entry) = entry {
        info!("removing todo `{}` for {} ({})", entry, user.name, user.id);
        message.push("To do list entry ").push(Italic + &entry).push(" was ");

        db::remove_todo(database, guild_id.get(), user.id.get(), &entry).await
    }
    else if let Some(category) = category {
        info!("removing all todos in category `{}` for {} ({})", category, user.name, user.id);
        match category.as_str() {
            "all" => {
                message.push("To do list entries were ");
                db::remove_all_todos(database, guild_id.get(), user.id.get(), None).await
            },
            cat => {
                message.push(format!("To do list entries in category `{}` were ", cat));
                db::remove_all_todos(database, guild_id.get(), user.id.get(), Some(cat)).await
            },
        }
    }
    else {
        return Err(anyhow!("No to do list entry or category specified to remove.").into());
    };

    match result {
        Ok(0) => {
            message.push_line(" not found.");
            Err(anyhow!(message.build()).into())
        },
        Ok(num) => {
            message.push_line(format!(" successfully removed. {} entries deleted.", num));
            reply(&ctx, "To do list updated", &message.build()).await?;
            Ok(())
        },
        Err(e) => Err(anyhow!("Error updating to do list: {}", e).into()),
    }
}

/// Send the full to do list.
#[poise::command(slash_command, guild_only, rename = "tt_todolist", category = "Todo list")]
pub(crate) async fn list(
    ctx: CommandContext<'_>,
    #[description = "The category or categories"] category: Vec<String>,
) -> CommandResult<()> {
    let user = ctx.author();
    let guild_user = match ctx.guild_id() {
        Some(id) => GuildUser { user_id: user.id, guild_id: id },
        None => return Err(anyhow!("Unable to manage todo list items outside of a server").into()),
    };

    let data = ctx.data();
    let database = &data.database;
    let mut message = MessageBuilder::new();

    let result = if !category.is_empty() {
        let categories: Vec<&str> = category.iter().map(|s| s.as_str()).collect();
        info!(
            "sending todos in categories `{}` for {} ({})",
            categories.join(", "),
            user.name,
            user.id
        );
        get_todos(database, &guild_user, Some(categories)).await
    }
    else {
        info!("sending all todos for {} ({})", user.name, user.id);
        get_todos(database, &guild_user, None).await
    };

    match result {
        Ok(todos) => {
            if !todos.is_empty() {
                let categories = categorise(todos);
                message.mention(&user.id).push_line("'s to do list:");

                for (name, todos) in categories {
                    if let Some(n) = name {
                        message.push("## ").push_line(n).push_line("");
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

            reply(&ctx, "To do list", &message.build()).await?;
            Ok(())
        },
        Err(e) => {
            message.push("Error retrieving ").mention(&user.id).push(": ").push_line(e.to_string());
            Err(anyhow!(message.build()).into())
        },
    }
}

/// Partition the to do entries into categories.
pub(crate) fn categorise(todos: Vec<Todo>) -> BTreeMap<Option<String>, Vec<Todo>> {
    partition_into_map(todos, |t| t.category.clone())
}

/// Retrieve a list of all to do entries in the target categories.
pub(crate) async fn get_todos(
    database: &Database,
    user: &GuildUser,
    categories: Option<Vec<&str>>,
) -> anyhow::Result<Vec<Todo>> {
    let mut result = Vec::new();

    match categories {
        Some(cats) => {
            for category in cats {
                result.extend(
                    enumerate(database, user, Some(category.trim_start_matches('!'))).await?,
                );
            }
        },
        None => result.extend(enumerate(database, user, None).await?),
    }

    Ok(result)
}

/// Create an iterator over the to do list entries in the database for the given user and category.
pub(crate) async fn enumerate(
    database: &Database,
    user: &GuildUser,
    category: Option<&str>,
) -> anyhow::Result<impl Iterator<Item = Todo>> {
    Ok(db::list_todos(database, user.guild_id.get(), user.user_id.get(), category)
        .await?
        .into_iter())
}

/// Append a line to the message builder containing the to do list item's text.
pub(crate) fn push_todo_line<'a>(
    message: &'a mut MessageBuilder,
    todo: &Todo,
) -> &'a mut MessageBuilder {
    message.push_line(format!("- {}", &todo.content))
}
