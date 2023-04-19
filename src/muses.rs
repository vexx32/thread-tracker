use serenity::{
    utils::{
        ContentModifier::*,
        MessageBuilder,
    },
};

use crate::{
    CommandError::*,

    db::{self, Database},
    utils::{EventData, GuildUser},
};

pub(crate) async fn add<'a>(args: Vec<&str>, event_data: &EventData, database: &Database) -> anyhow::Result<()> {
    if args.is_empty() {
        return Err(MissingArguments(String::from("Please provide a muse name, such as: `tt!addmuse Annie Grey`")).into());
    }

    let reply_context = event_data.reply_context();
    let muse_name = args.join(" ");

    let mut result = MessageBuilder::new();
    result.push("Muse ").push(Italic + &muse_name);
    match db::add_muse(database, event_data.guild_id.0, event_data.user_id.0, &muse_name).await? {
        true => {
            result.push_line(" added successfully.");
            reply_context.send_success_embed("Muse added", result).await;
        },
        false => {
            result.push(" is already known for ").mention(&event_data.user_id).push_line(".");
            reply_context.send_error_embed("Error adding muse", result).await;
        },
    };

    Ok(())
}

pub(crate) async fn remove(args: Vec<&str>, event_data: &EventData, database: &Database) -> anyhow::Result<()> {
    if args.is_empty() {
        return Err(MissingArguments(String::from("Please provide a muse name, such as: `tt!removemuse Annie Grey`")).into());
    }

    let muse_name = args.join(" ");
    let reply_context = event_data.reply_context();

    let mut result = MessageBuilder::new();
    result.push("Muse ").push(Italic + &muse_name);
    match db::remove_muse(database, event_data.guild_id.0, event_data.user_id.0, &muse_name).await? {
        0 => {
            result.push_line(" was not found.");
            reply_context.send_error_embed("Error removing muse", result).await;
        },
        _ => {
            result.push_line(" was successfully removed.");
            reply_context.send_success_embed("Muse removed", result).await;
        },
    };

    Ok(())
}

pub(crate) async fn send_list(event_data: &EventData, database: &Database) -> anyhow::Result<()> {
    let mut result = MessageBuilder::new();
    let muses = list(database, &event_data.user()).await?;

    if !muses.is_empty() {
        result.push("Muses registered for ").mention(&event_data.user_id).push_line(":");

        for muse in muses {
            result.push_line(format!("• {}", muse));
        }
    }
    else {
        result.push_line("You have not registered any muses yet.");
    }

    event_data.reply_context().send_success_embed("Registered muses", result).await;

    Ok(())
}

pub(crate) async fn list(database: &Database, user: &GuildUser) -> anyhow::Result<Vec<String>> {
    Ok(
        db::list_muses(database, user.guild_id.0, user.user_id.0).await?
            .into_iter()
            .map(|m| m.muse_name)
            .collect()
    )
}
