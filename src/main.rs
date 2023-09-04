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
use serenity::{
    async_trait,
    model::{
        channel::Message,
        prelude::{command::Command, interaction::Interaction, *},
    },
    prelude::*,
    utils::MessageBuilder,
};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    ConnectOptions,
    Executor,
};
use tokio::time::sleep;
use toml::Table;
use tracing::{debug, error, info, log::LevelFilter};

mod background_tasks;
mod cache;
mod commands;
mod consts;
mod db;
mod messaging;
mod stats;
mod utils;

use background_tasks::*;
use consts::*;
use db::Database;
use messaging::*;
use utils::message_is_command;

/// Primary bot state struct that will be passed to all event handlers.
struct ThreadTrackerBot {
    /// Postgres database pool
    database: Database,
    /// Threadsafe memory cache for messages the bot has sent or looked up
    message_cache: MessageCache,
    /// The bot's current user id
    user_id: Arc<RwLock<Option<UserId>>>,
    /// The current list of tracked threads
    tracked_threads: Arc<RwLock<HashSet<ChannelId>>>,
    /// The total number of guilds the bot is in
    guild_count: AtomicUsize,
}

impl ThreadTrackerBot {
    /// Create a new bot instance.
    ///
    /// ### Arguments
    ///
    /// - `database` - database pool connection
    fn new(database: Database) -> Self {
        Self {
            database,
            message_cache: MessageCache::new(),
            user_id: Arc::new(RwLock::new(None)),
            tracked_threads: Arc::new(RwLock::new(HashSet::new())),
            guild_count: AtomicUsize::new(0),
        }
    }

    /// Sets the current user ID for the bot
    ///
    /// ### Arguments
    ///
    /// - `id` - the UserId
    async fn set_user(&self, id: UserId) {
        let mut guard = self.user_id.write().await;
        *guard = Some(id);
    }

    /// Gets the current user ID for the bot, if it's been set
    async fn user(&self) -> Option<UserId> {
        *self.user_id.read().await
    }

    async fn process_direct_message(&self, reply_context: ReplyContext, message: Message) {
        if message.author.id == DEBUG_USER {
            let mut body = MessageBuilder::new();
            body.push_codeblock_safe(format!("{:?}", message), Some("json"));

            handle_send_result(
                reply_context.send_message_embed("Debug information", body),
                &self.message_cache,
            )
            .await;
        }
        else {
            reply_context
                .send_error_embed(
                    "No direct messages please",
                    "Sorry, Titi is only designed to work in a server currently.",
                    &self.message_cache,
                )
                .await;
        }
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
            let mut tracked_threads = self.tracked_threads.write().await;
            tracked_threads.remove(&channel_id);
        }

        Ok(())
    }

    /// Check if the given channel_id is in the list of tracked threads.
    async fn tracking_thread(&self, channel_id: ChannelId) -> bool {
        self.tracked_threads.read().await.contains(&channel_id)
    }
}

#[async_trait]
impl EventHandler for ThreadTrackerBot {
    async fn reaction_add(&self, context: Context, reaction: Reaction) {
        let bot_user = self.user().await;
        if reaction.user_id == bot_user {
            // Ignore reactions made by the bot user
            return;
        }

        debug!("Received reaction {} on message {}", reaction.emoji, reaction.message_id);

        if DELETE_EMOJI.iter().any(|&emoji| reaction.emoji.unicode_eq(emoji)) {
            let channel_message = (reaction.channel_id, reaction.message_id).into();
            if let Ok(message) = self
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
                            self.message_cache.remove(&channel_message).await;
                        }
                    }
                }
                else {
                    error!("Could not find referenced message to check requesting user ID against")
                }
            }
        }
    }

    async fn message(&self, context: Context, message: Message) {
        let user_id = message.author.id;
        if Some(user_id) == self.user().await {
            return;
        }

        if !message_is_command(&message.content) {
            if self.tracking_thread(message.channel_id).await {
                debug!("Caching new message from tracked channel {}", message.channel_id);
                self.message_cache.store((message.channel_id, message.id).into(), message).await;
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

    async fn guild_create(&self, _ctx: Context, guild: Guild, is_new: bool) {
        if is_new {
            info!("notified that Titi was added to a new guild: `{}` ({})!", guild.name, guild.id);
            self.guild_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    async fn guild_delete(
        &self,
        _ctx: Context,
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
            self.guild_count.fetch_sub(1, Ordering::SeqCst);
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        let guild_count = ready.guilds.len();

        info!("connected to Discord successfully as `{}`", ready.user.name);
        info!("currently active in {} guilds", guild_count);

        self.set_user(ready.user.id).await;
        self.guild_count.store(guild_count, Ordering::Relaxed);

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

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            commands::interaction(command, self, &ctx).await;
        }
        else {
            error!("Unsupported interaction type received: {:?}", interaction)
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use anyhow::Context;

    tracing_subscriber::fmt::init();

    let configuration = include_str!("../Secrets.toml").parse::<Table>().unwrap();

    // Get the discord token set in `Secrets.toml`
    let token_entry = if cfg!(debug_assertions) { "DISCORD_TOKEN_DEV" } else { "DISCORD_TOKEN" };

    let discord_token = configuration[token_entry].as_str().unwrap();

    let mut options: PgConnectOptions =
        configuration["CONNECTION_STRING"].as_str().unwrap().parse()?;
    options.log_statements(LevelFilter::Trace);
    let database = PgPoolOptions::new().max_connections(10).connect_with(options).await?;

    // Run the schema migration
    database
        .execute(include_str!("../sql/schema.sql"))
        .await
        .context("failed to run migrations")?;

    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::GUILD_MESSAGE_REACTIONS
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::GUILDS;

    let bot = ThreadTrackerBot::new(database);
    if let Err(e) = bot.update_tracked_threads().await {
        return Err(anyhow!(e));
    }

    let mut client = Client::builder(discord_token, intents)
        .event_handler(bot)
        .await
        .expect("Err creating client");

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

    if let Err(why) = client.start_autosharded().await {
        error!("Client error: {:?}", why);
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
