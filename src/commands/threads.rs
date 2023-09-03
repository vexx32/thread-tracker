use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap},
};

use rand::Rng;
use serenity::{
    builder::CreateApplicationCommands,
    http::{CacheHttp, Http},
    model::prelude::{
        command::{CommandOptionType, CommandType},
        interaction::application_command::ApplicationCommandInteraction,
        *,
    },
    prelude::*,
    utils::{ContentModifier::*, EmbedMessageBuilding, MessageBuilder},
};
use tracing::{error, info};

use crate::{
    cache::MessageCache,
    commands::{
        muses,
        todos::{self, Todo},
    },
    consts::{THREAD_NAME_LENGTH, TRACKABLE_CHANNEL_TYPES},
    db::{self},
    messaging::*,
    utils::*,
    Database,
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

pub(crate) async fn enumerate_tracked_channel_ids(
    database: &Database,
) -> sqlx::Result<impl Iterator<Item = ChannelId>> {
    Ok(db::get_global_tracked_thread_ids(database)
        .await?
        .into_iter()
        .map(|t| ChannelId(t.channel_id as u64)))
}

pub fn register_commands(
    commands: &mut CreateApplicationCommands,
) -> &mut CreateApplicationCommands {
    commands
        .create_application_command(|command| command
            .name("tt!track")
            .description("Add a new tracked thread")
            .kind(CommandType::ChatInput)
            .create_option(|option| option
                .name("thread")
                .description("The thread to track")
                .kind(CommandOptionType::Channel)
                .channel_types(&TRACKABLE_CHANNEL_TYPES)
                .required(true))
            .create_option(|option| option
                .name("category")
                .description("Set the category for the new thread (optional)")
                .kind(CommandOptionType::String)))
        .create_application_command(|command| command
            .name("tt!category")
            .description("Update or remove the category for an already-tracked thread")
            .kind(CommandType::ChatInput)
            .create_option(|option| option
                .name("thread")
                .description("The thread to update")
                .kind(CommandOptionType::Channel)
                .channel_types(&TRACKABLE_CHANNEL_TYPES)
                .required(true))
            .create_option(|option| option
                .name("category")
                .description("Set the category for the thread (optional, omit to un-set the category)")
                .kind(CommandOptionType::String)))
        .create_application_command(|command| command
            .name("tt!untrack")
            .description("Remove a currently tracked thread")
            .kind(CommandType::ChatInput)
            .create_option(|option| option
                .name("thread")
                .description("The thread to remove")
                .kind(CommandOptionType::Channel)
                .channel_types(&TRACKABLE_CHANNEL_TYPES))
            .create_option(|option| option
                .name("category")
                .description("Removes all threads in the category")
                .kind(CommandOptionType::SubCommand)
                .create_sub_option(|option| option
                    .name("category")
                    .description("The category to untrack all threads from; specify 'all' to untrack all threads")
                    .required(true))))
        .create_application_command(|command| command
            .name("tt!replies")
            .description("List currently tracked threads")
            .kind(CommandType::ChatInput)
            .create_option(|option| option
                .name("category")
                .description("The specific category to list threads from")
                .kind(CommandOptionType::String)))
        .create_application_command(|command| command
            .name("tt!random")
            .description("Get a random thread that is waiting for your reply")
            .kind(CommandType::ChatInput)
            .create_option(|option| option
                .name("category")
                .description("The specific category to find a thread from")
                .kind(CommandOptionType::String)))
}

/// Adds an entry to the threads table
///
/// ### Arguments
///
/// - `command` - the slash command interaction data
/// - `bot` - the bot instance
/// - `context` - the interaction context
pub(crate) async fn add(
    command: &ApplicationCommandInteraction,
    bot: &ThreadTrackerBot,
    context: &Context,
) -> Vec<InteractionResponse> {
    const ERROR_TITLE: &str = "Error adding tracked thread";

    let guild_id = match command.guild_id {
        Some(id) => id,
        None => {
            return InteractionResponse::error(
                ERROR_TITLE,
                "Unable to track threads outside of a server",
            )
        },
    };

    let user_id = command.user.id;

    let args = &command.data.options;
    let (database, message_cache) = (&bot.database, &bot.message_cache);

    let category = find_string_option(args, "category");

    let mut threads_added = MessageBuilder::new();
    let mut errors = MessageBuilder::new();

    if let Some(thread) = find_channel_option(args, "thread") {
        match thread.id.to_channel(context).await {
            Ok(channel) => {
                info!(
                    "Adding tracked thread {} for user `{}` ({})",
                    thread.id, command.user.name, command.user.id
                );
                cache_last_channel_message(channel.guild().as_ref(), context, message_cache)
                    .await;

                let result = db::add_thread(
                    database,
                    guild_id.0,
                    thread.id.0,
                    user_id.0,
                    category,
                )
                .await;
                match result {
                    Ok(true) => {
                        bot.add_tracked_thread(thread.id).await;
                        threads_added.push("- ").mention(&thread.id).push_line("")
                    },
                    Ok(false) => threads_added
                        .push("- Skipped ")
                        .mention(&thread.id)
                        .push_line(" as it is already being tracked"),
                    Err(e) => errors
                        .push("- Failed to register thread ")
                        .mention(&thread.id)
                        .push_line_safe(format!(": {}", e)),
                }
            },
            Err(e) => errors
                .push("- Cannot access channel ")
                .mention(&thread.id)
                .push_line_safe(format!(": {}", e)),
        };
    }
    else {
        errors.push_line("- No thread or channel id was provided.");
    }

    let mut messages = Vec::new();
    if !errors.0.is_empty() {
        messages.extend(InteractionResponse::error(ERROR_TITLE, errors.build()).into_iter());
        error!("Errors handling thread registration:\n{}", errors);
    }

    if !threads_added.0.is_empty() {
        let title = match category {
            Some(name) => format!("Tracked threads added to `{}`", name),
            None => "Tracked threads added".to_owned(),
        };

        messages.extend(InteractionResponse::reply(title, threads_added.build()).into_iter());
    }

    messages
}

/// Change the category of an existing entry in the threads table
///
/// ### Arguments
///
/// - `command` - the slash command interaction data
/// - `database` - the database to update
/// - `context` - the interaction context
pub(crate) async fn set_category(
    command: &ApplicationCommandInteraction,
    database: &Database,
    context: &Context,
) -> Vec<InteractionResponse> {
    const ERROR_TITLE: &str = "Error updating tracked thread category";
    let guild_id = match command.guild_id {
        Some(id) => id,
        None => {
            return InteractionResponse::error(
                ERROR_TITLE,
                "Unable to managed tracked threads outside of a server",
            )
        },
    };

    let user_id = command.user.id;

    let args = &command.data.options;
    let category = find_string_option(args, "category");

    let mut threads_updated = MessageBuilder::new();
    let mut errors = MessageBuilder::new();

    if let Some(thread) = find_channel_option(args, "thread") {
        info!(
            "updating category for thread `{}` to `{}`",
            thread.id,
            category.unwrap_or("none")
        );
        match thread.id.to_channel(context).await {
            Ok(_) => match db::update_thread_category(
                database,
                guild_id.0,
                thread.id.0,
                user_id.0,
                category,
            )
            .await
            {
                Ok(true) => threads_updated.push("- ").mention(&thread.id).push_line(""),
                Ok(false) => errors
                    .push("- ")
                    .mention(&thread.id)
                    .push_line(" is not currently being tracked"),
                Err(e) => errors
                    .push("- Failed to update thread category for ")
                    .mention(&thread.id)
                    .push_line_safe(format!(": {}", e)),
            },
            Err(e) => errors
                .push("- Cannot access channel ")
                .mention(&thread.id)
                .push_line(format!(": {}", e)),
        }
    }
    else {
        errors.push_line("- No thread or channel id was provided.")
    };

    let mut messages = Vec::new();
    if !errors.0.is_empty() {
        error!("Errors updating thread categories:\n{}", errors);
        messages.extend(InteractionResponse::error(ERROR_TITLE, errors.build()).into_iter());
    }

    if !threads_updated.0.is_empty() {
        let title = match category {
            Some(name) => format!("Tracked threads' category set to `{}`", name),
            None => String::from("Tracked threads' categories removed"),
        };

        messages.extend(InteractionResponse::reply(title, threads_updated.build()).into_iter());
    }

    messages
}

/// Remove an entry for the threads table.
///
/// ### Arguments
///
/// - `command` - the slash command interaction data
/// - `bot` - the bot instance
/// - `context` - the interaction context
pub(crate) async fn remove(
    command: &ApplicationCommandInteraction,
    bot: &ThreadTrackerBot,
) -> Vec<InteractionResponse> {
    const ERROR_TITLE: &str = "Error adding tracked thread";

    let guild_id = match command.guild_id {
        Some(id) => id,
        None => {
            return InteractionResponse::error(
                ERROR_TITLE,
                "Unable to manage tracked threads outside of a server",
            )
        },
    };

    let args = &command.data.options;
    let database = &bot.database;

    let mut threads_removed = MessageBuilder::new();
    let mut errors = MessageBuilder::new();

    if let Some(thread) = find_channel_option(args, "thread") {
        info!(
            "removing tracked thread `{}` for {} ({})",
            thread.id, command.user.name, command.user.id
        );
        let result = db::remove_thread(database, guild_id.0, thread.id.0, command.user.id.0).await;

        match result {
            Ok(0) => errors
                .push_line(format!("- {} is not currently being tracked", thread.id.mention())),
            Ok(_) => {
                bot.remove_tracked_thread(thread.id).await.ok();
                threads_removed.push_line(format!("- {:}", thread.id.mention()))
            },
            Err(e) => errors.push_line(format!(
                "- Failed to unregister thread {}: {}",
                thread.id.mention(),
                e
            )),
        };
    }

    let target_category = args
        .iter()
        .find(|opt| opt.name == "category")
        .and_then(|opt| find_string_option(&opt.options, "category"));

    if let Some(category) = target_category {
        if category == "all" {
            info!("removing all tracked threads for {} ({})", command.user.name, command.user.id);
            match db::remove_all_threads(database, guild_id.0, command.user.id.0, None).await {
                Ok(0) => {
                    threads_removed.push_line("No threads are currently being tracked.")
                },
                Ok(count) => threads_removed
                    .push_line(format!("All {} threads removed from tracking.", count)),
                Err(e) => {
                    error!(
                        "Error untracking all threads for user {} ({}): {}",
                        command.user.name, command.user.id, e
                    );
                    errors.push_line(format!("Error untracking all threads: {}", e))
                },
            };

            if let Err(e) = bot.update_tracked_threads().await {
                error!("Error updating in-memory list of tracked threads: {}", e)
            };
        }
    }

    let mut result = Vec::new();

    if !errors.0.is_empty() {
        error!("Errors handling thread removal:\n{}", errors);
        result.extend(InteractionResponse::error(ERROR_TITLE, errors.build()).into_iter());
    }

    if !threads_removed.0.is_empty() {
        result.extend(
            InteractionResponse::reply("Tracked threads removed", threads_removed.build())
                .into_iter(),
        );
    }

    result
}

/// Send the list of threads and todos with the default title.
///
/// ### Arguments
///
/// - `command` - the slash command interaction data
/// - `bot` - the bot instance
/// - `context` - the interaction context
pub(crate) async fn send_list(
    command: &ApplicationCommandInteraction,
    bot: &ThreadTrackerBot,
    context: &Context,
) -> Vec<InteractionResponse> {
    const ERROR_TITLE: &str = "Error fetching tracked threads";

    let guild_id = match command.guild_id {
        Some(id) => id,
        None => {
            return InteractionResponse::error(
                ERROR_TITLE,
                "Unable to manage tracked threads outside of a server",
            )
        },
    };

    let args = &command.data.options;

    let category = find_string_option(args, "category");

    get_list_with_title(
        "Currently tracked threads".into(),
        &command.user,
        guild_id,
        category,
        bot,
        context,
    )
    .await
}

/// Send the list of threads and todos with a custom title.
///
/// ### Arguments
///
/// - `title` - the title to use for the thread list
/// - `user` - the user which requested the thread list
/// - `guild_id` - the guild ID the threads are tracked in
/// - `category` - the category to filter the threads/todos by
/// - `bot` - the bot instance
/// - `context` - the Serenity context
pub(crate) async fn get_list_with_title(
    title: Cow<'static, str>,
    user: &User,
    guild_id: GuildId,
    category: Option<&str>,
    bot: &ThreadTrackerBot,
    context: &Context,
) -> Vec<InteractionResponse> {
    const ERROR_TITLE: &str = "Error collating tracked threads";
    info!("Getting tracked threads and todo list for {} ({})", user.name, user.id);

    let guild_user = GuildUser { user_id: user.id, guild_id };

    let mut threads: Vec<TrackedThread> = Vec::new();
    let mut todos: Vec<Todo> = Vec::new();

    let mut errors = MessageBuilder::new();

    match enumerate(&bot.database, &guild_user, category).await {
        Ok(t) => threads.extend(t),
        Err(e) => {
            error!("Error listing tracked threads for {}: {}", user.name, e);
            errors.push("- ").push_line(e);
        },
    }

    match todos::enumerate(&bot.database, &guild_user, category).await {
        Ok(t) => todos.extend(t),
        Err(e) => {
            error!("Error listing todos for {}: {}", user.name, e);
            errors.push("- ").push_line(e);
        },
    }

    let muses = match muses::get_list(&bot.database, guild_user.user_id, guild_user.guild_id).await {
        Ok(m) => m,
        Err(e) => {
            error!("Error finding muse list for {}: {}", user.name, e);
            errors.push("- ").push_line(e);
            Vec::new()
        },
    };

    let message =
        match get_formatted_list(threads, todos, muses, &guild_user, context, &bot.message_cache)
            .await
        {
            Ok(m) => m,
            Err(e) => {
                error!("Error collating tracked threads for {}: {}", user.name, e);
                errors.push("- ").push_line(e);
                String::new()
            },
        };

    let mut result = Vec::new();

    if !errors.0.is_empty() {
        error!("Errors getting tracked threads list for {}: {}", user.name, errors);
        result.extend(InteractionResponse::error(ERROR_TITLE, errors.build()).into_iter());
    }

    if !message.is_empty() {
        result.extend(InteractionResponse::reply(title, message).into_iter());
    }

    result
}

/// Select and send a random thread to the user that is awaiting their reply.
///
/// ### Arguments
///
/// - `command` - the slash command interaction data
/// - `bot` - the bot instance
/// - `context` - the interaction context
pub(crate) async fn send_random_thread(
    command: &ApplicationCommandInteraction,
    bot: &ThreadTrackerBot,
    context: &Context,
) -> Vec<InteractionResponse> {
    const ERROR_TITLE: &str = "Error fetching tracked threads";

    let guild_id = match command.guild_id {
        Some(id) => id,
        None => {
            return InteractionResponse::error(
                ERROR_TITLE,
                "Unable to manage tracked threads outside of a server",
            )
        },
    };

    let mut message = MessageBuilder::new();
    let mut errors = MessageBuilder::new();
    let category = find_string_option(&command.data.options, "category");

    info!("sending a random thread for {} ({})", command.user.name, command.user.id);

    match get_random_thread(category, &command.user, guild_id, context, bot).await {
        Ok(None) => {
            message.push("Congrats! You don't seem to have any threads that are waiting on your reply! :tada:");
        },
        Ok(Some((last_author, thread))) => {
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
                .push_quote(get_thread_link(&thread, None, context).await)
                .push(" — ")
                .push_line(Bold + last_author);
        },
        Err(e) => {
            errors.push("- ").push_line(e);
        },
    };

    let mut result = Vec::new();

    if !errors.0.is_empty() {
        error!("Errors encountered getting a random thread for {}: {}", command.user.name, errors);
        result.extend(InteractionResponse::error(ERROR_TITLE, errors.build()).into_iter());
    }

    if !message.0.is_empty() {
        result.extend(InteractionResponse::reply("Random thread", message.build()).into_iter());
    }

    result
}

/// Get a random thread for the current user that is awaiting a reply.
///
/// ### Arguments
///
/// - `category` - constrain the selection to the given category
/// - `user` - the user to find a random thread for
/// - `guild_id` - the guild to find a random tracked thread in
/// - `context` - the Serenity context
/// - `bot` - the bot instance
pub(crate) async fn get_random_thread(
    category: Option<&str>,
    user: &User,
    guild_id: GuildId,
    context: &Context,
    bot: &ThreadTrackerBot,
) -> anyhow::Result<Option<(String, TrackedThread)>> {
    let guild_user = GuildUser { user_id: user.id, guild_id };
    let muses = muses::get_list(&bot.database, guild_user.user_id, guild_user.guild_id).await?;
    let mut pending_threads = Vec::new();

    for thread in enumerate(&bot.database, &guild_user, category).await? {
        let last_message_author = get_last_responder(&thread, context, &bot.message_cache).await;
        match last_message_author {
            Some(author) => {
                let last_author_name = get_nick_or_name(&author, guild_id, context).await;
                if author.id != user.id && !muses.contains(&last_author_name) {
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
    match context.http().get_channel(thread.channel_id.into()).await {
        Ok(Channel::Guild(channel)) => {
            let last_message = if let Some(last_message_id) = channel.last_message_id {
                let channel_message = (last_message_id, channel.id).into();
                message_cache
                    .get_or_else(&channel_message, || channel_message.fetch(context))
                    .await
                    .ok()
            }
            else {
                None
            };

            // This fallback is necessary as Discord may not report a correct or available message as the last_message_id.
            // Messages can be deleted or otherwise unavailable, so this fallback should get the most recent
            // *available* message in the channel.
            match last_message {
                Some(m) => Some(m.author.clone()),
                None => get_last_channel_message(channel, context).await.map(|m| m.author),
            }
        },
        _ => None,
    }
}

async fn get_last_channel_message(channel: GuildChannel, context: &Context) -> Option<Message> {
    channel.messages(context, |messages| messages.limit(1)).await.ok().and_then(|mut m| m.pop())
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
    message.push("- ").push(link).push(" — ");

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
        None => message.push_line(Bold + "No replies yet"),
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
        let trimmed = substring(name, max_length);
        format!("{}…", trimmed.trim())
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
        get_channel_name(thread.channel_id, cache_http).await
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
