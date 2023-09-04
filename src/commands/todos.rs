use std::collections::BTreeMap;

use serenity::{
    builder::CreateApplicationCommands,
    model::prelude::{
        command::{CommandOptionType, CommandType},
        interaction::application_command::ApplicationCommandInteraction,
    },
    utils::{ContentModifier::*, MessageBuilder},
};
use tracing::{error, info};

use crate::{
    db::{self},
    messaging::InteractionResponse,
    utils::*,
    Database,
    ThreadTrackerBot,
};

/// To do list entry from the database.
pub(crate) struct Todo {
    pub content: String,
    pub category: Option<String>,
}

impl From<db::TodoRow> for Todo {
    fn from(value: db::TodoRow) -> Self {
        Self { content: value.content, category: value.category }
    }
}

pub fn register_commands(
    commands: &mut CreateApplicationCommands,
) -> &mut CreateApplicationCommands {
    commands
        .create_application_command(|command| {
            command
                .name("tt_todo")
                .description("Add a new to do list item")
                .kind(CommandType::ChatInput)
                .create_option(|option| {
                    option
                        .name("entry")
                        .description("The text for the to do list item")
                        .kind(CommandOptionType::String)
                        .required(true)
                })
                .create_option(|option|
                    option
                        .name("category")
                        .description("The category to place the to do list item in")
                        .kind(CommandOptionType::String))
        })
        .create_application_command(|command| {
            command
                .name("tt_done")
                .description("Cross off a to do list item, or remove an entire category of todo list items")
                .kind(CommandType::ChatInput)
                .create_option(|option| option
                    .name("entry")
                    .description("Untrack a specific to do list entry")
                    .kind(CommandOptionType::String))
                .create_option(|option| option
                    .name("category")
                    .description("The category to remove to do list items from; use 'all' to remove all todo list items")
                    .kind(CommandOptionType::String))
        })
        .create_application_command(|command| {
            command
                .name("tt_todolist")
                .description("Show your current to do list")
                .kind(CommandType::ChatInput)
                .create_option(|option| option
                    .name("category")
                    .description("The category to show to do list items from, omit this to show all to do list items")
                    .kind(CommandOptionType::String))
        })
}

/// Add a new to do list entry.
///
/// ### Arguments
///
/// - `command` - the slash command interaction data
/// - `bot` - the bot instance
pub(crate) async fn add(
    command: &ApplicationCommandInteraction,
    bot: &ThreadTrackerBot,
) -> Vec<InteractionResponse> {
    const ERROR_TITLE: &str = "Error adding todo list entry";
    let guild_id = match command.guild_id {
        Some(id) => id,
        None => {
            return InteractionResponse::error(
                ERROR_TITLE,
                "Unable to manage todo list items outside of a server",
            )
        },
    };
    let database = &bot.database;

    let todo_text = find_string_option(&command.data.options, "entry");
    let category = find_string_option(&command.data.options, "category");

    if let Some(text) = todo_text {
        info!("adding todo list entry `{}` for {} ({})", text, command.user.name, command.user.id);

        let mut result = MessageBuilder::new();
        let mut errors = MessageBuilder::new();
        result.push("Todo list entry ").push(Italic + text);
        match db::add_todo(database, guild_id.0, command.user.id.0, text, category)
            .await
        {
            Ok(true) => {
                result.push_line(" added successfully.");
                InteractionResponse::reply("To do list entry added", result.build())
            },
            Ok(false) => {
                result.push(" was not added as it is already on your todo list.");
                InteractionResponse::error(ERROR_TITLE, result.build())
            },
            Err(e) => {
                error!("Error adding todo list item for {}: {}", command.user.name, e);
                errors.push_line(e);
                InteractionResponse::error(ERROR_TITLE, errors.build())
            },
        }
    }
    else {
        Vec::new()
    }
}

/// Remove an existing to do list entry.
///
/// ### Arguments
///
/// - `command` - the slash command interaction data
/// - `bot` - the bot instance
pub(crate) async fn remove(
    command: &ApplicationCommandInteraction,
    bot: &ThreadTrackerBot,
) -> Vec<InteractionResponse> {
    const ERROR_TITLE: &str = "Error removing todo list entry";
    let guild_id = match command.guild_id {
        Some(id) => id,
        None => {
            return InteractionResponse::error(
                ERROR_TITLE,
                "Unable to manage todo list items outside of a server",
            )
        },
    };

    let database = &bot.database;
    let mut message = MessageBuilder::new();

    let result = if let Some(entry) = find_string_option(&command.data.options, "entry") {
        info!("removing todo `{}` for {} ({})", entry, command.user.name, command.user.id);
        message.push("To do list entry ").push(Italic + entry).push(" was ");

        db::remove_todo(database, guild_id.0, command.user.id.0, entry).await
    }
    else if let Some(category) = find_string_option(&command.data.options, "category") {
        info!(
            "removing all todos in category `{}` for {} ({})",
            category, command.user.name, command.user.id
        );
        match category {
            "all" => {
                message.push("To do list entries were ");
                db::remove_all_todos(database, guild_id.0, command.user.id.0, None).await
            },
            cat => {
                message.push(format!("To do list entries in category `{}` were ", cat));
                db::remove_all_todos(database, guild_id.0, command.user.id.0, Some(cat)).await
            },
        }
    }
    else {
        return InteractionResponse::error(
            ERROR_TITLE,
            "No to do list entry or category specified to remove.",
        );
    };

    match result {
        Ok(0) => {
            message.push_line(" not found.");
            InteractionResponse::error(ERROR_TITLE, message.build())
        },
        Ok(num) => {
            message.push_line(format!(" successfully removed. {} entries deleted.", num));
            InteractionResponse::reply("To do list updated", message.build())
        },
        Err(e) => {
            InteractionResponse::error(ERROR_TITLE, format!("Error updating to do list: {}", e))
        },
    }
}

/// Send the full to do list.
///
/// ### Arguments
///
/// - `command` - the slash command interaction data
/// - `bot` - the bot instance
pub(crate) async fn list(
    command: &ApplicationCommandInteraction,
    bot: &ThreadTrackerBot,
) -> Vec<InteractionResponse> {
    const ERROR_TITLE: &str = "Error getting todo list";
    let user = match command.guild_id {
        Some(id) => GuildUser { user_id: command.user.id, guild_id: id },
        None => {
            return InteractionResponse::error(
                ERROR_TITLE,
                "Unable to manage todo list items outside of a server",
            )
        },
    };

    let database = &bot.database;
    let mut message = MessageBuilder::new();

    let result = if let Some(category) = find_string_option(&command.data.options, "category") {
        let categories: Vec<&str> = category.split_whitespace().collect();
        info!(
            "sending todos in categories `{}` for {} ({})",
            categories.join(", "),
            command.user.name,
            command.user.id
        );
        get_todos(database, &user, Some(categories)).await
    }
    else {
        info!("sending all todos for {} ({})", command.user.name, command.user.id);
        get_todos(database, &user, None).await
    };

    match result {
        Ok(todos) => {
            if !todos.is_empty() {
                let categories = categorise(todos);
                message.mention(&command.user.id).push_line("'s to do list:");

                for (name, todos) in categories {
                    if let Some(n) = name {
                        message.push_line(Bold + Underline + n).push_line("");
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

            InteractionResponse::reply("To do list", message.build())
        },
        Err(e) => {
            message.push("Error retrieving ").mention(&command.user.id).push(": ").push_line(e);
            InteractionResponse::error(ERROR_TITLE, message.build())
        },
    }
}

/// Partition the to do entries into categories.
pub(crate) fn categorise(todos: Vec<Todo>) -> BTreeMap<Option<String>, Vec<Todo>> {
    partition_into_map(todos, |t| t.category.clone())
}

/// Retrieve a list of all to do entries in the target categories.
///
/// ### Arguments
///
/// - `database` - the database to query
/// - `user` - the user to query to do entries for
/// - `categories` - an optional list of categories to find to do entries in
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
    Ok(db::list_todos(database, user.guild_id.0, user.user_id.0, category)
        .await?
        .into_iter()
        .map(|t| t.into()))
}

/// Append a line to the message builder containing the to do list item's text.
pub(crate) fn push_todo_line<'a>(
    message: &'a mut MessageBuilder,
    todo: &Todo,
) -> &'a mut MessageBuilder {
    message.push_line(format!("- {}", &todo.content))
}
