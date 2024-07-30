use tracing::info;

use crate::{
    commands::{CommandContext, CommandError},
    messaging::{reply, HelpMessage},
};

#[poise::command(slash_command, rename = "tt_help", category = "Help")]
/// Show the help information summary, or request detailed help for specific commands.
pub(crate) async fn help(
    ctx: CommandContext<'_>,
    #[description = "Specific command to show help about"]
    #[autocomplete = "poise::builtins::autocomplete_command"]
    command: Option<String>,
) -> Result<(), CommandError> {
    if command.is_none() {
        reply(&ctx, "Command help", HelpMessage::Main.text()).await?;
    }
    else {
        let category = ctx
            .framework()
            .options
            .commands
            .iter()
            .filter(|cmd| Some(&cmd.name) == command.as_ref())
            .map(|cmd| cmd.category.as_deref())
            .next()
            .flatten();

        info!(
            "fetching help for category '{}' (command: {})",
            category.unwrap_or("none"),
            command.as_deref().unwrap_or("none")
        );
        let help_message = HelpMessage::from_category(category);
        reply(&ctx, help_message.title(), help_message.text()).await?;
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
