use tracing::info;

use crate::{
    commands::{CommandContext, CommandError},
    consts::*,
    messaging::reply,
};

/// Mapping enum to select appropriate help messages for various commands and retrieve the associated text.
pub(crate) enum HelpMessage {
    Bugs,
    Main,
    Muses,
    Scheduling,
    Threads,
    Todos,
}

impl HelpMessage {
    /// Retrieve a specific HelpMessage based on the category name as a string, case insensitive.
    pub fn from_category(category: Option<&str>) -> Self {
        match category.map(|s| s.to_ascii_lowercase()).as_deref() {
            Some("bugs") => Self::Bugs,
            Some("muses") => Self::Muses,
            Some("threads" | "thread tracking") => Self::Threads,
            Some("todos" | "todo list") => Self::Todos,
            Some("scheduling") => Self::Scheduling,
            _ => Self::Main,
        }
    }

    /// Get the text for this help message.
    pub fn text(&self) -> &'static str {
        use help::*;

        match self {
            Self::Bugs => BUGS,
            Self::Main => MAIN,
            Self::Muses => MUSES,
            Self::Threads => THREADS,
            Self::Todos => TODOS,
            Self::Scheduling => SCHEDULING,
        }
    }

    /// Get the message title for this help message.
    pub fn title(&self) -> &'static str {
        use help::*;

        match self {
            Self::Bugs => BUGS_TITLE,
            Self::Main => MAIN_TITLE,
            Self::Muses => MUSES_TITLE,
            Self::Threads => THREADS_TITLE,
            Self::Todos => TODOS_TITLE,
            Self::Scheduling => SCHEDULING_TITLE,
        }
    }
}

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
    } else {
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
