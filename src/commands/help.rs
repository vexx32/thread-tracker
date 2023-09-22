use crate::{TitiContext, TitiError};

#[poise::command(slash_command, rename = "tt_help")]
pub async fn help(
    ctx: TitiContext<'_>,
    #[description = "Specific command to show help about"]
    #[autocomplete = "poise::builtins::autocomplete_command"]
    command: Option<String>,
) -> Result<(), TitiError> {
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
    Ok(())
}
