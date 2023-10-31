use crate::{SlashCommandContext, CommandError, TitiReplyContext, messaging::HelpMessage};

use tracing::info;


#[poise::command(slash_command, rename = "tt_help", category = "Help")]
/// Show the help information summary, or request detailed help for specific commands.
pub(crate) async fn help(
    ctx: SlashCommandContext<'_>,
    #[description = "Specific command to show help about"]
    #[autocomplete = "poise::builtins::autocomplete_command"]
    command: Option<String>,
) -> Result<(), CommandError> {
    if command.is_none() {
        ctx.reply_success("Command help", HelpMessage::Main.text()).await?;
    }
    else {
        let category = ctx.framework().options.commands.iter().filter(|cmd| Some(&cmd.name) == command.as_ref())
            .map(|cmd| cmd.category)
            .next()
            .flatten();

        info!("fetching help for category '{}' (command: {})", category.unwrap_or("none"), command.as_deref().unwrap_or("none"));
        let help_message = HelpMessage::from_category(category);
        ctx.reply_success(help_message.title(), help_message.text()).await?;
    }

    if cfg!(debug_assertions) {
        poise::builtins::help(
            ctx,
            command.as_deref(),
            poise::builtins::HelpConfiguration {
                ephemeral: false,
                extra_text_at_bottom: "React with 🗑️ or 🚫 to delete this message",
                ..Default::default()
            },
        )
        .await?;
    }
    Ok(())
}
