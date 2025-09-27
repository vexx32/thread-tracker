pub(crate) mod greetings;
pub(crate) mod help;
pub(crate) mod muses;
pub(crate) mod scheduling;
pub(crate) mod stats;
pub(crate) mod threads;
pub(crate) mod todos;
pub(crate) mod watchers;

use std::{borrow::Cow, fmt::Display};

use crate::{Data, Error};

use poise::ChoiceParameter;

pub(crate) type CommandContext<'a> = poise::Context<'a, Data, CommandError>;
pub(crate) type CommandResult<T> = std::result::Result<T, CommandError>;

#[derive(Debug)]
pub(crate) struct CommandError {
    text: Cow<'static, str>,
    inner: Option<Error>,
}

impl CommandError {
    pub(crate) fn new(text: impl Into<Cow<'static, str>>) -> Self {
        Self { text: text.into(), inner: None }
    }

    pub(crate) fn detailed(text: impl Into<Cow<'static, str>>, inner: impl Into<Error>) -> Self {
        Self { text: text.into(), inner: Some(inner.into()) }
    }
}

impl Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.inner {
            Some(err) => write!(f, "{}: {}", self.text, err),
            None => write!(f, "{}", self.text),
        }
    }
}

impl std::error::Error for CommandError {}

impl From<serenity::Error> for CommandError {
    fn from(value: serenity::Error) -> Self {
        Self { text: "Communication error".into(), inner: Some(value.into()) }
    }
}

impl From<anyhow::Error> for CommandError {
    fn from(value: anyhow::Error) -> Self {
        Self { text: value.to_string().into(), inner: None }
    }
}

impl From<sqlx::Error> for CommandError {
    fn from(value: sqlx::Error) -> Self {
        Self { text: "Database error".into(), inner: Some(value.into()) }
    }
}

#[derive(Debug, Copy, Clone, ChoiceParameter)]
pub(crate) enum SortResultsBy {
    #[name = "Oldest first"]
    OldestFirst,
    #[name = "Newest first"]
    NewestFirst,
}

/// Retrieve the full list of commands for the bot.
pub(crate) fn list() -> Vec<poise::Command<Data, CommandError>> {
    vec![
        greetings::hello(),
        help::help(),
        muses::add(),
        muses::remove(),
        muses::list(),
        stats::send_statistics(),
        scheduling::schedule(),
        threads::add(),
        threads::cleanup(),
        threads::untrack(),
        threads::set_category(),
        threads::send_list(),
        threads::send_pending_list(),
        threads::send_random_thread(),
        threads::notify_replies(),
        threads::set_timestamps(),
        todos::add(),
        todos::remove(),
        todos::list(),
        watchers::add(),
        watchers::remove(),
        watchers::list(),
    ]
}
