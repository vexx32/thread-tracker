use std::collections::BTreeMap;

use lazy_static::lazy_static;
use serenity::{
    model::prelude::*,
    prelude::*,
    utils::{MessageBuilder, EmbedMessageBuilding},
};
use regex::Regex;
use tracing::{error, info};

use crate::{
    db::{self, Database},
    messaging::*,
    muses,

    CommandError::*,
};

lazy_static!{
    static ref URL_REGEX: Regex = Regex::new("^https://discord.com/channels/").unwrap();
}

pub(crate) struct TrackedThread {
    pub channel_id: ChannelId,
    pub category: Option<String>,
    pub guild_id: GuildId,
    pub id: i32,
}

impl From<db::TrackedThreadRow> for TrackedThread {
    fn from(thread: db::TrackedThreadRow) -> Self {
        Self {
            channel_id: ChannelId(thread.channel_id as u64),
            category: thread.category,
            guild_id: GuildId(thread.guild_id as u64),
            id: thread.id,
        }
    }
}

pub(crate) async fn add<'a>(
    args: Vec<&str>,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &Database
) -> Result<(), anyhow::Error> {
    let mut args = args.into_iter().peekable();

    let category = if !URL_REGEX.is_match(args.peek().unwrap_or(&"")) {
        args.next()
    }
    else {
        None
    };

    if args.peek().is_none() {
        return Err(MissingArguments(format!(
            "Please provide a thread or channel URL, such as: `tt!add {channel}`, optionally alongside a category name: `tt!add category {channel}`",
            channel = channel_id.mention()
        )).into());
    }

    let mut threads_added = MessageBuilder::new();
    let mut errors = MessageBuilder::new();

    for thread_id in args {
        if let Some(Ok(target_channel_id)) = thread_id.split("/").last().and_then(|x| Some(x.parse())) {
            let thread = ChannelId(target_channel_id);
            match thread.to_channel(&ctx.http).await {
                Ok(_) => {
                    info!("Adding tracked thread {} for user {}", target_channel_id, user_id);
                    match db::add_thread(database, guild_id.0, target_channel_id, user_id.0, category.as_deref()).await {
                        Ok(true) => threads_added.push("• ").mention(&thread).push_line(""),
                        Ok(false) => threads_added.push("• Skipped ").mention(&thread).push_line(" as it is already being tracked"),
                        Err(e) => errors.push("• Failed to register thread ").mention(&thread).push_line_safe(format!(": {}", e)),
                    }
                },
                Err(e) => errors.push("• Cannot access channel ").mention(&thread).push_line_safe(format!(": {}", e)),
            };
        }
        else {
            errors.push_line(format!("• Could not parse channel ID: `{}`", thread_id));
        }
    }

    if errors.0.len() > 0 {
        error!("Errors handling thread registration:\n{}", errors);
        send_error_embed(&ctx.http, channel_id, "Error adding tracked threads", errors).await;
    }

    let title = match category {
        Some(name) => format!("Tracked threads added to `{}`", name),
        None => "Tracked threads added".to_owned(),
    };

    send_success_embed(&ctx.http, channel_id, title, threads_added).await;

    Ok(())
}

pub(crate) async fn set_category(
    args: Vec<&str>,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &Database
) -> Result<(), anyhow::Error> {
    let mut args = args.into_iter().peekable();

    let category = match args.next() {
        Some("unset" | "none") => None,
        Some(cat) => Some(cat),
        None => return Err(MissingArguments(format!("Please provide a category name and a thread or channel URL, such as: `tt!cat category {}`", channel_id.mention())).into()),
    };

    let mut threads_updated = MessageBuilder::new();
    let mut errors = MessageBuilder::new();

    for thread_id in args {
        if let Some(Ok(target_channel_id)) = thread_id.split("/").last().and_then(|x| Some(x.parse())) {
            let thread = ChannelId(target_channel_id);
            match thread.to_channel(&ctx.http).await {
                Ok(_) => match db::update_thread_category(database, guild_id.0, target_channel_id, user_id.0, category.as_deref()).await {
                    Ok(true) => threads_updated.push("• ").mention(&thread).push_line(""),
                    Ok(false) => errors.push("• ").mention(&thread).push_line(" is not currently being tracked"),
                    Err(e) => errors.push("• Failed to update thread category for ").mention(&thread).push_line_safe(format!(": {}", e)),
                },
                Err(e) => errors.push("• Cannot access channel ").mention(&thread).push_line(format!(": {}", e)),
            };
        }
        else {
            errors.push_line(format!("• Could not parse channel ID: {}", thread_id));
        }
    }

    if errors.0.len() > 0 {
        error!("Errors updating thread categories:\n{}", errors);
        send_error_embed(&ctx.http, channel_id, "Error updating thread category", errors).await;
    }

    let title = match category {
        Some(name) => format!("Tracked threads' category set to `{}`", name),
        None => String::from("Tracked threads' categories removed"),
    };

    send_success_embed(&ctx.http, channel_id, title, threads_updated).await;

    Ok(())
}

pub(crate) async fn remove(
    args: Vec<&str>,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &Database
) -> Result<(), anyhow::Error> {
    let mut args = args.into_iter().peekable();

    if args.peek().is_none() {
        return Err(MissingArguments(format!(
            "Please provide a thread or channel URL, for example: `tt!remove {:}` -- or use `tt!remove all` to untrack all threads.",
            channel_id.mention()
        )).into());
    }

    if let Some(&"all") = args.peek() {
        db::remove_all_threads(database, guild_id.0, user_id.0, None).await?;
        send_success_embed(
            &ctx.http,
            channel_id,
            "Tracked threads removed",
            &format!("All registered threads for user {:} removed.", user_id.mention())
        ).await;

        return Ok(());
    }

    let mut threads_removed = MessageBuilder::new();
    let mut errors = MessageBuilder::new();

    for thread_or_category in args {
        if !URL_REGEX.is_match(thread_or_category) {
            match db::remove_all_threads(database, guild_id.0, user_id.0, Some(thread_or_category)).await {
                Ok(0) => errors.push_line(format!("• No threads in category {} to remove", thread_or_category)),
                Ok(count) => threads_removed.push_line(format!("• All {} threads in category `{}` removed", count, thread_or_category)),
                Err(e) => errors.push_line(format!("• Unable to remove threads in category `{}`: {}", thread_or_category, e)),
            };
        }
        else if let Some(Ok(target_channel_id)) = thread_or_category.split("/").last().and_then(|x| Some(x.parse())) {
            let thread = ChannelId(target_channel_id);
            match db::remove_thread(database, guild_id.0, target_channel_id, user_id.0).await {
                Ok(0) => errors.push_line(format!("• {} is not currently being tracked", thread.mention())),
                Ok(_) => threads_removed.push_line(format!("• {:}", thread.mention())),
                Err(e) => errors.push_line(format!("• Failed to unregister thread {}: {}", thread.mention(), e)),
            };
        }
        else {
            errors.push_line(format!("• Could not parse channel ID: {}", thread_or_category));
        }
    }

    if errors.0.len() > 0 {
        error!("Errors handling thread removal:\n{}", errors);
        send_error_embed(&ctx.http, channel_id, "Error removing tracked threads", errors).await;
    }

    send_success_embed(&ctx.http, channel_id, "Tracked threads removed", threads_removed).await;

    Ok(())
}

pub(crate) async fn send_list(
    args: Vec<&str>,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    ctx: &Context,
    database: &Database
) -> Result<Message, anyhow::Error> {
    send_list_with_title(args, guild_id, user_id, channel_id, "Currently tracked threads", ctx, database).await
}

pub(crate) async fn send_list_with_title(
    args: Vec<&str>,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
    embed_title: impl ToString,
    ctx: &Context,
    database: &Database
) -> Result<Message, anyhow::Error> {
    let mut args = args.into_iter().peekable();

    let mut threads: Vec<TrackedThread> = Vec::new();

    if args.peek().is_some() {
        for category in args {
            threads.extend(
                db::list_threads(database, guild_id.0, user_id.0, Some(category)).await?
                    .into_iter()
                    .map(|t| t.into())
            );
        }
    }
    else {
        threads.extend(
            db::list_threads(database, guild_id.0, user_id.0, None).await?
                .into_iter()
                .map(|t| t.into())
        );
    }

    let muses = muses::list(guild_id, user_id, database).await?;
    let response = get_formatted_list(threads, muses, ctx).await?;

    Ok(send_message_embed(&ctx.http, channel_id, embed_title, &response).await?)
}

pub(crate) async fn get_formatted_list(threads: Vec<TrackedThread>, muses: Vec<String>, ctx: &Context) -> Result<String, SerenityError> {
    use serenity::utils::ContentModifier::*;

    let categories = categorise_threads(threads);
    let mut message = MessageBuilder::new();

    for (name, threads) in categories {
        match name {
            Some(n) => {
                message.push_line(Bold + Underline + n)
                    .push_line("");
            },
            None => {},
        }

        for thread in threads {
            // Default behaviour for retriever is to get most recent messages
            let last_message = thread.channel_id
                .messages(&ctx.http, |retriever| retriever.limit(1)).await
                .map_or(None, |mut m| m.pop());

            let last_message_author = match last_message {
                Some(message) => if message.author.bot {
                    message.author.name
                }
                else {
                    message.author
                        .nick_in(&ctx.http, thread.guild_id).await
                        .unwrap_or_else(|| message.author.name)
                },
                None => String::from("No replies yet"),
            };

            // Thread entries in blockquotes
            message.push_quote("");

            let guild_channel = thread.channel_id
                .to_channel(&ctx.http).await
                .map_or(None, |c| c.guild());
            match guild_channel {
                Some(gc) => {
                    let name = trim_link_name(&gc.name);
                    message.push_named_link(Bold + format!("#{}", name), format!("https://discord.com/channels/{}/{}", gc.guild_id, gc.id))
                },
                None => message.push(thread.channel_id.mention()),
            };

            if muses.iter().any(|m| m.contains(&last_message_author)) {
                message.push(" — ").push_line(last_message_author);
            }
            else {
                message.push(" — ").push_line(Bold + last_message_author);
            };
        }

        message.push_line("");
    }

    if message.0.len() == 0 {
        message.push_line("No threads are currently being tracked.");
    }

    Ok(message.to_string())
}

fn categorise_threads(threads: Vec<TrackedThread>) -> BTreeMap<Option<String>, Vec<TrackedThread>> {
    let mut categories: BTreeMap<Option<String>, Vec<TrackedThread>> = BTreeMap::new();

    for thread in threads {
        categories.entry(thread.category.clone()).or_default().push(thread);
    }

    categories
}

fn trim_link_name(name: &str) -> String {
    if name.chars().count() > 32 {
        let (cutoff, _) = name.char_indices().nth(31).unwrap();
        format!("{}…", &name[0..cutoff].trim())
    }
    else {
        name.to_owned()
    }
}
