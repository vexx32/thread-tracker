use serenity::{
    builder::CreateApplicationCommands,
    model::prelude::{
        command::{CommandOptionType, CommandType},
        interaction::application_command::ApplicationCommandInteraction,
        GuildId,
        UserId,
    },
    utils::{ContentModifier::*, MessageBuilder},
};
use tracing::{error, info};

use crate::{
    db::{self, Database},
    messaging::InteractionResponse,
    utils::find_string_option,
    ThreadTrackerBot,
};

pub fn register_commands(
    commands: &mut CreateApplicationCommands,
) -> &mut CreateApplicationCommands {
    commands
        .create_application_command(|command| {
            command
                .name("tt_addmuse")
                .description("Add a new muse")
                .kind(CommandType::ChatInput)
                .create_option(|option| {
                    option
                        .name("name")
                        .description("The name of the muse to add to your list")
                        .kind(CommandOptionType::String)
                        .required(true)
                })
        })
        .create_application_command(|command| {
            command
                .name("tt_removemuse")
                .description("Remove a muse")
                .kind(CommandType::ChatInput)
                .create_option(|option| {
                    option
                        .name("name")
                        .description("The name of the muse to remove from your list")
                        .kind(CommandOptionType::String)
                        .required(true)
                })
        })
        .create_application_command(|command| {
            command
                .name("tt_muses")
                .description("Show your currently tracked muses")
                .kind(CommandType::ChatInput)
        })
}

/// Add a new muse to the user's list.
///
/// ### Arguments
///
/// - `command` - the slash command interaction data
/// - `bot` - the bot instance
pub(crate) async fn add(
    command: &ApplicationCommandInteraction,
    bot: &ThreadTrackerBot,
) -> Vec<InteractionResponse> {
    const ERROR_TITLE: &str = "Error adding muse";
    let guild_id = match command.guild_id {
        Some(id) => id,
        None => {
            return InteractionResponse::error(
                ERROR_TITLE,
                "Unable to manage muses outside of a server",
            )
        },
    };
    let database = &bot.database;

    let name_option = find_string_option(&command.data.options, "name");

    if let Some(muse_name) = name_option {
        info!("adding muse `{}` for {} ({})", muse_name, command.user.name, command.user.id);

        let mut result = MessageBuilder::new();
        let mut errors = MessageBuilder::new();
        result.push("Muse ").push(Italic + muse_name);
        match db::add_muse(database, guild_id.0, command.user.id.0, muse_name).await {
            Ok(true) => {
                result.push_line(" added successfully.");
                InteractionResponse::reply("Add muse", result.build())
            },
            Ok(false) => {
                result.push(" is already known for ").mention(&command.user.id).push_line(".");
                InteractionResponse::error(ERROR_TITLE, result.build())
            },
            Err(e) => {
                error!("Error adding muse for {}: {}", command.user.name, e);
                errors.push_line(e);
                InteractionResponse::error(ERROR_TITLE, errors.build())
            },
        }
    }
    else {
        Vec::new()
    }
}

/// Removes a muse from the user's list.
///
/// ### Arguments
///
/// - `command` - the slash command interaction data
/// - `bot` - the bot instance
pub(crate) async fn remove(
    command: &ApplicationCommandInteraction,
    bot: &ThreadTrackerBot,
) -> Vec<InteractionResponse> {
    const ERROR_TITLE: &str = "Error removing muse";
    let guild_id = match command.guild_id {
        Some(id) => id,
        None => {
            return InteractionResponse::error(
                ERROR_TITLE,
                "Unable to manage muses outside of a server",
            )
        },
    };
    let database = &bot.database;

    let name_option = find_string_option(&command.data.options, "name");

    if let Some(muse_name) = name_option {
        info!("removing muse `{}` for {} ({})", muse_name, command.user.name, command.user.id);

        let mut result = MessageBuilder::new();
        result.push("Muse ").push(Italic + muse_name);
        match db::remove_muse(database, guild_id.0, command.user.id.0, muse_name).await {
            Ok(0) => {
                result.push_line(" was not found.");
                InteractionResponse::error(ERROR_TITLE, result.build())
            },
            Ok(_) => {
                result.push_line(" was successfully removed.");
                InteractionResponse::reply("Muse removed", result.build())
            },
            Err(e) => {
                result.push_line(" could not be removed due to an error: ").push_line(e);
                InteractionResponse::error(ERROR_TITLE, result.build())
            },
        }
    }
    else {
        Vec::new()
    }
}

/// Sends the list of muses as a reply to the received command.
///
/// ### Arguments
///
/// - `command` - the slash command interaction data
/// - `bot` - the bot instance
pub(crate) async fn list(
    command: &ApplicationCommandInteraction,
    bot: &ThreadTrackerBot,
) -> Vec<InteractionResponse> {
    const ERROR_TITLE: &str = "Error listing muses";
    let guild_id = match command.guild_id {
        Some(id) => id,
        None => {
            return InteractionResponse::error(
                ERROR_TITLE,
                "Unable to list muses outside of a server",
            )
        },
    };
    let database = &bot.database;

    let muses = match get_list(database, command.user.id, guild_id).await {
        Ok(m) => m,
        Err(e) => {
            return InteractionResponse::error(ERROR_TITLE, format!("Error listing muses: {}", e));
        },
    };

    let mut result = MessageBuilder::new();
    if !muses.is_empty() {
        result.push("Muses registered for ").mention(&command.user.id).push_line(":");

        for muse in muses {
            result.push_line(format!("• {}", muse));
        }
    }
    else {
        result.push_line("You have not registered any muses yet.");
    }

    info!("sending muse list for {} ({})", command.user.name, command.user.id);
    InteractionResponse::reply("Registered muses", result.build())
}

/// Get the list of muses for the user out of the database.
///
/// ### Arguments
///
/// - `database` - the database to get the list from
/// - `user` - the user to retrieve the list of muses for
pub(crate) async fn get_list(
    database: &Database,
    user_id: UserId,
    guild_id: GuildId,
) -> anyhow::Result<Vec<String>> {
    Ok(db::list_muses(database, guild_id.0, user_id.0)
        .await?
        .into_iter()
        .map(|m| m.muse_name)
        .collect())
}
