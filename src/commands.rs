pub(crate) mod help;
pub(crate) mod muses;
pub(crate) mod threads;
pub(crate) mod todos;
pub(crate) mod watchers;

use serenity::{
    builder::CreateApplicationCommands,
    model::application::interaction::application_command::*,
    prelude::Context,
    utils::Colour,
};
use tracing::error;

use crate::{messaging::InteractionResponse, ThreadTrackerBot};

pub fn register_commands(
    commands: &mut CreateApplicationCommands,
) -> &mut CreateApplicationCommands {
    commands.create_application_command(|command| help::register(command));

    threads::register_commands(commands);
    muses::register_commands(commands);
    todos::register_commands(commands);
    watchers::register_commands(commands);

    commands
}

pub(crate) async fn interaction(
    command: ApplicationCommandInteraction,
    bot: &ThreadTrackerBot,
    context: &Context,
) {
    let responses = match command.data.name.as_str() {
        "tt_track" => threads::add(&command, bot, context).await,
        "tt_untrack" => threads::remove(&command, bot).await,
        "tt_cat" | "tt_category" => threads::set_category(&command, &bot.database, context).await,
        "tt_replies" | "tt_threads" => threads::send_list(&command, bot, context).await,
        "tt_random" => threads::send_random_thread(&command, bot, context).await,
        "tt_watch" => watchers::add(&command, bot, context).await,
        "tt_watching" => watchers::list(&command, bot).await,
        "tt_unwatch" => watchers::remove(&command, bot, context).await,
        "tt_muses" => muses::list(&command, bot).await,
        "tt_addmuse" => muses::add(&command, bot).await,
        "tt_removemuse" => muses::remove(&command, bot).await,
        "tt_todo" => todos::add(&command, bot).await,
        "tt_done" => todos::remove(&command, bot).await,
        "tt_todos" | "tt_todolist" => todos::list(&command, bot).await,
        cmd => {
            if cmd.starts_with("tt?") {
                help::run(&command)
            }
            else {
                handle_unknown_command(cmd).await
            }
        },
    };

    let mut messages = responses.iter();
    if let Some(message) = messages.next() {
        let mut embed_colour = if message.is_error() { Colour::RED } else { Colour::PURPLE };
        let result = command
            .create_interaction_response(&context, |response| {
                response.interaction_response_data(|data| {
                    data.embed(|embed| {
                        embed
                            .title(message.title())
                            .description(message.content())
                            .colour(embed_colour)
                    })
                    .ephemeral(message.is_ephemeral())
                })
            })
            .await;

        log_interaction_response_errors(result);

        for message in responses {
            embed_colour = if message.is_error() { Colour::RED } else { Colour::PURPLE };
            let result = command
                .create_followup_message(&context, |response| {
                    response
                        .embed(|embed| {
                            embed
                                .title(message.title())
                                .description(message.content())
                                .colour(embed_colour)
                        })
                        .ephemeral(message.is_ephemeral())
                })
                .await;

            log_interaction_response_errors(result);
        }
    }
    else {
        error!(
            "Command '{}' resulted in no response!\nUser: {} ({})\nGuild: {}\nOptions: {:?}",
            command.data.name,
            command.user.name,
            command.user.id,
            command.guild_id.map_or("nil".to_owned(), |g| g.to_string()),
            command.data.options
        );
    }
}

async fn handle_unknown_command(command_name: &str) -> Vec<InteractionResponse> {
    error!("Received unknown command `{}`, check command mappings", command_name);
    InteractionResponse::error(
        "Unknown command",
        format!("The command `{}` is not recognised.", command_name),
    )
}

fn log_interaction_response_errors<T>(result: serenity::Result<T>) {
    if let Err(e) = result {
        error!("Error sending interaction response: {}", e);
    }
}
