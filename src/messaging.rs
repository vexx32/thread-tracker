use std::sync::Arc;

use serenity::{
    http::Http,
    model::prelude::*,
    prelude::*,
    utils::Colour,
};
use tracing::{error, info};

pub(crate) struct ReplyContext
{
    channel_id: ChannelId,
    http: Arc<Http>,
}

impl ReplyContext {
    pub(crate) fn new(channel_id: ChannelId, http: &Arc<Http>) -> Self {
        Self {
            channel_id,
            http: Arc::clone(http),
        }
    }

    pub(crate) async fn send_embed(&self, title: impl ToString, body: impl ToString, colour: Option<Colour>) -> Result<Message, SerenityError> {
        info!("Sending embed `{}` with content `{}`", title.to_string(), body.to_string());
        self.channel_id.send_message(&self.http, |msg| {
            msg.embed(|embed| embed.title(title).description(body).colour(colour.unwrap_or(Colour::PURPLE)))
        }).await
    }

    pub(crate) async fn send_success_embed(&self, title: impl ToString, body: impl ToString) {
        log_send_errors(self.send_embed(title, body, Some(Colour::FABLED_PINK)).await);
    }

    pub(crate) async fn send_error_embed(&self, title: impl ToString, body: impl ToString) {
        log_send_errors(self.send_embed(title, body, Some(Colour::DARK_ORANGE)).await);
    }

    pub(crate) async fn send_message_embed(&self, title: impl ToString, body: impl ToString) -> Result<Message, SerenityError> {
        self.send_embed(title, body, None).await
    }
}

impl From<&crate::EventData> for ReplyContext {
    fn from(value: &crate::EventData) -> Self {
        Self {
            channel_id: value.channel_id,
            http: Arc::clone(&value.context.http),
        }
    }
}

pub(crate) fn log_send_errors(result: Result<Message, SerenityError>) {
    if let Err(err) = result {
        error!("Error sending message: {:?}", err);
    }
}
