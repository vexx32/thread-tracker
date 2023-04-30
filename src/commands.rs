use std::fmt::Display;

use serenity::model::prelude::Attachment;
use thiserror::Error;
use tracing::error;

use crate::{
    messaging::{self, send_unknown_command, HelpMessage, ReplyContext},
    muses,
    stats,
    threads,
    todos,
    utils::EventData,
    watchers,
    ThreadTrackerBot,
};

/// Command parsing errors
#[derive(Debug, Error)]
pub(crate) enum CommandError {
    #[error("Additional arguments are required. {0}")]
    MissingArguments(String),

    #[error("Unrecognised arguments: {0}")]
    UnrecognisedArguments(String),

    #[error("Unknown command `{0}`. Use `tt!help` for a list of commands.")]
    UnknownCommand(String),
}

/// Handles mapping an input command and arguments to the bot actions.
pub(crate) struct CommandDispatcher<'a> {
    bot: &'a ThreadTrackerBot,
    event_data: EventData,
    reply_context: ReplyContext,
}

impl<'a> CommandDispatcher<'a> {
    pub fn new(
        bot: &'a ThreadTrackerBot,
        event_data: EventData,
        reply_context: ReplyContext,
    ) -> Self {
        Self { bot, event_data, reply_context }
    }

    /// Run the action associated with the input command.
    ///
    /// ### Arguments
    ///
    /// - `command` - the command which determines which action to take
    /// - `args` - any additional arguments that have been provided
    /// - `attachments` - attachments that were provided with the command
    pub async fn dispatch(&self, command: &str, args: &str, attachments: &[Attachment]) {
        match command {
            "tt!add" | "tt!track" => self.track(args).await,
            "tt!remove" | "tt!untrack" => self.untrack(args).await,
            "tt!cat" | "tt!category" => self.category(args).await,
            "tt!replies" | "tt!threads" => self.threads(args).await,
            "tt!random" => self.random(args).await,
            "tt!watch" => self.watch(args).await,
            "tt!watching" => self.watching(args).await,
            "tt!unwatch" => self.unwatch(args).await,
            "tt!muses" => self.muses(args).await,
            "tt!addmuse" => self.addmuse(args).await,
            "tt!removemuse" => self.removemuse(args).await,
            "tt!todo" => self.todo(args).await,
            "tt!done" => self.done(args).await,
            "tt!todos" | "tt!todolist" => self.todolist(args).await,
            "tt!stats" => self.stats(args).await,
            "tt!bug" => self.bug(args, attachments).await,
            cmd => self.handle_unknown_command(cmd, args).await,
        }
    }

    async fn track(&self, args: &str) {
        let args = args.split_ascii_whitespace().collect();
        self.handle_command_errors(
            threads::add(args, &self.event_data, self.bot).await,
            "Error adding tracked thread(s)",
        )
        .await;
    }

    async fn untrack(&self, args: &str) {
        let args = args.split_ascii_whitespace().collect();
        self.handle_command_errors(
            threads::remove(args, &self.event_data, self.bot).await,
            "Error removing tracked thread(s)",
        )
        .await;
    }

    async fn category(&self, args: &str) {
        let args = args.split_ascii_whitespace().collect();
        self.handle_command_errors(
            threads::set_category(args, &self.event_data, self.bot).await,
            "Error updating thread categories",
        )
        .await;
    }

    async fn threads(&self, args: &str) {
        let args = args.split_ascii_whitespace().collect();
        self.handle_command_errors(
            threads::send_list(args, &self.event_data, self.bot).await,
            "Error retrieving thread list",
        )
        .await;
    }

    async fn random(&self, args: &str) {
        let args = args.split_ascii_whitespace().collect();
        self.handle_command_errors(
            threads::send_random_thread(args, &self.event_data, self.bot).await,
            "Error retrieving a random thread",
        )
        .await;
    }

    async fn watch(&self, args: &str) {
        let args = args.split_ascii_whitespace().collect();
        self.handle_command_errors(
            watchers::add(args, &self.event_data, self.bot).await,
            "Error adding watcher",
        )
        .await;
    }

    async fn watching(&self, args: &str) {
        let args = args.split_ascii_whitespace().collect();
        self.handle_command_errors(error_on_additional_arguments(args), "Too many arguments").await;

        self.handle_command_errors(
            watchers::list(&self.event_data, self.bot).await,
            "Error listing watchers",
        )
        .await;
    }

    async fn unwatch(&self, args: &str) {
        let args = args.split_ascii_whitespace().collect();
        self.handle_command_errors(
            watchers::remove(args, &self.event_data, self.bot).await,
            "Error removing watcher",
        )
        .await;
    }

    async fn muses(&self, args: &str) {
        let args = args.split_ascii_whitespace().collect();
        self.handle_command_errors(error_on_additional_arguments(args), "Too many arguments").await;

        self.handle_command_errors(
            muses::send_list(&self.event_data, self.bot).await,
            "Error finding muses",
        )
        .await;
    }

    async fn addmuse(&self, args: &str) {
        self.handle_command_errors(
            muses::add(args, &self.event_data, self.bot).await,
            "Error adding muse",
        )
        .await;
    }

    async fn removemuse(&self, args: &str) {
        self.handle_command_errors(
            muses::remove(args, &self.event_data, self.bot).await,
            "Error removing muse",
        )
        .await;
    }

    async fn todo(&self, args: &str) {
        self.handle_command_errors(
            todos::add(args, &self.event_data, self.bot).await,
            "Error adding to do-list item",
        )
        .await;
    }

    async fn done(&self, args: &str) {
        self.handle_command_errors(
            todos::remove(args, &self.event_data, self.bot).await,
            "Error removing to do-list item",
        )
        .await;
    }

    async fn todolist(&self, args: &str) {
        let args = args.split_ascii_whitespace().collect();
        self.handle_command_errors(
            todos::send_list(args, &self.event_data, self.bot).await,
            "Error getting to do-list",
        )
        .await;
    }

    async fn stats(&self, args: &str) {
        let args = args.split_ascii_whitespace().collect();
        self.handle_command_errors(error_on_additional_arguments(args), "Too many arguments").await;

        self.handle_command_errors(
            stats::send_statistics(&self.reply_context, self.bot).await,
            "Error fetching statistics",
        )
        .await;
    }

    async fn bug(&self, args: &str, attachments: &[Attachment]) {
        self.handle_command_errors(
            messaging::submit_bug_report(
                args,
                attachments,
                &self.event_data.user,
                &self.bot.message_cache,
                &self.reply_context,
            )
            .await,
            "Failed to submit bug report",
        )
        .await;
    }

    async fn handle_unknown_command(&self, command: &str, args: &str) {
        match HelpMessage::from_command(command) {
            Some(help_message) => {
                let args = args.split_ascii_whitespace().collect();
                if let Err(e) = error_on_additional_arguments(args) {
                    self.reply_context
                        .send_error_embed("Too many arguments", e, &self.bot.message_cache)
                        .await;
                };

                self.reply_context.send_help(help_message, &self.bot.message_cache).await;
            },
            None => {
                send_unknown_command(&self.reply_context, command, &self.bot.message_cache).await;
            },
        }
    }

    async fn handle_command_errors<T, E>(&self, result: Result<T, E>, error_summary: &str)
    where
        E: Display,
    {
        if let Err(e) = result {
            self.reply_context.send_error_embed(error_summary, e, &self.bot.message_cache).await;
        }
    }
}

/// Returns `Err` if `unrecognised_args` is not empty.
pub(crate) fn error_on_additional_arguments(unrecognised_args: Vec<&str>) -> anyhow::Result<()> {
    if !unrecognised_args.is_empty() {
        return Err(CommandError::UnrecognisedArguments(unrecognised_args.join(", ")).into());
    }

    Ok(())
}
