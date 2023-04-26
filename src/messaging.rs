use std::{future::Future, sync::Arc};

use serenity::{http::Http, model::prelude::*, prelude::*, utils::Colour};
use tracing::{error, info};

use crate::{cache::MessageCache, consts::*, CommandError::*};

/// Wrapper struct to keep track of which channel and message is being replied to.
pub(crate) struct ReplyContext {
    channel_id: ChannelId,
    message_id: MessageId,
    http: Arc<Http>,
}

impl ReplyContext {
    /// Create a new `ReplyContext`
    pub(crate) fn new(channel_id: ChannelId, message_id: MessageId, http: &Arc<Http>) -> Self {
        Self { channel_id, message_id, http: Arc::clone(http) }
    }

    /// Sends a custom embed.
    ///
    /// ### Arguments
    ///
    /// - `title` - the title of the embed
    /// - `body` - the content of the embed
    /// - `colour` - the colour of the embed border
    pub(crate) async fn send_embed(
        &self,
        title: impl ToString,
        body: impl ToString,
        colour: Option<Colour>,
    ) -> Result<Message, SerenityError> {
        info!("Sending embed `{}` with content `{}`", title.to_string(), body.to_string());
        self.channel_id
            .send_message(&self.http, |msg| {
                msg.embed(|embed| {
                    embed.title(title).description(body).colour(colour.unwrap_or(Colour::PURPLE))
                })
                .reference_message((self.channel_id, self.message_id))
            })
            .await
    }

    /// Sends a 'success' confirmation embed.
    ///
    /// ### Arguments
    ///
    /// - `title` - the title of the embed
    /// - `body` - the content of the embed
    /// - `message_cache` - the cache to store sent messages in
    pub(crate) async fn send_success_embed(
        &self,
        title: impl ToString,
        body: impl ToString,
        message_cache: &MessageCache,
    ) {
        handle_send_result(self.send_embed(title, body, Some(Colour::FABLED_PINK)), message_cache)
            .await;
    }

    /// Sends an error embed.
    ///
    /// ### Argumentds
    ///
    /// - `title` - the title of the embed
    /// - `body` - the content of the embed
    /// - `message_cache` - the cache to store sent messages in
    pub(crate) async fn send_error_embed(
        &self,
        title: impl ToString,
        body: impl ToString,
        message_cache: &MessageCache,
    ) {
        handle_send_result(self.send_embed(title, body, Some(Colour::DARK_ORANGE)), message_cache)
            .await;
    }

    /// Sends a normal message embed with the default colour.
    ///
    /// ### Arguments
    ///
    /// - `title` - the title of the embed
    /// - `body` - the contents of the embed
    pub(crate) async fn send_message_embed(
        &self,
        title: impl ToString,
        body: impl ToString,
    ) -> Result<Message, SerenityError> {
        self.send_embed(title, body, None).await
    }

    /// Sends the bot's help message to the channel.
    ///
    /// ### Arguments
    ///
    /// - `message` - the type of help message to send
    /// - `reply_context` - the bot context and channel to reply to
    pub(crate) async fn send_help(&self, message: HelpMessage, message_cache: &MessageCache) {
        handle_send_result(self.send_message_embed(message.title(), message.text()), message_cache)
            .await;
    }
}

impl From<&crate::EventData> for ReplyContext {
    fn from(value: &crate::EventData) -> Self {
        Self {
            channel_id: value.channel_id,
            message_id: value.message_id,
            http: Arc::clone(&value.context.http),
        }
    }
}

/// Mapping enum to select appropriate help messages for various commands and retrieve the associated text.
pub(crate) enum HelpMessage {
    Main,
    Muses,
    Threads,
    Todos,
}

impl HelpMessage {
    /// Gets the appropriate `HelpMessage` for a given bot command.
    ///
    /// ### Arguments
    ///
    /// - `command` - the command string to fetch a help message for.
    pub fn from_command(command: &str) -> Option<Self> {
        match command {
            "tt!help" | "tt?help" => Some(Self::Main),
            "tt?muses" | "tt?addmuse" | "tt?removemuse" => Some(Self::Muses),
            "tt?threads" | "tt?replies" | "tt?add" | "tt?track" | "tt?remove" | "tt?untrack"
            | "tt?watch" | "tt?unwatch" | "tt?random" | "tt?category" => Some(Self::Threads),
            "tt?todos" | "tt?todolist" | "tt?todo" | "tt?done" => Some(Self::Todos),
            _ => None,
        }
    }

    /// Get the text for this help message.
    pub fn text(&self) -> &'static str {
        match self {
            Self::Main => HELP_MAIN,
            Self::Muses => HELP_MUSES,
            Self::Threads => HELP_THREADS,
            Self::Todos => HELP_TODOS,
        }
    }

    /// Get the message title for this help message.
    pub fn title(&self) -> &'static str {
        match self {
            Self::Main => HELP_MAIN_TITLE,
            Self::Muses => HELP_MUSES_TITLE,
            Self::Threads => HELP_THREADS_TITLE,
            Self::Todos => HELP_TODOS_TITLE,
        }
    }
}

/// Log errors encountered when sending messages, and cache successful sent messages.
///
/// ### Arguments
///
/// - `task` - the async task that attempts to send a message.
/// - `message_cache` - the cache to store sent messages in.
pub(crate) async fn handle_send_result(
    task: impl Future<Output = Result<Message, SerenityError>>,
    message_cache: &MessageCache,
) {
    match task.await {
        Ok(msg) => {
            message_cache.store((msg.id, msg.channel_id).into(), msg).await;
        },
        Err(err) => error!("Error sending message: {:?}", err),
    };
}

/// Sends an error embed indicating the input command is not recognised.
///
/// ### Arguments
///
/// - `reply_context` - the context to use when sending the reply
/// - `command` - the unrecognised command
/// - `message_cache` - the cache to store sent message in
pub(crate) async fn send_unknown_command(
    reply_context: &ReplyContext,
    command: &str,
    message_cache: &MessageCache,
) {
    info!("Unknown command received: {}", command);
    reply_context
        .send_error_embed("Unknown command", UnknownCommand(command.to_owned()), message_cache)
        .await;
}
