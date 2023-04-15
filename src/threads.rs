use std::collections::{BTreeMap, BTreeSet};

use lazy_static::lazy_static;
use rand::Rng;
use serenity::{
    http::{Http, CacheHttp},
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

    CommandError::*, EventData,
};

lazy_static!{
    static ref URL_REGEX: Regex = Regex::new("^https://discord.com/channels/").unwrap();
}

pub(crate) struct TrackedThread {
    pub channel_id: ChannelId,
    pub category: Option<String>,
    pub guild_id: GuildId,
}

impl From<db::TrackedThreadRow> for TrackedThread {
    fn from(thread: db::TrackedThreadRow) -> Self {
        Self {
            channel_id: ChannelId(thread.channel_id as u64),
            category: thread.category,
            guild_id: GuildId(thread.guild_id as u64),
        }
    }
}

pub(crate) async fn add(
    args: Vec<&str>,
    event_data: &EventData,
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
            channel = event_data.channel_id.mention()
        )).into());
    }

    let mut threads_added = MessageBuilder::new();
    let mut errors = MessageBuilder::new();

    for thread_id in args {
        if let Some(Ok(target_channel_id)) = thread_id.split('/').last().map(|x| x.parse()) {
            let thread = ChannelId(target_channel_id);
            match thread.to_channel(event_data.http()).await {
                Ok(_) => {
                    info!("Adding tracked thread {} for user {}", target_channel_id, event_data.user_id);
                    match db::add_thread(database, event_data.guild_id.0, target_channel_id, event_data.user_id.0, category).await {
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

    let reply_context = event_data.reply_context();
    if !errors.0.is_empty() {
        error!("Errors handling thread registration:\n{}", errors);
        reply_context.send_error_embed("Error adding tracked threads", errors).await;
    }

    let title = match category {
        Some(name) => format!("Tracked threads added to `{}`", name),
        None => "Tracked threads added".to_owned(),
    };

    reply_context.send_success_embed(title, threads_added).await;

    Ok(())
}

pub(crate) async fn set_category(
    args: Vec<&str>,
    event_data: &EventData,
    database: &Database
) -> Result<(), anyhow::Error> {
    let mut args = args.into_iter().peekable();

    let category = match args.next() {
        Some("unset" | "none") => None,
        Some(cat) => Some(cat),
        None => return Err(MissingArguments(format!("Please provide a category name and a thread or channel URL, such as: `tt!cat category {}`", event_data.channel_id.mention())).into()),
    };

    let mut threads_updated = MessageBuilder::new();
    let mut errors = MessageBuilder::new();

    for thread_id in args {
        if let Some(Ok(target_channel_id)) = thread_id.split('/').last().map(|x| x.parse()) {
            let thread = ChannelId(target_channel_id);
            match thread.to_channel(event_data.http()).await {
                Ok(_) => match db::update_thread_category(database, event_data.guild_id.0, target_channel_id, event_data.user_id.0, category).await {
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

    let reply_context = event_data.reply_context();
    if !errors.0.is_empty() {
        error!("Errors updating thread categories:\n{}", errors);
        reply_context.send_error_embed("Error updating thread category", errors).await;
    }

    let title = match category {
        Some(name) => format!("Tracked threads' category set to `{}`", name),
        None => String::from("Tracked threads' categories removed"),
    };

    reply_context.send_success_embed(title, threads_updated).await;

    Ok(())
}

pub(crate) async fn remove(args: Vec<&str>, event_data: &EventData, database: &Database) -> Result<(), anyhow::Error> {
    let mut args = args.into_iter().peekable();

    if args.peek().is_none() {
        return Err(MissingArguments(format!(
            "Please provide a thread or channel URL, for example: `tt!remove {:}` -- or use `tt!remove all` to untrack all threads.",
            event_data.channel_id.mention()
        )).into());
    }

    let reply_context = event_data.reply_context();
    if let Some(&"all") = args.peek() {
        db::remove_all_threads(database, event_data.guild_id.0, event_data.user_id.0, None).await?;
        reply_context.send_success_embed(
            "Tracked threads removed",
            &format!("All registered threads for user {:} removed.", event_data.user_id.mention())
        ).await;

        return Ok(());
    }

    let mut threads_removed = MessageBuilder::new();
    let mut errors = MessageBuilder::new();

    for thread_or_category in args {
        if !URL_REGEX.is_match(thread_or_category) {
            match db::remove_all_threads(database, event_data.guild_id.0, event_data.user_id.0, Some(thread_or_category)).await {
                Ok(0) => errors.push_line(format!("• No threads in category {} to remove", thread_or_category)),
                Ok(count) => threads_removed.push_line(format!("• All {} threads in category `{}` removed", count, thread_or_category)),
                Err(e) => errors.push_line(format!("• Unable to remove threads in category `{}`: {}", thread_or_category, e)),
            };
        }
        else if let Some(Ok(target_channel_id)) = thread_or_category.split('/').last().map(|x| x.parse()) {
            let thread = ChannelId(target_channel_id);
            match db::remove_thread(database, event_data.guild_id.0, target_channel_id, event_data.user_id.0).await {
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
        reply_context.send_error_embed("Error removing tracked threads", errors).await;
    }

    reply_context.send_success_embed("Tracked threads removed", threads_removed).await;

    Ok(())
}

pub(crate) async fn send_list(
    args: Vec<&str>,
    event_data: &EventData,
    database: &Database
) -> Result<Message, anyhow::Error> {
    send_list_with_title(args, "Currently tracked threads", event_data, database).await
}

pub(crate) async fn send_list_with_title(
    args: Vec<&str>,
    title: impl ToString,
    event_data: &EventData,
    database: &Database
) -> Result<Message, anyhow::Error> {
    let mut args = args.into_iter().peekable();

    let mut threads: Vec<TrackedThread> = Vec::new();

    if args.peek().is_some() {
        for category in args {
            threads.extend(
                db::list_threads(database, event_data.guild_id.0, event_data.user_id.0, Some(category)).await?
                    .into_iter()
                    .map(|t| t.into())
            );
        }
    }
    else {
        threads.extend(
            db::list_threads(database, event_data.guild_id.0, event_data.user_id.0, None).await?
                .into_iter()
                .map(|t| t.into())
        );
    }

    let user = event_data.user();
    let muses = muses::list(database, &user).await?;
    let todos = todos::categorise(todos::list(database, &user, None).await?);
    let message = get_formatted_list(threads, todos, muses, event_data.user_id, &event_data.context).await?;

    Ok(event_data.reply_context().send_message_embed(title, message).await?)
}

pub(crate) async fn get_random_thread(event_data: &EventData, database: &Database) -> anyhow::Result<Option<(String, TrackedThread)>> {
    let muses = muses::list(database, &event_data.user()).await?;
    let mut pending_threads = Vec::new();

    for thread in db::list_threads(database, event_data.guild_id.0, event_data.user_id.0, None).await?.into_iter().map(|t| t.into()) {
        let last_message_author = get_last_responder(&thread, event_data.http()).await;
        match last_message_author {
            Some(user) => {
                let last_author_name = get_nick_or_name(&user, event_data.guild_id, event_data.http()).await;
                if user.id != event_data.user_id && !muses.contains(&last_author_name) {
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

pub(crate) async fn send_random_thread(event_data: &EventData, database: &Database) -> anyhow::Result<()> {
    let mut message = MessageBuilder::new();
    let reply_context = event_data.reply_context();

    match get_random_thread(event_data, database).await? {
        None => {
            message.push("Congrats! You don't seem to have any threads that are waiting on your reply! :tada:");
            log_send_errors(reply_context.send_message_embed("No waiting threads", message).await);
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
            message.push_quote(get_thread_link(&thread, event_data.http()).await).push(" — ").push_line(Bold + last_author);

            log_send_errors(reply_context.send_message_embed("Random thread", message).await);
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
            if name.is_some() {
                for todo in todos {
                    todos::push_todo_line(&mut message, todo);
                }
            }
        }

        message.push_line("");
    }

    // Uncategorised todos at the end of the list
    if let Some(todos) = todos.get(&None) {
        message.push_line("")
            .push_line(Bold + Italic + Underline + "To Do")
            .push_line("");

        for todo in todos {
            todos::push_todo_line(&mut message, todo);
        }
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

async fn get_last_responder(thread: &TrackedThread, http: impl AsRef<Http>) -> Option<User> {
    // Default behaviour for retriever is to get most recent messages
    let last_message = thread.channel_id
        .messages(http, |retriever| retriever.limit(1)).await
        .map_or(None, |mut m| m.pop());

    last_message.map(|m| m.author)
}

async fn get_nick_or_name(user: &User, guild_id: GuildId, cache_http: impl CacheHttp) -> String {
    if user.bot {
        user.name.clone()
    }
    else {
        user.nick_in(cache_http, guild_id).await
            .unwrap_or(user.name.clone())
    }
}

async fn get_thread_link(thread: &TrackedThread, cache_http: impl CacheHttp) -> MessageBuilder {
    let mut link = MessageBuilder::new();
    let guild_channel = thread.channel_id
        .to_channel(cache_http).await
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
