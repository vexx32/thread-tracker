use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
        Mutex,
    },
    time::Duration,
};

use background_tasks::Task;
use cache::MessageCache;
use commands::{threads, CommandError};
use db::Database;
use poise::{
    serenity_prelude::*,
    FrameworkError,
};
use serenity::model::channel::Message;
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    ConnectOptions,
    Executor,
};
use tokio::{
    sync::{mpsc::{self, Sender}, RwLock},
    time::sleep,
};
use toml::Table;
use tracing::{debug, error, info, log::LevelFilter};
use utils::message_is_command;

use crate::{
    background_tasks::{
        listen_for_background_tasks,
        run_periodic_shard_tasks,
        start_periodic_tasks,
    },
    consts::{DELETE_EMOJI, MPSC_BUFFER_SIZE, SHARD_CHECKUP_INTERVAL},
    messaging::reply_error,
};

mod background_tasks;
mod cache;
mod commands;
mod consts;
mod db;
mod messaging;
mod utils;

/// Utility error type to encapsulate any errors.
type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug)]
/// Data needed for bot commands.
struct Data {
    /// Postgres database pool
    database: Database,
    /// The total number of guilds the bot is in
    guild_count: AtomicUsize,
    /// Threadsafe memory cache for messages the bot has sent or looked up
    message_cache: MessageCache,
    /// The current list of tracked threads
    tracked_threads: Arc<RwLock<HashSet<ChannelId>>>,
}

impl Data {
    /// Create a new Data.
    fn new(database: Database) -> Self {
        Self {
            database,
            message_cache: MessageCache::new(),
            tracked_threads: Arc::new(RwLock::new(HashSet::new())),
            guild_count: AtomicUsize::new(0),
        }
    }

    /// Retrieve the current guild count.
    fn guilds(&self) -> usize {
        self.guild_count.load(Ordering::SeqCst)
    }

    /// Retrieve the full list of tracked threads from the database to populate the in-memory
    /// list of tracked threads.
    async fn update_tracked_threads(&self) -> sqlx::Result<()> {
        let mut tracked_threads = self.tracked_threads.write().await;
        tracked_threads.clear();

        threads::enumerate_tracked_channel_ids(&self.database).await?.for_each(|id| {
            tracked_threads.insert(id);
        });

        Ok(())
    }

    /// Add a newly tracked thread ID into the in-memory list of tracked threads.
    async fn add_tracked_thread(&self, channel_id: ChannelId) {
        self.tracked_threads.write().await.insert(channel_id);
    }

    /// Call this function after removing a tracked thread from the database to update the in-memory
    /// list of tracked threads. The thread will only be removed from the list if it is no longer
    /// being tracked by any users.
    async fn remove_tracked_thread(&self, channel_id: ChannelId) -> sqlx::Result<()> {
        let still_tracked = threads::enumerate_tracked_channel_ids(&self.database)
            .await?
            .any(|id| id == channel_id);

        if !still_tracked {
            self.tracked_threads.write().await.remove(&channel_id);
        }

        Ok(())
    }

    /// Check if the given channel_id is in the list of tracked threads.
    async fn tracking_thread(&self, channel_id: ChannelId) -> bool {
        self.tracked_threads.read().await.contains(&channel_id)
    }
}

/// Event handler struct, managing Serenity's events and handling dispatching them to Poise.
struct Handler {
    /// The shared command data
    data: Arc<RwLock<Data>>,
    /// The Poise framework options
    options: poise::FrameworkOptions<Data, CommandError>,
    /// The Serenity shard manager
    shard_manager: Mutex<Option<std::sync::Arc<ShardManager>>>,
    /// The bot's user id
    user_id: AtomicU64,
    /// The root sender for the background task message queue
    channel: Sender<Task>,
}

impl Handler {
    /// Create a new Handler.
    fn new(
        options: poise::FrameworkOptions<Data, CommandError>,
        database: Database,
        channel: Sender<Task>,
    ) -> Self {
        Self {
            options,
            channel,
            data: Arc::new(RwLock::new(Data::new(database))),
            shard_manager: Mutex::new(None),
            user_id: AtomicU64::new(0),
        }
    }

    /// Sets the current user ID for the bot. Typically called from the `ready` event.
    fn set_user(&self, id: UserId) {
        self.user_id.store(id.get(), Ordering::SeqCst);
    }

    /// Gets the current user ID for the bot, if it's been set.
    fn user(&self) -> Option<UserId> {
        let value = self.user_id.load(Ordering::SeqCst);

        if value == 0u64 {
            None
        }
        else {
            Some(value.into())
        }
    }

    /// Forward an event to Poise to streamline command handling.
    async fn forward_to_poise(&self, ctx: &Context, event: FullEvent) {
        // FrameworkContext contains all data that poise::Framework usually manages
        let shard_manager = (*self.shard_manager.lock().unwrap()).clone().unwrap();
        let framework_data = poise::FrameworkContext {
            bot_id: self.user().unwrap_or_default(),
            options: &self.options,
            user_data: &*self.data.read().await,
            shard_manager: &shard_manager,
        };

        poise::dispatch_event(framework_data, ctx, event).await;
    }
}

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn reaction_add(&self, context: Context, reaction: Reaction) {
        let bot_user = self.user();
        if reaction.user_id == bot_user {
            // Ignore reactions made by the bot user
            return;
        }

        debug!("Received reaction {} on message {}", reaction.emoji, reaction.message_id);

        if DELETE_EMOJI.iter().any(|&emoji| reaction.emoji.unicode_eq(emoji)) {
            let channel_message = (reaction.channel_id, reaction.message_id).into();
            let data = self.data.read().await;
            if let Ok(message) = data
                .message_cache
                .get_or_else(&channel_message, || channel_message.fetch(&context))
                .await
            {
                if Some(message.author.id) != bot_user {
                    // Ignore reactions to messages not sent by the bot.
                    return;
                }

                // Follow chained messages up to the initial bot-message
                let mut root_message: &Message = &message;
                while let Some(message) = &root_message.referenced_message {
                    if Some(message.author.id) != self.user() {
                        // Parent referenced message is not from the bot, this is a reply to a user message.
                        break;
                    }

                    root_message = message;
                }

                if let Some(referenced_message) = &root_message.referenced_message {
                    info!("Processing deletion request for message {}", message.id);
                    if Some(referenced_message.author.id) == reaction.user_id {
                        utils::delete_message(&message, &context, &data).await;
                    }
                }
                else if let Some(interaction) = &root_message.interaction {
                    info!("Processing deletion request for message {}", message.id);
                    if Some(interaction.user.id) == reaction.user_id {
                        utils::delete_message(&message, &context, &data).await;
                    }
                }
                else {
                    error!("Could not find referenced message to check requesting user ID against")
                }
            }
        }

        self.forward_to_poise(&context, FullEvent::ReactionAdd { add_reaction: reaction }).await;
    }

    async fn message(&self, context: Context, message: Message) {
        let user_id = message.author.id;
        if Some(user_id) == self.user() && cfg!(not(debug_assertions)) {
            return;
        }

        if !message_is_command(&message.content) {
            let is_tracking_thread =
                { self.data.read().await.tracking_thread(message.channel_id).await };

            if is_tracking_thread {
                let data = self.data.read().await;
                debug!("Caching new message from tracked channel {}", message.channel_id);
                data.message_cache
                    .store((message.channel_id, message.id).into(), message.clone())
                    .await;

                // Send notification task to background task runner.
                if let Err(e) = self.channel.send(Task::Notify(message.clone())).await {
                    error!(
                        "Error sending reply notifications due to internal communication error: {}",
                        e
                    );
                }
            }
        }

        self.forward_to_poise(&context, FullEvent::Message { new_message: message }).await;
    }

    async fn message_update(
        &self,
        ctx: Context,
        old_if_available: Option<Message>,
        new: Option<Message>,
        event: MessageUpdateEvent,
    ) {
        self.forward_to_poise(&ctx, FullEvent::MessageUpdate { old_if_available, new, event })
            .await;
    }

    async fn guild_create(&self, ctx: Context, guild: Guild, is_new: Option<bool>) {
        if let Some(true) = is_new {
            info!("notified that Titi was added to a new guild: `{}` ({})!", guild.name, guild.id);
            self.data.read().await.guild_count.fetch_add(1, Ordering::SeqCst);

            if cfg!(debug_assertions) {
                utils::register_guild_commands(&self.options.commands, guild.id, &ctx).await;
            }
        }

        self.forward_to_poise(&ctx, FullEvent::GuildCreate { guild, is_new }).await;
    }

    async fn guild_delete(
        &self,
        ctx: Context,
        guild_partial: UnavailableGuild,
        guild_full: Option<Guild>,
    ) {
        if !guild_partial.unavailable {
            let guild_name =
                guild_full.as_ref().map(|g| g.name.clone()).unwrap_or_else(|| format!("{}", guild_partial.id));
            info!(
                "notified that Titi has been removed from the `{}` guild ({})",
                guild_name, guild_partial.id
            );

            self.data.read().await.guild_count.fetch_sub(1, Ordering::SeqCst);
        }

        self.forward_to_poise(&ctx, FullEvent::GuildDelete { incomplete: guild_partial, full: guild_full }).await;
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        let guild_count = ready.guilds.len();

        info!("connected to Discord successfully as `{}`", ready.user.name);
        info!("currently active in {} guilds", guild_count);

        self.set_user(ready.user.id);
        let data = self.data.read().await;
        data.guild_count.store(guild_count, Ordering::Relaxed);

        let commands = poise::builtins::create_application_commands(&self.options.commands);

        // Register current commands.
        let result = Command::set_global_commands(&ctx, commands).await;

        match result {
            Ok(cmds) => info!("Successfully updated global command registration with {} commands", cmds.len()),
            Err(e) => error!("Unable to register commands globally: {}", e),
        }

        run_periodic_shard_tasks(&ctx, &self.channel);

        self.forward_to_poise(&ctx, FullEvent::Ready { data_about_bot: ready }).await;
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        self.forward_to_poise(&ctx, FullEvent::InteractionCreate { interaction }).await;
    }
}

/// Handler to be invoked on errors received from Poise.
async fn on_error(error: poise::FrameworkError<'_, Data, CommandError>) {
    // This is our custom error handler
    // They are many errors that can occur, so we only handle the ones we want to customize
    // and forward the rest to the default handler
    match &error {
        FrameworkError::Setup { error: e, .. } => panic!("Failed to start bot: {:?}", e),
        FrameworkError::Command { error: e, ctx , ..} => {
            error!("Error in command `{}`: {}", ctx.command().name, e);
            if let Err(e) = reply_error(ctx, "Error running command", &e.to_string()).await {
                error!("Could not send error response to user: {}", e);
            }
        },
        _ => {
            if let Err(e) = poise::builtins::on_error(error).await {
                error!("Error while handling error: {}", e)
            }
        },
    }
}

#[tokio::main]
/// Main bot application thread.
async fn main() -> anyhow::Result<()> {
    use anyhow::Context;

    tracing_subscriber::fmt::init();

    let configuration = include_str!("../Secrets.toml").parse::<Table>().unwrap();

    // Get the discord token set in `Secrets.toml`
    let token_entry = if cfg!(debug_assertions) { "DISCORD_TOKEN_DEV" } else { "DISCORD_TOKEN" };
    let db_entry =
        if cfg!(debug_assertions) { "CONNECTION_STRING_DEV" } else { "CONNECTION_STRING" };

    let discord_token = configuration[token_entry].as_str().unwrap();
    let connection_string = configuration[db_entry].as_str().unwrap();

    let options = connection_string
        .parse::<PgConnectOptions>()?
        .log_statements(LevelFilter::Trace)
        .log_slow_statements(LevelFilter::Warn, Duration::from_secs(5));
    let database = PgPoolOptions::new()
        .max_connections(20)
        .connect_with(options)
        .await?;

    // Run the schema migration
    database
        .execute(include_str!("../sql/schema.sql"))
        .await?;

    // FrameworkOptions contains all of poise's configuration option in one struct
    // Every option can be omitted to use its default value
    let options = poise::FrameworkOptions {
        commands: commands::list(),
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: Some("tt!".into()),
            edit_tracker: Some(Arc::new(poise::EditTracker::for_timespan(Duration::from_secs(3600)))),
            mention_as_prefix: true,
            ..Default::default()
        },
        // The global error handler for all error cases that may occur
        on_error: |error| Box::pin(on_error(error)),
        // This code is run before every command
        pre_command: |ctx| {
            Box::pin(async move {
                info!("Executing command {}...", ctx.invoked_command_name());
            })
        },
        // This code is run after a command if it was successful (returned Ok)
        post_command: |ctx| {
            Box::pin(async move {
                info!("Execution of {} completed", ctx.invoked_command_name());
            })
        },
        // Enforce command checks even for owners (enforced by default)
        // Set to true to bypass checks, which is useful for testing
        skip_checks_for_owners: false,
        ..Default::default()
    };

    // Setup the MPSC channel for sending off background tasks
    let (sender, receiver) = mpsc::channel(MPSC_BUFFER_SIZE);

    let mut handler = Handler::new(options, database, sender);

    poise::set_qualified_names(&mut handler.options.commands);

    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::GUILD_MESSAGE_REACTIONS
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::GUILDS;

    // Just log result from here, we just want a semi-consistent initial state for the tracked threads.
    info!("Retrieving currently tracked threads");
    if let Err(e) = handler.data.read().await.update_tracked_threads().await {
        error!("Error populating currently tracked threads: {}", e);
    }

    let handler = std::sync::Arc::new(handler);
    let mut client =
        Client::builder(discord_token, intents).event_handler_arc(Arc::clone(&handler)).await?;

    client.cache.set_max_messages(1);

    let manager = client.shard_manager.clone();
    tokio::spawn(async move {
        loop {
            sleep(SHARD_CHECKUP_INTERVAL).await;

            let runners = manager.runners.lock().await;

            for (id, runner) in runners.iter() {
                info!("Shard ID {} is {} with a latency of {:?}", id, runner.stage, runner.latency);
            }
        }
    });

    *handler.shard_manager.lock().unwrap() = Some(client.shard_manager.clone());

    listen_for_background_tasks(receiver, handler.data.clone(), client.http.clone());
    start_periodic_tasks(&handler.channel);

    client.start_autosharded().await.context("Error starting client")?;

    Ok(())
}
