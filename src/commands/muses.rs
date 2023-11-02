use anyhow::anyhow;
use serenity::{
    model::prelude::{GuildId, UserId},
    utils::{ContentModifier::*, MessageBuilder},
};
use tracing::{error, info};

use crate::{
    commands::CommandResult,
    db::{self, Database},
    CommandContext, messaging::reply,
};


/// Add a new muse to your list.
#[poise::command(slash_command, guild_only, rename = "tt_addmuse", category = "Muses")]
pub(crate) async fn add(
    ctx: CommandContext<'_>,
    #[description = "The name of the muse to add"]
    muse_name: String,
) -> CommandResult<()> {
    let guild_id = match ctx.guild_id() {
        Some(id) => id,
        None => return Err(anyhow!("Unable to manage muses outside of a server").into()),
    };

    let user = ctx.author();
    let database = &ctx.data().database;

    info!("adding muse `{}` for {} ({})", &muse_name, user.name, user.id);

    let mut result = MessageBuilder::new();
    let mut errors = MessageBuilder::new();
    result.push("Muse ").push(Italic + &muse_name);
    match db::add_muse(database, guild_id.0, user.id.0, &muse_name).await {
        Ok(true) => {
            result.push_line(" added successfully.");
            reply(&ctx, "Add muse", &result.build()).await?;
            Ok(())
        },
        Ok(false) => {
            result.push(" is already known for ").mention(&user.id).push_line(".");
            let error = result.build();
            Err(anyhow!(error).into())
        },
        Err(e) => {
            error!("Error adding muse for {}: {}", user.name, e);
            errors.push_line(e);
            let error = errors.build();
            Err(anyhow!(error).into())
        },
    }
}

/// Removes a muse from your list.
#[poise::command(slash_command, guild_only, rename = "tt_removemuse", category = "Muses")]
pub(crate) async fn remove(
    ctx: CommandContext<'_>,
    #[description = "The name of the muse to remove"]
    muse_name: String,
) -> CommandResult<()> {
    let guild_id = match ctx.guild_id() {
        Some(id) => id,
        None => return Err(anyhow!("Unable to manage muses outside of a server").into()),
    };

    let user = ctx.author();
    let database = &ctx.data().database;

    info!("removing muse `{}` for {} ({})", &muse_name, user.name, user.id);

    let mut result = MessageBuilder::new();
    result.push("Muse ").push(Italic + &muse_name);
    match db::remove_muse(database, guild_id.0, user.id.0, &muse_name).await? {
        0 => {
            result.push_line(" was not found.");
            let error = result.build();
            Err(anyhow!(error).into())
        },
        _ => {
            result.push_line(" was successfully removed.");
            reply(&ctx, "Muse removed", &result.build()).await?;
            Ok(())
        }
    }
}

/// Show your list of muses.
#[poise::command(slash_command, guild_only, rename = "tt_muses", category = "Muses")]
pub(crate) async fn list(ctx: CommandContext<'_>) -> CommandResult<()> {
    let guild_id = match ctx.guild_id() {
        Some(id) => id,
        None => return Err(anyhow!("Unable to list muses outside of a server").into()),
    };
    let database = &ctx.data().database;
    let user = ctx.author();

    let muses = match get_list(database, user.id, guild_id).await {
        Ok(m) => m,
        Err(e) => return Err(anyhow!("Error listing muses: {}", e).into()),
    };

    let mut result = MessageBuilder::new();
    if !muses.is_empty() {
        result.push("Muses registered for ").mention(&user.id).push_line(":");

        for muse in muses {
            result.push_line(format!("- {}", muse));
        }
    }
    else {
        result.push_line("You have not registered any muses yet.");
    }

    info!("sending muse list for {} ({})", user.name, user.id);
    reply(&ctx, "Registered muses", &result.build()).await?;

    Ok(())
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
