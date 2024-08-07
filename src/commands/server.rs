use poise::serenity_prelude::*;

use crate::{
    commands::*, db, messaging::*,
};

#[poise::command(slash_command, guild_only, rename = "tt_server", category = "Server", subcommands("name"))]
pub(crate) async fn server(ctx: CommandContext<'_>) -> CommandResult<()> {
    send_invalid_command_call_error(ctx).await
}

#[poise::command(slash_command, guild_only, category = "Server", subcommands("set_name", "get_name"))]
pub(crate) async fn name (ctx: CommandContext<'_>) -> CommandResult<()> {
    send_invalid_command_call_error(ctx).await
}

#[poise::command(slash_command, guild_only, rename = "set", category = "Server")]
pub(crate) async fn set_name(
    ctx: CommandContext<'_>,
    #[description = "The name to use to refer to this server; leave this blank to un-set the name."]
    name: Option<String>,
) -> CommandResult<()> {
    const REPLY_TITLE: &str = "Change server nickname";

    let data = ctx.data();
    let author = ctx.author();
    let Some(guild_id) = ctx.guild_id() else {
        return Err(CommandError::new("This command must be called from within a server."));
    };

    let mut message = MessageBuilder::new();

    match name {
        Some(name) => {
            if db::set_server_nickname(&data.database, author.id, guild_id, &name).await? {
                message.push_line("Server nickname updated.");
            }
            else {
                message.push_line(format!("Server nickname was already set to {}.", name));
            }
        },
        None => {
            if db::remove_server_nickname(&data.database, author.id, guild_id).await? {
                message.push_line("Server nickname removed.");
            }
            else {
                message.push_line("Server nickname could not be removed; it may not have been set.");
            }
        },
    }
;
    reply(&ctx, REPLY_TITLE, &message.build()).await?;

    Ok(())
}

#[poise::command(slash_command, guild_only, rename = "get", category = "Server")]
pub(crate) async fn get_name(ctx: CommandContext<'_>) -> CommandResult<()> {
    const REPLY_TITLE: &str = "Server nickname";

    let data = ctx.data();
    let author = ctx.author();
    let Some(guild_id) = ctx.guild_id() else {
        return Err(CommandError::new("This command must be called from within a server."));
    };

    let mut message = MessageBuilder::new();

    match db::get_server_nickname(&data.database, author.id, guild_id).await? {
        Some(name) => message.push_line(format!("The server nickname for this server is: {}", name.nickname)),
        None => message.push_line("No server nickname has been set for this server."),
    };

    reply(&ctx, REPLY_TITLE, &message.build()).await?;

    Ok(())
}
