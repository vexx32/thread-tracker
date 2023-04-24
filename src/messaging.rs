use std::{future::Future, sync::Arc};

use serenity::{http::Http, model::prelude::*, prelude::*, utils::Colour};
use tracing::{error, info};

use crate::{
    cache::MessageCache,
    consts::*,
    CommandError::*,
};

pub(crate) struct ReplyContext {
    channel_id: ChannelId,
    message_id: MessageId,
    http: Arc<Http>,
}

impl ReplyContext {
    pub(crate) fn new(channel_id: ChannelId, message_id: MessageId, http: &Arc<Http>) -> Self {
        Self { channel_id, message_id, http: Arc::clone(http) }
    }

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

    pub(crate) async fn send_success_embed(
        &self,
        title: impl ToString,
        body: impl ToString,
        message_cache: &MessageCache,
    ) {
        handle_send_result(self.send_embed(title, body, Some(Colour::FABLED_PINK)), message_cache)
            .await;
    }

    pub(crate) async fn send_error_embed(
        &self,
        title: impl ToString,
        body: impl ToString,
        message_cache: &MessageCache,
    ) {
        handle_send_result(self.send_embed(title, body, Some(Colour::DARK_ORANGE)), message_cache)
            .await;
    }

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
    /// - `reply_context` - the bot context and channel to reply to
    pub(crate) async fn send_help(&self, message: HelpMessage, message_cache: &MessageCache) {
        handle_send_result(
            self.send_message_embed(message.title(), message.text()),
            message_cache,
        )
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

pub(crate) enum HelpMessage {
    Main,
    Muses,
    Threads,
    Todos,
}

impl HelpMessage {
    pub fn from_command(command: &str) -> Option<Self> {
        match command {
            "tt!help"
            | "tt?help" => Some(Self::Main),
            "tt?muses"
            | "tt?addmuse"
            | "tt?removemuse" => Some(Self::Muses),
            "tt?threads"
            | "tt?replies"
            | "tt?add"
            | "tt?track"
            | "tt?remove"
            | "tt?untrack"
            | "tt?watch"
            | "tt?unwatch"
            | "tt?random" => Some(Self::Threads),
            "tt?todos"
            | "tt?todolist"
            | "tt?todo"
            | "tt?done" => Some(Self::Todos),
            _ => None,
        }
    }

    pub fn text(&self) -> &'static str {
        match self {
            Self::Main => HELP_MAIN,
            Self::Muses => HELP_MUSES,
            Self::Threads => HELP_THREADS,
            Self::Todos => HELP_TODOS,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::Main => HELP_MAIN_TITLE,
            Self::Muses => HELP_MUSES_TITLE,
            Self::Threads => HELP_THREADS_TITLE,
            Self::Todos => HELP_TODOS_TITLE,
        }
    }
}

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

pub(crate) async fn send_unknown_command(reply_context: &ReplyContext, command: &str, message_cache: &MessageCache) {
    info!("Unknown command received: {}", command);
    reply_context
        .send_error_embed(
            "Unknown command",
            UnknownCommand(command.to_owned()),
            message_cache,
        )
        .await;
}
