use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::anyhow;
use cache::MessageCache;
use commands::threads;
use messaging::ReplyContext;
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    ConnectOptions,
    Executor,
};
use tokio::time::sleep;
use toml::Table;
use tracing::{debug, error, info, log::LevelFilter, warn};

mod background_tasks;
mod cache;
mod commands;
mod consts;
mod db;
mod messaging;
mod stats;
mod utils;

use db::Database;
use utils::message_is_command;

use std::{collections::HashMap, env::var, sync::Mutex};
use poise::{FrameworkError, serenity_prelude::ShardManager};

use serenity::{
    async_trait,
    model::{
        channel::Message,
        prelude::{command::Command, interaction::Interaction, *},
    },
    prelude::{Context as SerenityContext, *},
    utils::MessageBuilder,
};

use crate::{consts::DELETE_EMOJI, background_tasks::run_periodic_tasks};

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Handler, Error>;

struct Data {
    /// Postgres database pool
    database: Database,
    /// The total number of guilds the bot is in
    guild_count: AtomicUsize,
    /// Threadsafe memory cache for messages the bot has sent or looked up
    message_cache: MessageCache,
    /// The current list of tracked threads
    tracked_threads: HashSet<ChannelId>,
}

impl Data {
    fn new(database: Database) -> Self {
        Self {
            database,
            message_cache: MessageCache::new(),
            tracked_threads: HashSet::new(),
            guild_count: AtomicUsize::new(0),
        }
    }
}

// Data to be passed to all commands
struct Handler {
    /// The shared command data
    data: Arc<RwLock<Data>>,
    /// The Poise framework options
    options: poise::FrameworkOptions<Data, Error>,
    /// The Serenity shard manager
    shard_manager: Mutex<Option<std::sync::Arc<tokio::sync::Mutex<ShardManager>>>>,
    /// The bot's user id
    user_id: Option<UserId>,
}

impl Handler {
    /// Create a new bot instance.
    ///
    /// ### Arguments
    ///
    /// - `database` - database pool connection
    fn new(options: poise::FrameworkOptions<Data, Error>, database: Database) -> Self {
        Self {
            data: Arc::new(RwLock::new(Data::new(database))),
            options,
            shard_manager: Mutex::new(None),
            user_id: None,
        }
    }

    /// Get a tracked reference to the shared data
    fn data(&self) -> Arc<RwLock<Data>> {
        Arc::clone(&self.data)
    }

    /// Sets the current user ID for the bot
    ///
    /// ### Arguments
    ///
    /// - `id` - the UserId
    async fn set_user(&mut self, id: UserId) {
        self.user_id = Some(id);
    }

    /// Gets the current user ID for the bot, if it's been set
    async fn user(&self) -> Option<UserId> {
        self.user_id
    }

    /// Retrieve the full list of tracked threads from the database to populate the in-memory
    /// list of tracked threads.
    async fn update_tracked_threads(&self) -> sqlx::Result<()> {
        let mut data = self.data.write().await;

        data.tracked_threads.clear();

        threads::enumerate_tracked_channel_ids(&data.database).await?.for_each(|id| {
            data.tracked_threads.insert(id);
        });

        Ok(())
    }

    /// Add a newly tracked thread ID into the in-memory list of tracked threads.
    async fn add_tracked_thread(&self, channel_id: ChannelId) {
        let mut data = self.data.write().await;
        data.tracked_threads.insert(channel_id);
    }

    /// Call this function after removing a tracked thread from the database to update the in-memory
    /// list of tracked threads. The thread will only be removed from the list if it is no longer
    /// being tracked by any users.
    async fn remove_tracked_thread(&self, channel_id: ChannelId) -> sqlx::Result<()> {

        let still_tracked = threads::enumerate_tracked_channel_ids(&self.data.read().await.database)
            .await?
            .any(|id| id == channel_id);

        if !still_tracked {
            let mut data = self.data.write().await;
            data.tracked_threads.remove(&channel_id);
        }

        Ok(())
    }

    /// Check if the given channel_id is in the list of tracked threads.
    async fn tracking_thread(&self, channel_id: ChannelId) -> bool {
        self.data.read().await.tracked_threads.contains(&channel_id)
    }

    async fn process_direct_message(&self, reply_context: ReplyContext, message: Message) {
        warn!("Received direct message from user {} ({}), ignoring.", message.author.name, message.author.id);
    }
}


#[serenity::async_trait]
impl EventHandler for Handler {
    async fn reaction_add(&self, context: SerenityContext, reaction: Reaction) {
        let bot_user = self.user().await;
        if reaction.user_id == bot_user {
            // Ignore reactions made by the bot user
            return;
        }

        debug!("Received reaction {} on message {}", reaction.emoji, reaction.message_id);

        if DELETE_EMOJI.iter().any(|&emoji| reaction.emoji.unicode_eq(emoji)) {
            let channel_message = (reaction.channel_id, reaction.message_id).into();
            let mut data = self.data.write().await;
            if let Ok(message) = data
                .message_cache
                .get_or_else(&channel_message, || channel_message.fetch(&context))
                .await
            {
                if Some(message.author.id) != bot_user {
                    // Ignore reactions to messages not sent by the bot.
                    return;
                }

                if let Some(referenced_message) = &message.referenced_message {
                    info!("Processing deletion request for message {}", message.id);
                    if Some(referenced_message.author.id) == reaction.user_id {
                        if let Err(e) = message.delete(&context).await {
                            error!("Unable to delete message {:?}: {}", message, e);
                        }
                        else {
                            info!("Message deleted successfully!");
                            data.message_cache.remove(&channel_message).await;
                        }
                    }
                }
                else {
                    error!("Could not find referenced message to check requesting user ID against")
                }
            }
        }
    }

    async fn message(&self, context: SerenityContext, message: Message) {
        let user_id = message.author.id;
        if Some(user_id) == self.user().await {
            return;
        }

        if !message_is_command(&message.content) {
            if self.tracking_thread(message.channel_id).await {
                let mut data = self.data.write().await;
                debug!("Caching new message from tracked channel {}", message.channel_id);
                data.message_cache.store((message.channel_id, message.id).into(), message).await;
            }

            return;
        }

        let channel_id = message.channel_id;

        if let Ok(Channel::Guild(_)) = channel_id.to_channel(&context.http).await {
            info!("Ignored guild message: '{}'", message.content);
        }
        else {
            let reply_context = ReplyContext::new(channel_id, context);
            self.process_direct_message(reply_context, message).await;
        }
    }

    async fn guild_create(&self, _ctx: SerenityContext, guild: Guild, is_new: bool) {
        if is_new {
            info!("notified that Titi was added to a new guild: `{}` ({})!", guild.name, guild.id);
            self.data.read().await.guild_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    async fn guild_delete(
        &self,
        _ctx: SerenityContext,
        guild_partial: UnavailableGuild,
        guild_full: Option<Guild>,
    ) {
        if !guild_partial.unavailable {
            let guild_name =
                guild_full.map(|g| g.name).unwrap_or_else(|| format!("{}", guild_partial.id));
            info!(
                "notified that Titi has been removed from the `{}` guild ({})",
                guild_name, guild_partial.id
            );

            self.data.read().await.guild_count.fetch_sub(1, Ordering::SeqCst);
        }
    }

    async fn ready(&self, ctx: SerenityContext, ready: Ready) {
        let guild_count = ready.guilds.len();

        info!("connected to Discord successfully as `{}`", ready.user.name);
        info!("currently active in {} guilds", guild_count);

        self.set_user(ready.user.id).await;
        self.data.read().await.guild_count.store(guild_count, Ordering::Relaxed);

        if cfg!(debug_assertions) {
            for guild in ready.guilds {
                log_slash_commands(
                    guild
                        .id
                        .set_application_commands(&ctx, |bot_commands| {
                            commands::register_commands(bot_commands)
                        })
                        .await,
                    Some(guild.id),
                );
            }
        }
        else {
            log_slash_commands(
                Command::set_global_application_commands(&ctx, |bot_commands| {
                    commands::register_commands(bot_commands)
                })
                .await,
                None,
            );
        }

        run_periodic_tasks(ctx.into(), self).await;
    }

    async fn interaction_create(&self, ctx: SerenityContext, interaction: Interaction) {
        // FrameworkContext contains all data that poise::Framework usually manages
        let shard_manager = (*self.shard_manager.lock().unwrap()).clone().unwrap();
        let framework_data = poise::FrameworkContext {
            bot_id: self.user_id.unwrap_or_default(),
            options: &self.options,
            user_data: &self.data.read().await,
            shard_manager: &shard_manager,
        };

        poise::dispatch_event(framework_data, &ctx, &poise::Event::InteractionCreate { interaction }).await;
    }
}

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    // This is our custom error handler
    // They are many errors that can occur, so we only handle the ones we want to customize
    // and forward the rest to the default handler
    match error {
        FrameworkError::Setup { error, .. } => panic!("Failed to start bot: {:?}", error),
        FrameworkError::Command { error, ctx } => {
            error!("Error in command `{}`: {:?}", ctx.command().name, error,);
        }
        error => {
            if let Err(e) = poise::builtins::on_error(error).await {
                error!("Error while handling error: {}", e)
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    use anyhow::Context;

    tracing_subscriber::fmt::init();

    let configuration = include_str!("../Secrets.toml").parse::<Table>().unwrap();

    // Get the discord token set in `Secrets.toml`
    let token_entry = if cfg!(debug_assertions) { "DISCORD_TOKEN_DEV" } else { "DISCORD_TOKEN" };

    let discord_token = configuration[token_entry].as_str().unwrap();

    let options =
        configuration["CONNECTION_STRING"].as_str().unwrap().parse::<PgConnectOptions>()?
        .log_statements(LevelFilter::Trace);
    let database = PgPoolOptions::new().max_connections(10).connect_with(options).await?;

    // Run the schema migration
    database
        .execute(include_str!("../sql/schema.sql"))
        .await
        .context("failed to run migrations")?;

    // FrameworkOptions contains all of poise's configuration option in one struct
    // Every option can be omitted to use its default value
    let options = poise::FrameworkOptions {
        commands: vec![todo!()],
        /// The global error handler for all error cases that may occur
        on_error: |error| Box::pin(on_error(error)),
        /// This code is run before every command
        pre_command: |ctx| {
            Box::pin(async move {
                info!("Executing command {}...", ctx.command().qualified_name);
            })
        },
        /// This code is run after a command if it was successful (returned Ok)
        post_command: |ctx| {
            Box::pin(async move {
                info!("Executed command {}!", ctx.command().qualified_name);
            })
        },
        /// Every command invocation must pass this check to continue execution
        // command_check: Some(|ctx| {
        //     Box::pin(async move {
        //         if ctx.author().id == 123456789 {
        //             return Ok(false);
        //         }
        //         Ok(true)
        //     })
        // }),
        /// Enforce command checks even for owners (enforced by default)
        /// Set to true to bypass checks, which is useful for testing
        skip_checks_for_owners: false,
        event_handler: |_ctx, event, _framework, _data| {
            Box::pin(async move {
                println!("Got an event in event handler: {:?}", event.name());
                Ok(())
            })
        },
        ..Default::default()
    };

    let handler = Handler::new(options, database);

    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::GUILD_MESSAGE_REACTIONS
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::GUILDS;

    // TODO:
    // if let Err(e) = bot.update_tracked_threads().await {
    //     return Err(anyhow!(e));
    // }
    let handler = std::sync::Arc::new(handler);
    let client = Client::builder(discord_token, intents)
        .event_handler_arc(Arc::clone(&handler))
        .await;

    let mut client = match client {
        Ok(c) => c,
        Err(e) => return Err(e),
    };

    client.cache_and_http.cache.set_max_messages(1);

    let manager = client.shard_manager.clone();
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(30)).await;

            let lock = manager.lock().await;
            let runners = lock.runners.lock().await;

            for (id, runner) in runners.iter() {
                info!("Shard ID {} is {} with a latency of {:?}", id, runner.stage, runner.latency);
            }
        }
    });

    *handler.shard_manager.lock().unwrap() = Some(client.shard_manager.clone());

    if let Err(why) = client.start_autosharded().await {
        error!("Client error: {:?}", why);
        return Err(why);
    }

    Ok(())
}

fn log_slash_commands(result: serenity::Result<Vec<Command>>, guild_id: Option<GuildId>) {
    match result {
        Ok(c) => {
            let commands_registered = c.iter().fold(String::new(), |mut s, cmd| {
                if !s.is_empty() {
                    s.push_str(", ");
                }

                s.push_str(&cmd.name);
                s
            });

            info!("Commands registered: {}", commands_registered);
        },
        Err(e) => match guild_id {
            Some(g) => error!("Error setting slash commands for guild {}: {}", g, e),
            None => error!("Error setting global slash commands: {}", e),
        },
    };
}
