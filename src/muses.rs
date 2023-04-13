use serenity::{
    model::prelude::*,
    prelude::*,
    utils::{
        ContentModifier::*,
        MessageBuilder,
    },
};

use crate::{
    CommandError::*,

    db::{self, Database},
    messaging::{send_success_embed, send_error_embed}, error_on_additional_arguments,
};

pub(crate) async fn add<'a>(
    args: Vec<&str>,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &Database
) -> anyhow::Result<()> {
    if args.len() == 0 {
        return Err(MissingArguments(String::from("Please provide a muse name, such as: `tt!addmuse Annie Grey`")).into());
    }

    let muse_name = args.join(" ");

    let mut result = MessageBuilder::new();
    result.push("Muse ").push(Italic + &muse_name);
    match db::add_muse(database, guild_id.0, user_id.0, &muse_name).await? {
        true => {
            result.push_line(" added successfully.");
            send_success_embed(&ctx.http, channel_id, "Muse added", result).await;
        },
        false => {
            result.push(" is already known for ").mention(&user_id).push_line(".");
            send_error_embed(&ctx.http, channel_id, "Error adding muse", result).await;
        },
    };

    Ok(())
}

pub(crate) async fn remove(
    args: Vec<&str>,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &Database
) -> anyhow::Result<()> {
    if args.len() == 0 {
        return Err(MissingArguments(String::from("Please provide a muse name, such as: `tt!removemuse Annie Grey`")).into());
    }

    let muse_name = args.join(" ");

    let mut result = MessageBuilder::new();
    result.push("Muse ").push(Italic + &muse_name);
    match db::remove_muse(database, guild_id.0, user_id.0, &muse_name).await? {
        0 => {
            result.push_line(" was not found.");
            send_error_embed(&ctx.http, channel_id, "Error removing muse", result).await;
        },
        _ => {
            result.push_line(" was successfully removed.");
            send_success_embed(&ctx.http, channel_id, "Muse removed", result).await;
        },
    };

    Ok(())
}

pub(crate) async fn send_list(
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &Database
) -> anyhow::Result<()> {
    let mut result = MessageBuilder::new();
    let muses = list(guild_id, user_id, database, ctx).await?;

    result.push("Muses registered for ").mention(&user_id).push_line(":");

    for muse in muses {
        result.push_line(format!("• {}", muse));
    }

    if result.0.len() == 0 {
        result.push_line("You have not registered any muses yet.");
    }

    send_success_embed(&ctx.http, channel_id, "Registered muses", result).await;

    Ok(())
}

pub(crate) async fn list(guild_id: GuildId, user_id: UserId, database: &Database, context: &Context) -> anyhow::Result<Vec<String>> {
    let user = user_id.to_user(&context.http).await?;
    let mut muses: Vec<String> = db::list_muses(database, guild_id.0, user_id.0).await?
        .into_iter()
        .map(|m| m.muse_name)
        .collect();

    muses.push(user.nick_in(&context.http, guild_id).await.unwrap_or(user.name));

    Ok(muses)
}
