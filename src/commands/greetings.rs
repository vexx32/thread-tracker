use rand::Rng;
use serenity::{prelude::Mentionable, utils::MessageBuilder};

use crate::{
    commands::{CommandContext, CommandResult},
    consts::greetings::MESSAGES,
};

#[poise::command(
    prefix_command,
    broadcast_typing,
    discard_spare_arguments,
    user_cooldown = 5,
    aliases("hello!", "hi", "hi!", "hullo!", "hullo", "hey", "hey!", "yo", "yo!", "sup", "sup?")
)]
/// Have Titi reply with a short greeting.
pub(crate) async fn hello(ctx: CommandContext<'_>) -> CommandResult<()> {
    let user = ctx.author().mention();

    let mut response = MessageBuilder::new();

    response.push_line(get_random_greeting().replace("{}", &user.to_string()));

    ctx.say(response.build()).await?;

    Ok(())
}

/// Retrieves a random greeting from the preset list.
fn get_random_greeting() -> &'static str {
    let greetings = MESSAGES;
    let mut rng = rand::rng();
    let index = rng.random_range(0..greetings.len());

    greetings[index]
}
