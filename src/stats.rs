use tracing::info;

use crate::{
    db,
    messaging::{handle_send_result, ReplyContext},
    ThreadTrackerBot,
};

/// Send the bot's statistics as a reply to the input context
///
/// ### Argument
///
/// - `reply_context` - the context to reply to
/// - `bot` - the bot instance
pub(crate) async fn send_statistics(
    reply_context: &ReplyContext,
    bot: &ThreadTrackerBot,
) -> anyhow::Result<()> {
    let stats = db::statistics(&bot.database).await?;

    let fields = [
        ("Unique users", stats.users),
        ("Servers", stats.servers),
        ("Unique threads", stats.threads_distinct),
        ("Total threads", stats.threads_total),
        ("Watchers", stats.watchers),
        ("Muses", stats.muses),
        ("To Dos", stats.todos),
    ];

    info!("sending bot statistics");
    handle_send_result(
        reply_context.send_data_embed("Thread Tracker Statistics", "", fields.into_iter()),
        &bot.message_cache,
    )
    .await;

    Ok(())
}
