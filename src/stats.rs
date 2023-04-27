use serenity::utils::MessageBuilder;

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

    let mut message = MessageBuilder::new();

    message
        .push_bold("Unique users: ")
        .push_line(stats.users)
        .push_bold("Servers: ")
        .push_line(stats.servers)
        .push_bold("Unique threads: ")
        .push_line(stats.threads_distinct)
        .push_bold("Total threads: ")
        .push_line(stats.threads_total)
        .push_bold("Watchers: ")
        .push_line(stats.watchers)
        .push_bold("Muses: ")
        .push_line(stats.muses)
        .push_bold("To do-list items: ")
        .push_line(stats.todos);

    handle_send_result(
        reply_context.send_message_embed("Thread Tracker Statistics", message),
        &bot.message_cache,
    )
    .await;

    Ok(())
}
