use std::collections::{BTreeMap, BTreeSet};

use lazy_static::lazy_static;
use rand::Rng;
use serenity::{
    model::prelude::*,
    prelude::*,
    utils::{MessageBuilder, EmbedMessageBuilding, ContentModifier::*},
};
use regex::Regex;
use tracing::{error, info};

use crate::{
    db::{self, Database},
    messaging::*,
    muses,
    todos::{self, Todo},
    utils::*,

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
        if let Some(Ok(target_channel_id)) = thread_id.split('/').last().map(|x| x.parse()) {
            let thread = ChannelId(target_channel_id);
            match thread.to_channel(&ctx.http).await {
                Ok(_) => {
                    info!("Adding tracked thread {} for user {}", target_channel_id, user_id);
                    match db::add_thread(database, guild_id.0, target_channel_id, user_id.0, category).await {
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

    if !errors.0.is_empty() {
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
        if let Some(Ok(target_channel_id)) = thread_id.split('/').last().map(|x| x.parse()) {
            let thread = ChannelId(target_channel_id);
            match thread.to_channel(&ctx.http).await {
                Ok(_) => match db::update_thread_category(database, guild_id.0, target_channel_id, user_id.0, category).await {
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

    if !errors.0.is_empty() {
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
        else if let Some(Ok(target_channel_id)) = thread_or_category.split('/').last().map(|x| x.parse()) {
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

    if !errors.0.is_empty() {
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
    context: &Context,
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
    let todos = todos::categorise(todos::list(database, guild_id, user_id, None).await?);
    let response = get_formatted_list(threads, todos, muses, user_id, context).await?;

    Ok(send_message_embed(&context.http, channel_id, embed_title, &response).await?)
}

pub(crate) async fn get_random_thread(user_id: UserId, guild_id: GuildId, database: &Database, context: &Context) -> anyhow::Result<Option<(String, TrackedThread)>> {
    let muses = muses::list(guild_id, user_id, database).await?;
    let mut pending_threads = Vec::new();

    for thread in db::list_threads(database, guild_id.0, user_id.0, None).await?.into_iter().map(|t| t.into()) {
        let last_message_author = get_last_responder(&thread, context).await;
        match last_message_author {
            Some(user) => {
                let last_author_name = get_nick_or_name(&user, guild_id, context).await;
                if user.id != user_id && !muses.contains(&last_author_name) {
                    pending_threads.push((last_author_name, thread));
                }
            },
            None => pending_threads.push((String::from("No replies yet"), thread)),
        }
    }

    if pending_threads.is_empty() {
        Ok(None)
    }
    else {
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..pending_threads.len());
        Ok(Some(pending_threads.remove(index)))
    }
}

pub(crate) async fn send_random_thread(
    user_id: UserId,
    guild_id: GuildId,
    channel_id: ChannelId,
    context: &Context,
    database: &Database
) -> anyhow::Result<()> {
    let mut message = MessageBuilder::new();

    match get_random_thread(user_id, guild_id, database, context).await? {
        None => {
            message.push("Congrats! You don't seem to have any threads that are waiting on your reply! :tada:");
            log_send_errors(send_message_embed(&context.http, channel_id, "No waiting threads", message).await);
        },
        Some((last_author, thread)) => {
            message.push("Titi has chosen... this thread");

            if let Some(category) = &thread.category {
                message.push(" from your ")
                    .push(Bold + Underline + category)
                    .push_line(" threads!");
            }
            else {
                message.push_line("!");
            }

            message.push_line("");
            message.push_quote(get_thread_link(&thread, context).await).push(" — ").push_line(Bold + last_author);

            log_send_errors(send_message_embed(&context.http, channel_id, "Random thread", message).await);
        },
    };

    Ok(())
}

pub(crate) async fn get_formatted_list(
    threads: Vec<TrackedThread>,
    todos: BTreeMap<Option<String>, Vec<Todo>>,
    muses: Vec<String>,
    user_id: UserId,
    context: &Context
) -> Result<String, SerenityError> {
    let threads = categorise(threads);
    let mut message = MessageBuilder::new();

    let mut categories = BTreeSet::new();
    for key in threads.keys() {
        categories.insert(key);
    }

    for key in todos.keys() {
        categories.insert(key);
    }

    for name in categories {
        if let Some(n) = name {
            message.push_line(Bold + Underline + n)
                .push_line("");
        }

        if let Some(threads) = threads.get(name) {
            for thread in threads {
                push_thread_line(&mut message, thread, context, user_id, &muses).await;
            }
        }

        if let Some(todos) = todos.get(name) {
            message.push_quote_line(Bold + Italic + Underline + "To Do");
            for todo in todos {
                todos::push_todo_line(&mut message, todo);
            }
        }

        message.push_line("");
    }

    if message.0.is_empty() {
        message.push_line("No threads are currently being tracked.");
    }

    Ok(message.to_string())
}

fn categorise(threads: Vec<TrackedThread>) -> BTreeMap<Option<String>, Vec<TrackedThread>> {
    partition_into_map(threads, |t| t.category.clone())
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

async fn get_last_responder(thread: &TrackedThread, context: &Context) -> Option<User> {
    // Default behaviour for retriever is to get most recent messages
    let last_message = thread.channel_id
        .messages(&context.http, |retriever| retriever.limit(1)).await
        .map_or(None, |mut m| m.pop());

    last_message.map(|m| m.author)
}

async fn get_nick_or_name(user: &User, guild_id: GuildId, context: &Context) -> String {
    if user.bot {
        user.name.clone()
    }
    else {
        user.nick_in(&context.http, guild_id).await
            .unwrap_or(user.name.clone())
    }
}

async fn get_thread_link(thread: &TrackedThread, context: &Context) -> MessageBuilder {
    let mut link = MessageBuilder::new();
    let guild_channel = thread.channel_id
        .to_channel(&context.http).await
        .map_or(None, |c| c.guild());
    match guild_channel {
        Some(gc) => {
            let name = trim_link_name(&gc.name);
            link.push_named_link(Bold + format!("#{}", name), format!("https://discord.com/channels/{}/{}", gc.guild_id, gc.id))
        },
        None => link.push(thread.channel_id.mention()),
    };

    link
}

async fn push_thread_line<'a>(
    message: &'a mut MessageBuilder,
    thread: &TrackedThread,
    context: &Context,
    user_id: UserId,
    muses: &[String],
) -> &'a mut MessageBuilder {
    let last_message_author = get_last_responder(thread, context).await;

    // Thread entries in blockquotes
    message.push_quote("• ")
        .push(get_thread_link(thread, context).await)
        .push(" — ");

    match last_message_author {
        Some(user) => {
            let last_author_name = get_nick_or_name(&user, thread.guild_id, context).await;
            if user.id == user_id || muses.contains(&last_author_name) {
                message.push_line(last_author_name)
            }
            else {
                message.push_line(Bold + last_author_name)
            }
        },
        None => message.push(Bold + "No replies yet"),
    }
}
