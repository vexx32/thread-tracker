use serenity::{
    builder::CreateApplicationCommand,
    model::prelude::{
        command::{CommandOptionType, CommandType},
        interaction::application_command::{ApplicationCommandInteraction, CommandDataOptionValue},
    },
};

use crate::messaging::{HelpMessage, InteractionResponse};

pub fn register(command: &mut CreateApplicationCommand) -> &mut CreateApplicationCommand {
    command
        .name("tt_help")
        .description("Show the help information for Thread Tracker")
        .kind(CommandType::ChatInput)
        .create_option(|option| {
            option
                .name("category")
                .description("The specific help category (optional)")
                .kind(CommandOptionType::String)
                .add_string_choice("threads", "threads")
                .add_string_choice("muses", "muses")
                .add_string_choice("todos", "todos")
                .add_string_choice("main", "main")
        })
}

pub fn run(interaction: &ApplicationCommandInteraction) -> Vec<InteractionResponse> {
    InteractionResponse::help(HelpMessage::from_category(
        interaction
            .data
            .options
            .iter()
            .find(|option| option.name == "category")
            .and_then(|option| option.resolved.as_ref())
            .and_then(|option| match option {
                CommandDataOptionValue::String(s) => Some(s.as_str()),
                _ => None,
            }),
    ))
}
