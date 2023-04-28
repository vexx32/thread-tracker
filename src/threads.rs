use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

use rand::Rng;
use serenity::{
    http::{CacheHttp, Http},
    model::prelude::*,
    prelude::*,
    utils::{ContentModifier::*, EmbedMessageBuilding, MessageBuilder},
};
use tracing::{error, info};

use crate::{
    cache::MessageCache,
    consts::THREAD_NAME_LENGTH,
    db::{self, Database},
    error_on_additional_arguments,
    messaging::*,
    muses,
    todos::{self, Todo},
    utils::*,
    CommandError::*,
    EventData,
    ThreadTrackerBot,
};

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

/// Get an iterator for the entries from the threads table for the given user.
///
/// ### Arguments
///
/// - `database` - the database to retrieve entries from
/// - `user` the user to get thread entries for
/// - `category` the category to filter results by
pub(crate) async fn enumerate(
    database: &Database,
    user: &GuildUser,
    category: Option<&str>,
) -> anyhow::Result<impl Iterator<Item = TrackedThread>> {
    Ok(db::list_threads(database, user.guild_id.0, user.user_id.0, category)
        .await?
        .into_iter()
        .map(|t| t.into()))
}

/// Adds an entry to the threads table
///
/// ### Arguments
///
/// - `args` - the command arguments
/// - `event_data` - the event data
/// - `bot` - the bot instance
pub(crate) async fn add(
    args: Vec<&str>,
    event_data: &EventData,
    bot: &ThreadTrackerBot,
) -> Result<(), anyhow::Error> {
    let mut args = args.into_iter().peekable();
    let (database, message_cache) = (&bot.database, &bot.message_cache);

    let first_arg = args.peek().unwrap_or(&"");
    let category = if !is_channel_reference(first_arg) { args.next() } else { None };

    if args.peek().is_none() {
        let example_url = format!(
            "https://discord.com/channels/{guild_id}/{channel_id}",
            guild_id = event_data.guild_id,
            channel_id = event_data.channel_id
        );
        return Err(MissingArguments(format!(
            "Please provide a `#thread-link` or URL, such as: `tt!track {example_url}` or `tt!track #thread-name`, optionally alongside a category name: `tt!track category {example_url}`"
        )).into());
    }

    let mut threads_added = MessageBuilder::new();
    let mut errors = MessageBuilder::new();

    for thread_id in args {
        if let Some(channel_id) = parse_channel_id(thread_id) {
            match channel_id.to_channel(event_data.http()).await {
                Ok(channel) => {
                    info!("Adding tracked thread {} for user {}", channel_id, event_data.user_id);
                    cache_last_channel_message(
                        channel.guild().as_ref(),
                        event_data.http(),
                        message_cache,
                    )
                    .await;

                    match db::add_thread(
                        database,
                        event_data.guild_id.0,
                        channel_id.0,
                        event_data.user_id.0,
                        category,
                    )
                    .await
                    {
                        Ok(true) => threads_added.push("• ").mention(&channel_id).push_line(""),
                        Ok(false) => threads_added
                            .push("• Skipped ")
                            .mention(&channel_id)
                            .push_line(" as it is already being tracked"),
                        Err(e) => errors
                            .push("• Failed to register thread ")
                            .mention(&channel_id)
                            .push_line_safe(format!(": {}", e)),
                    }
                },
                Err(e) => errors
                    .push("• Cannot access channel ")
                    .mention(&channel_id)
                    .push_line_safe(format!(": {}", e)),
            };
        }
        else {
            errors.push_line(format!("• Could not parse channel ID: `{}`", thread_id));
        }
    }

    let reply_context = event_data.reply_context();
    if !errors.0.is_empty() {
        error!("Errors handling thread registration:\n{}", errors);
        reply_context.send_error_embed("Error adding tracked threads", errors, message_cache).await;
    }

    if !threads_added.0.is_empty() {
        let title = match category {
            Some(name) => format!("Tracked threads added to `{}`", name),
            None => "Tracked threads added".to_owned(),
        };

        reply_context.send_success_embed(title, threads_added, message_cache).await;
    }

    Ok(())
}

/// Change the category of an existing entry in the threads table
///
/// ### Arguments
///
/// - `args` - the command arguments
/// - `event_data` - the event data
/// - `bot` - the bot instance
pub(crate) async fn set_category(
    args: Vec<&str>,
    event_data: &EventData,
    bot: &ThreadTrackerBot,
) -> Result<(), anyhow::Error> {
    let mut args = args.into_iter().peekable();
    let (database, message_cache) = (&bot.database, &bot.message_cache);

    let category = match args.next() {
        Some("unset" | "none") => None,
        Some(cat) => Some(cat),
        None => return Err(MissingArguments(format!("Please provide a category name and a thread or channel URL, such as: `tt!cat category {}`", event_data.channel_id.mention())).into()),
    };

    let mut threads_updated = MessageBuilder::new();
    let mut errors = MessageBuilder::new();

    for thread_id in args {
        if let Some(channel_id) = parse_channel_id(thread_id) {
            match channel_id.to_channel(event_data.http()).await {
                Ok(_) => match db::update_thread_category(
                    database,
                    event_data.guild_id.0,
                    channel_id.0,
                    event_data.user_id.0,
                    category,
                )
                .await
                {
                    Ok(true) => threads_updated.push("• ").mention(&channel_id).push_line(""),
                    Ok(false) => errors
                        .push("• ")
                        .mention(&channel_id)
                        .push_line(" is not currently being tracked"),
                    Err(e) => errors
                        .push("• Failed to update thread category for ")
                        .mention(&channel_id)
                        .push_line_safe(format!(": {}", e)),
                },
                Err(e) => errors
                    .push("• Cannot access channel ")
                    .mention(&channel_id)
                    .push_line(format!(": {}", e)),
            };
        }
        else {
            errors.push_line(format!("• Could not parse channel ID: {}", thread_id));
        }
    }

    let reply_context = event_data.reply_context();
    if !errors.0.is_empty() {
        error!("Errors updating thread categories:\n{}", errors);
        reply_context
            .send_error_embed("Error updating thread category", errors, message_cache)
            .await;
    }

    if !threads_updated.0.is_empty() {
        let title = match category {
            Some(name) => format!("Tracked threads' category set to `{}`", name),
            None => String::from("Tracked threads' categories removed"),
        };

        reply_context.send_success_embed(title, threads_updated, message_cache).await;
    }

    Ok(())
}

/// Remove an entry for the threads table.
///
/// ### Arguments
///
/// - `args` - the command arguments
/// - `event_data` - the event data
/// - `bot` - the bot instance
pub(crate) async fn remove(
    args: Vec<&str>,
    event_data: &EventData,
    bot: &ThreadTrackerBot,
) -> Result<(), anyhow::Error> {
    let mut args = args.into_iter().peekable();
    let database = &bot.database;
    let message_cache = &bot.message_cache;

    if args.peek().is_none() {
        return Err(MissingArguments(format!(
            "Please provide a thread or channel URL, for example: `tt!remove {:}` -- or use `tt!remove all` to untrack all threads.",
            event_data.channel_id.mention()
        )).into());
    }

    let reply_context = event_data.reply_context();
    if let Some(&"all") = args.peek() {
        db::remove_all_threads(database, event_data.guild_id.0, event_data.user_id.0, None).await?;
        reply_context
            .send_success_embed(
                "Tracked threads removed",
                &format!(
                    "All registered threads for user {:} removed.",
                    event_data.user_id.mention()
                ),
                message_cache,
            )
            .await;

        return Ok(());
    }

    let mut threads_removed = MessageBuilder::new();
    let mut errors = MessageBuilder::new();

    for thread_or_category in args {
        if let Some(channel_id) = parse_channel_id(thread_or_category) {
            let result = db::remove_thread(
                database,
                event_data.guild_id.0,
                channel_id.0,
                event_data.user_id.0,
            )
            .await;

            match result {
                Ok(0) => errors.push_line(format!(
                    "• {} is not currently being tracked",
                    channel_id.mention()
                )),
                Ok(_) => threads_removed.push_line(format!("• {:}", channel_id.mention())),
                Err(e) => errors.push_line(format!(
                    "• Failed to unregister thread {}: {}",
                    channel_id.mention(),
                    e
                )),
            };
        }
        else if !is_channel_reference(thread_or_category) {
            match db::remove_all_threads(
                database,
                event_data.guild_id.0,
                event_data.user_id.0,
                Some(thread_or_category),
            )
            .await
            {
                Ok(0) => errors.push_line(format!(
                    "• No threads in category {} to remove",
                    thread_or_category
                )),
                Ok(count) => threads_removed.push_line(format!(
                    "• All {} threads in category `{}` removed",
                    count, thread_or_category
                )),
                Err(e) => errors.push_line(format!(
                    "• Unable to remove threads in category `{}`: {}",
                    thread_or_category, e
                )),
            };
        }
        else {
            errors.push_line(format!("• Could not parse channel ID: {}", thread_or_category));
        }
    }

    if !errors.0.is_empty() {
        error!("Errors handling thread removal:\n{}", errors);
        reply_context
            .send_error_embed("Error removing tracked threads", errors, message_cache)
            .await;
    }

    if !threads_removed.0.is_empty() {
        reply_context
            .send_success_embed("Tracked threads removed", threads_removed, message_cache)
            .await;
    }

    Ok(())
}

/// Send the list of threads and todos with the default title.
///
/// ### Arguments
///
/// - `args` - the command arguments
/// - `event_data` - the event data
/// - `bot` - the bot instance
pub(crate) async fn send_list(
    args: Vec<&str>,
    event_data: &EventData,
    bot: &ThreadTrackerBot,
) -> Result<Arc<Message>, anyhow::Error> {
    send_list_with_title(args, "Currently tracked threads", event_data, bot).await
}

/// Send the list of threads and todos with a custom title.
///
/// ### Arguments
///
/// - `args` - the command arguments
/// - `event_data` - the event data
/// - `bot` - the bot instance
pub(crate) async fn send_list_with_title(
    args: Vec<&str>,
    title: impl ToString,
    event_data: &EventData,
    bot: &ThreadTrackerBot,
) -> Result<Arc<Message>, anyhow::Error> {
    let mut args = args.into_iter().peekable();
    let user = event_data.user();

    let mut threads: Vec<TrackedThread> = Vec::new();
    let mut todos: Vec<Todo> = Vec::new();

    if args.peek().is_some() {
        for category in args {
            threads.extend(enumerate(&bot.database, &user, Some(category)).await?);
            todos.extend(todos::enumerate(&bot.database, &user, Some(category)).await?);
        }
    }
    else {
        threads.extend(enumerate(&bot.database, &user, None).await?);
        todos.extend(todos::enumerate(&bot.database, &user, None).await?);
    }

    let muses = muses::list(&bot.database, &user).await?;
    let message =
        get_formatted_list(threads, todos, muses, &user, &event_data.context, &bot.message_cache)
            .await?;

    match event_data.reply_context().send_message_embed(title, message).await {
        Ok(msg) => Ok(bot.message_cache.store((msg.id, msg.channel_id).into(), msg).await),
        Err(e) => Err(e.into()),
    }
}

/// Select and send a random thread to the user that is awaiting their reply.
///
/// ### Arguments
///
/// - `args` - the command arguments
/// - `event_data` - the event data
/// - `bot` - the bot instance
pub(crate) async fn send_random_thread(
    mut args: Vec<&str>,
    event_data: &EventData,
    bot: &ThreadTrackerBot,
) -> anyhow::Result<()> {
    let mut message = MessageBuilder::new();
    let reply_context = event_data.reply_context();

    let category = args.pop();
    if let Err(e) = error_on_additional_arguments(args) {
        reply_context.send_error_embed("Too many arguments", e, &bot.message_cache).await;
    }

    match get_random_thread(category, event_data, bot).await? {
        None => {
            message.push("Congrats! You don't seem to have any threads that are waiting on your reply! :tada:");
            handle_send_result(
                reply_context.send_message_embed("No waiting threads", message),
                &bot.message_cache,
            )
            .await;
        },
        Some((last_author, thread)) => {
            message.push("Titi has chosen... this thread");

            if let Some(category) = &thread.category {
                message
                    .push(" from your ")
                    .push(Bold + Underline + category)
                    .push_line(" threads!");
            }
            else {
                message.push_line("!");
            }

            message.push_line("");
            message
                .push_quote(get_thread_link(&thread, None, event_data.http()).await)
                .push(" — ")
                .push_line(Bold + last_author);

            handle_send_result(
                reply_context.send_message_embed("Random thread", message),
                &bot.message_cache,
            )
            .await;
        },
    };

    Ok(())
}

/// Get a random thread for the current user that is awaiting a reply.
///
/// ### Arguments
///
/// - `category` - constrain the selection to the given category
/// - `event_data` - the event data
/// - `bot` - the bot instance
pub(crate) async fn get_random_thread(
    category: Option<&str>,
    event_data: &EventData,
    bot: &ThreadTrackerBot,
) -> anyhow::Result<Option<(String, TrackedThread)>> {
    let user = event_data.user();
    let muses = muses::list(&bot.database, &user).await?;
    let mut pending_threads = Vec::new();

    for thread in enumerate(&bot.database, &user, category).await? {
        let last_message_author =
            get_last_responder(&thread, &event_data.context, &bot.message_cache).await;
        match last_message_author {
            Some(user) => {
                let last_author_name =
                    get_nick_or_name(&user, event_data.guild_id, event_data.http()).await;
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

/// Build a formatted thread and todo list message.
///
/// ### Arguments
///
/// - `threads` - the list of threads
/// - `todos` - the list of todos
/// - `muses` - the target user's muses
/// - `user` - the target user
/// - `context` - the event context
/// - `message_cache` - the message cache
pub(crate) async fn get_formatted_list(
    threads: Vec<TrackedThread>,
    todos: Vec<Todo>,
    muses: Vec<String>,
    user: &GuildUser,
    context: &Context,
    message_cache: &MessageCache,
) -> Result<String, SerenityError> {
    let threads = categorise(threads);
    let todos = todos::categorise(todos);

    let mut guild_threads: HashMap<ChannelId, String> = HashMap::new();
    for channel in user.guild_id.get_active_threads(context.http()).await?.threads.into_iter() {
        cache_last_channel_message(Some(&channel), context.http(), message_cache).await;
        guild_threads.insert(channel.id, channel.name);
    }

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
            message.push_line(Bold + Underline + n).push_line("");
        }

        if let Some(threads) = threads.get(name) {
            for thread in threads {
                push_thread_line(
                    &mut message,
                    thread,
                    &guild_threads,
                    context,
                    message_cache,
                    user.user_id,
                    &muses,
                )
                .await;
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
        if !todos.is_empty() {
            message.push_line(Bold + Underline + "To Do").push_line("");

            for todo in todos {
                todos::push_todo_line(&mut message, todo);
            }
        }
    }

    if message.0.is_empty() {
        message.push_line("No threads are currently being tracked.");
    }

    Ok(message.to_string())
}

/// Partition the given threads by their categories.
fn categorise(threads: Vec<TrackedThread>) -> BTreeMap<Option<String>, Vec<TrackedThread>> {
    partition_into_map(threads, |t| t.category.clone())
}

/// Get the last user that responded to the thread, if any.
async fn get_last_responder(
    thread: &TrackedThread,
    context: &Context,
    message_cache: &MessageCache,
) -> Option<User> {
    if let Ok(Channel::Guild(channel)) = context.http().get_channel(thread.channel_id.into()).await {
        if let Some(last_message_id) = channel.last_message_id {
            let channel_message = (last_message_id, channel.id).into();
            message_cache
                .get_or_else(&channel_message, || channel_message.fetch(context))
                .await
                .ok()
                .map(|m| m.author.clone())
        }
        else {
            None
        }
    }
    else {
        None
    }
}

/// Get the user's nickname in the given guild, or their username.
async fn get_nick_or_name(user: &User, guild_id: GuildId, cache_http: impl CacheHttp) -> String {
    if user.bot {
        user.name.clone()
    }
    else {
        user.nick_in(cache_http, guild_id).await.unwrap_or(user.name.clone())
    }
}

/// Append a thread list entry to the message, followed by a newline.
async fn push_thread_line<'a>(
    message: &'a mut MessageBuilder,
    thread: &TrackedThread,
    guild_threads: &HashMap<ChannelId, String>,
    context: &Context,
    message_cache: &MessageCache,
    user_id: UserId,
    muses: &[String],
) -> &'a mut MessageBuilder {
    let last_message_author = get_last_responder(thread, context, message_cache).await;

    let link =
        get_thread_link(thread, guild_threads.get(&thread.channel_id).cloned(), context).await;
    // Thread entries in blockquotes
    message.push_quote("• ").push(link).push(" — ");

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

/// Build a thread link, either as a named link or a simple thread mention if the name isn't provided and can't be looked up.
async fn get_thread_link(
    thread: &TrackedThread,
    name: Option<String>,
    cache_http: impl CacheHttp,
) -> MessageBuilder {
    let mut link = MessageBuilder::new();
    let channel_name = match name {
        Some(n) => Some(n),
        None => get_thread_name(thread, cache_http).await,
    };

    match channel_name {
        Some(n) => {
            let name = trim_string(&n, THREAD_NAME_LENGTH);
            link.push_named_link(
                Bold + format!("#{}", name),
                format!("https://discord.com/channels/{}/{}", thread.guild_id, thread.channel_id),
            )
        },
        None => link.push(thread.channel_id.mention()),
    };

    link
}

/// Trim the given string to the maximum length, and append ellipsis if the string was trimmed.
fn trim_string(name: &str, max_length: usize) -> String {
    if name.chars().count() > max_length {
        let (cutoff, _) = name.char_indices().nth(max_length - 1).unwrap();
        format!("{}…", &name[0..cutoff].trim())
    }
    else {
        name.to_owned()
    }
}

/// Attempt to get the thread name from the Discord API
async fn get_thread_name(thread: &TrackedThread, cache_http: impl CacheHttp) -> Option<String> {
    let name = if let Some(cache) = cache_http.cache() {
        thread.channel_id.name(cache).await
    }
    else {
        None
    };

    if let Some(n) = name {
        Some(n)
    }
    else {
        thread.channel_id.to_channel(cache_http).await.map_or(None, |c| c.guild()).map(|gc| gc.name)
    }
}

/// Retrieve the most recent message in the given channel and store it in the cache.
async fn cache_last_channel_message(
    channel: Option<&GuildChannel>,
    http: impl AsRef<Http>,
    message_cache: &MessageCache,
) {
    if let Some(channel) = channel {
        if let Some(last_message_id) = channel.last_message_id {
            let channel_message = (last_message_id, channel.id).into();

            if !message_cache.contains_key(&channel_message).await {
                if let Ok(last_message) = channel_message.fetch(http).await {
                    message_cache.store(channel_message, last_message).await;
                }
            }
        }
    }
}

/// Parse the channel ID from either a channel mention or a full discord URL to the channel.
fn parse_channel_id(url_or_mention: &str) -> Option<ChannelId> {
    if is_channel_mention(url_or_mention) {
        url_or_mention.parse().ok()
    }
    else if is_discord_link(url_or_mention) {
        if let Some(Ok(target_channel_id)) = url_or_mention.split('/').last().map(|x| x.parse()) {
            Some(ChannelId(target_channel_id))
        }
        else {
            None
        }
    }
    else {
        None
    }
}

/// Returns true if the string is either a full channel URL or a channel mention (`<#8782839489279>`).
fn is_channel_reference(s: &str) -> bool {
    is_channel_mention(s) || is_discord_link(s)
}

/// Returns true if the string is a discord channel URL.
fn is_discord_link(s: &str) -> bool {
    s.starts_with("https://") && s.matches("discord.com/channels/").any(|_| true)
}

/// Returns true if the string is (probably) a channel mention.
fn is_channel_mention(s: &str) -> bool {
    s.starts_with("<#") && s.ends_with('>')
}
