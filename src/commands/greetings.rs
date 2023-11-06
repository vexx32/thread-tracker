use rand::Rng;
use serenity::{utils::MessageBuilder, prelude::Mentionable};

use crate::{commands::{CommandContext, CommandResult}, consts::greetings::MESSAGES};

#[poise::command(
    prefix_command,
    broadcast_typing,
    discard_spare_arguments,
    user_cooldown = 5,
    aliases("hello!", "hi", "hi!", "hullo!", "hullo", "hey", "hey!", "yo", "yo!", "sup", "sup?"))]
pub(crate) async fn hello(ctx: CommandContext<'_>) -> CommandResult<()> {
    let user = ctx.author().mention();

    let mut response = MessageBuilder::new();

    response.push_line(get_random_greeting().replace("{}", &user.to_string()));

    ctx.say(response.build()).await?;

    Ok(())
}

fn get_random_greeting() -> &'static str {
    let greetings = MESSAGES;
    let mut rng = rand::thread_rng();
    let index = rng.gen_range(0..greetings.len());

    greetings[index]
}
