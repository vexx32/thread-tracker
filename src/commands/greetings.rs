use serenity::utils::MessageBuilder;

use crate::commands::{CommandContext, CommandResult};

#[poise::command(
    prefix_command,
    broadcast_typing,
    discard_spare_arguments,
    user_cooldown = 5,
    aliases("hello!", "hi", "hi!", "hullo!", "hullo", "hey", "hey!", "yo", "yo!", "sup", "sup?"))]
pub(crate) async fn hello(ctx: CommandContext<'_>) -> CommandResult<()> {
    let user = ctx.author();

    let mut response = MessageBuilder::new();

    response.push("Hello ").mention(user).push_line("! How are you?");

    ctx.say(response.build()).await?;

    Ok(())
}
