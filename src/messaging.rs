use serenity::{
    http::Http,
    model::prelude::*,
    prelude::*,
    utils::Colour,
};
use tracing::{error, info};

pub(crate) async fn send_success_embed(http: impl AsRef<Http>, channel: ChannelId, title: impl ToString, body: impl ToString) {
    log_send_errors(send_embed(http, channel, title, body, Some(Colour::FABLED_PINK)).await);
}

pub(crate) async fn send_error_embed(http: impl AsRef<Http>, channel: ChannelId, title: impl ToString, body: impl ToString) {
    log_send_errors(send_embed(http, channel, title, body, Some(Colour::DARK_ORANGE)).await);
}

pub(crate) async fn send_message_embed(http: impl AsRef<Http>, channel: ChannelId, title: impl ToString, body: impl ToString) -> Result<Message, SerenityError> {
    send_embed(http, channel, title, body, None).await
}

pub(crate) fn log_send_errors(result: Result<Message, SerenityError>) {
    if let Err(err) = result {
        error!("Error sending message: {:?}", err);
    }
}

async fn send_embed(
    http: impl AsRef<Http>,
    channel: ChannelId,
    title: impl ToString,
    body: impl ToString,
    colour: Option<Colour>
) -> Result<Message, SerenityError> {
    info!("Sending embed `{}` with content `{}`", title.to_string(), body.to_string());

    channel.send_message(http, |msg| {
        msg.embed(|embed| {
            embed.title(title).description(body).colour(colour.unwrap_or(Colour::PURPLE))
        })
    }).await
}
