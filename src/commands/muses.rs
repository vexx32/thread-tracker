use anyhow::anyhow;
use serenity::{
    model::prelude::{GuildId, UserId},
    utils::{ContentModifier::*, MessageBuilder},
};
use tracing::{error, info};

use crate::{
    db::{self, Database},
    TitiContext,
    TitiError,
    TitiResponse,
};

/// Add a new muse to the user's list.
///
/// ### Arguments
///
/// - `command` - the slash command interaction data
/// - `bot` - the bot instance
#[poise::command(slash_command, guild_only, rename = "tt_addmuse")]
pub(crate) async fn add(
    ctx: TitiContext<'_>,
    #[description = "The name of the muse to add"]
    muse_name: String,
) -> Result<(), TitiError> {
    const ERROR_TITLE: &str = "Error adding muse";
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
            ctx.reply_success("Add muse", &result.build()).await;
            Ok(())
        },
        Ok(false) => {
            result.push(" is already known for ").mention(&user.id).push_line(".");
            let error = result.build();
            ctx.reply_error(ERROR_TITLE, &error).await;
            Err(anyhow!(error).into())
        },
        Err(e) => {
            error!("Error adding muse for {}: {}", user.name, e);
            errors.push_line(e);
            let error = errors.build();
            ctx.reply_error(ERROR_TITLE, &error).await;
            Err(anyhow!(error).into())
        },
    }
}

/// Removes a muse from the user's list.
///
/// ### Arguments
///
/// - `command` - the slash command interaction data
/// - `bot` - the bot instance
#[poise::command(slash_command, guild_only, rename = "tt_removemuse")]
pub(crate) async fn remove(
    ctx: TitiContext<'_>,
    #[description = "The name of the muse to remove"]
    muse_name: String,
) -> Result<(), TitiError> {
    const ERROR_TITLE: &str = "Error removing muse";
    let guild_id = match ctx.guild_id() {
        Some(id) => id,
        None => return Err(anyhow!("Unable to manage muses outside of a server").into()),
    };

    let user = ctx.author();
    let database = &ctx.data().database;

    info!("removing muse `{}` for {} ({})", &muse_name, user.name, user.id);

    let mut result = MessageBuilder::new();
    result.push("Muse ").push(Italic + &muse_name);
    match db::remove_muse(database, guild_id.0, user.id.0, &muse_name).await {
        Ok(0) => {
            result.push_line(" was not found.");
            let error = result.build();
            ctx.reply_error(ERROR_TITLE, &error).await;
            Err(anyhow!(error).into())
        },
        Ok(_) => {
            result.push_line(" was successfully removed.");
            ctx.reply_success("Muse removed", &result.build()).await;
            Ok(())
        },
        Err(e) => {
            result.push_line(" could not be removed due to an error: ").push_line(e);
            let error = result.build();
            ctx.reply_error(ERROR_TITLE, &error).await;
            Err(anyhow!(error).into())
        },
    }
}

/// Sends the list of muses as a reply to the received command.
#[poise::command(slash_command, guild_only, rename = "tt_list")]
pub(crate) async fn list(ctx: TitiContext<'_>) -> Result<(), TitiError> {
    const ERROR_TITLE: &str = "Error listing muses";
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
            result.push_line(format!("• {}", muse));
        }
    }
    else {
        result.push_line("You have not registered any muses yet.");
    }

    info!("sending muse list for {} ({})", user.name, user.id);
    ctx.reply_success("Registered muses", &result.build()).await;
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
