use std::{sync::Arc, future::Future};

use serenity::{
    http::Http,
    model::prelude::*,
    prelude::*,
    utils::Colour,
};
use tracing::{error, info};

use crate::cache::MessageCache;

const EMBED_FOOTER: &str = "React with 🚫 to delete this message.";

pub(crate) struct ReplyContext
{
    channel_id: ChannelId,
    message_id: MessageId,
    http: Arc<Http>,
}

impl ReplyContext {
    pub(crate) fn new(channel_id: ChannelId, message_id: MessageId, http: &Arc<Http>) -> Self {
        Self {
            channel_id,
            message_id,
            http: Arc::clone(http),
        }
    }

    pub(crate) async fn send_embed(&self, title: impl ToString, body: impl ToString, colour: Option<Colour>) -> Result<Message, SerenityError> {
        info!("Sending embed `{}` with content `{}`", title.to_string(), body.to_string());
        self.channel_id.send_message(&self.http, |msg| {
            msg.embed(|embed| embed.title(title).description(body).colour(colour.unwrap_or(Colour::PURPLE)).footer(|f| f.text(EMBED_FOOTER)))
                .reference_message((self.channel_id, self.message_id))
        }).await
    }

    pub(crate) async fn send_success_embed(&self, title: impl ToString, body: impl ToString, message_cache: &MessageCache) {
        log_send_errors(self.send_embed(title, body, Some(Colour::FABLED_PINK)), message_cache).await;
    }

    pub(crate) async fn send_error_embed(&self, title: impl ToString, body: impl ToString, message_cache: &MessageCache) {
        log_send_errors(self.send_embed(title, body, Some(Colour::DARK_ORANGE)), message_cache).await;
    }

    pub(crate) async fn send_message_embed(&self, title: impl ToString, body: impl ToString) -> Result<Message, SerenityError> {
        self.send_embed(title, body, None).await
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

pub(crate) async fn log_send_errors(task: impl Future<Output = Result<Message, SerenityError>>, message_cache: &MessageCache) {
    match task.await {
        Ok(msg) => {
            message_cache.store((msg.id, msg.channel_id).into(), msg).await;
        },
        Err(err) => error!("Error sending message: {:?}", err),
    };
}
