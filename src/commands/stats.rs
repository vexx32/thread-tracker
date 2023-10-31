use serenity::utils::MessageBuilder;
use tracing::info;

use crate::{db, TitiContext, TitiResponse, commands::CommandResult};

/// Send the bot's statistics as a reply to the input context
#[poise::command(slash_command, dm_only, rename = "tt_stats")]
pub(crate) async fn send_statistics(
    ctx: TitiContext<'_>,
) -> CommandResult<()> {
    ctx.defer_ephemeral().await?;

    let data = ctx.data();
    let stats = db::statistics(&data.database).await?;

    let mut message = MessageBuilder::new();

    message.push("- **Unique Users** — ").push_line(stats.users);
    message.push("- **Servers (Active)** — ").push_line(stats.servers);
    message.push("- **Servers (Total)** — ").push_line(data.guilds());
    message.push("- **Threads (Unique)** — ").push_line(stats.threads_distinct);
    message.push("- **Threads (Total)** — ").push_line(stats.threads_total);
    // message.push("- **Watchers** — ").push_line(stats.watchers);
    message.push("- **Muses** — ").push_line(stats.muses);
    message.push("- **To Dos** — ").push_line(stats.todos);

    let user = ctx.author();
    info!("sending bot statistics to {} ({})", &user.name, user.id);

    ctx.reply_success("Titi Statistics", &message.build()).await;

    Ok(())
}
