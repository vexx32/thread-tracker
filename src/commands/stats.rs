use serenity::utils::{Content, MessageBuilder};
use tracing::info;

use crate::{
    commands::{CommandContext, CommandResult},
    db,
    messaging::reply,
};

/// Send the bot's current statistics
#[poise::command(slash_command, dm_only, owners_only, rename = "stats")]
pub(crate) async fn send_statistics(ctx: CommandContext<'_>) -> CommandResult<()> {
    let data = ctx.data();
    let stats = db::statistics(&data.database).await?;

    let mut message = MessageBuilder::new();

    write_stats_line(&mut message, "Unique Users", stats.users);
    write_stats_line(&mut message, "Servers (Active)", stats.servers);
    write_stats_line(&mut message, "Servers (Total)", data.guilds());
    write_stats_line(&mut message, "Threads (Unique)", stats.threads_distinct);
    write_stats_line(&mut message, "Threads (Total)", stats.threads_total);
    write_stats_line(&mut message, "Muses", stats.muses);
    write_stats_line(&mut message, "To Dos", stats.todos);
    write_stats_line(&mut message, "Watchers", stats.watchers);
    write_stats_line(&mut message, "Scheduled Messages", stats.scheduled_messages);

    let user = ctx.author();
    info!("sending bot statistics to {} ({})", &user.name, user.id);

    reply(&ctx, "Statistics", &message.build()).await?;

    Ok(())
}

/// Retrieve information on which servers the bot is a member of
#[poise::command(slash_command, dm_only, owners_only, rename = "servers")]
pub(crate) async fn send_servers(ctx: CommandContext<'_>) -> CommandResult<()> {
    Ok(poise::builtins::servers(ctx).await?)
}

/// Write a line to the message being built, in the format `- **<name>** - <value>`
fn write_stats_line(msg: &mut MessageBuilder, name: impl Into<Content>, value: impl ToString) {
    msg.push("- **").push(name).push("** — ").push_line(value.to_string());
}
