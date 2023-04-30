use std::future::Future;

use serenity::{
    builder::CreateEmbed,
    model::prelude::*,
    prelude::*,
    utils::{Colour, MessageBuilder},
};
use tracing::{error, info};

use crate::{
    cache::MessageCache,
    commands::CommandError::{self, *},
    consts::*,
};

/// Wrapper struct to keep track of which channel and message is being replied to.
pub(crate) struct ReplyContext {
    channel_id: ChannelId,
    message_id: MessageId,
    context: Context,
}

impl ReplyContext {
    /// Create a new ReplyContext.
    pub(crate) fn new(channel_id: ChannelId, message_id: MessageId, ctx: Context) -> Self {
        Self { channel_id, message_id, context: ctx }
    }

    /// Sends a custom embed.
    ///
    /// ### Arguments
    ///
    /// - `title` - the title of the embed
    /// - `body` - the content of the embed
    /// - `colour` - the colour of the embed border
    pub(crate) async fn send_embed(
        &self,
        title: impl ToString,
        body: impl ToString,
        colour: Option<Colour>,
    ) -> Result<Message, SerenityError> {
        self.send_custom_embed(title, body, |embed| embed.colour(colour.unwrap_or(Colour::PURPLE)))
            .await
    }

    async fn send_custom_embed<F>(
        &self,
        title: impl ToString,
        body: impl ToString,
        f: F,
    ) -> Result<Message, SerenityError>
    where
        F: FnOnce(&mut CreateEmbed) -> &mut CreateEmbed,
    {
        self.channel_id
            .send_message(&self.context, |msg| {
                msg.embed(|embed| f(embed.title(title).description(body)))
                    .reference_message((self.channel_id, self.message_id))
            })
            .await
    }

    /// Sends a 'success' confirmation embed.
    ///
    /// ### Arguments
    ///
    /// - `title` - the title of the embed
    /// - `body` - the content of the embed
    /// - `message_cache` - the cache to store sent messages in
    pub(crate) async fn send_success_embed(
        &self,
        title: impl ToString,
        body: impl ToString,
        message_cache: &MessageCache,
    ) {
        handle_send_result(self.send_embed(title, body, Some(Colour::FABLED_PINK)), message_cache)
            .await;
    }

    /// Sends an error embed.
    ///
    /// ### Argumentds
    ///
    /// - `title` - the title of the embed
    /// - `body` - the content of the embed
    /// - `message_cache` - the cache to store sent messages in
    pub(crate) async fn send_error_embed(
        &self,
        title: impl ToString,
        body: impl ToString,
        message_cache: &MessageCache,
    ) {
        error!("{}", body.to_string());
        handle_send_result(self.send_embed(title, body, Some(Colour::DARK_ORANGE)), message_cache)
            .await;
    }

    /// Sends a normal message embed with the default colour.
    ///
    /// ### Arguments
    ///
    /// - `title` - the title of the embed
    /// - `body` - the contents of the embed
    pub(crate) async fn send_message_embed(
        &self,
        title: impl ToString,
        body: impl ToString,
    ) -> Result<Message, SerenityError> {
        self.send_embed(title, body, None).await
    }

    /// Sends an embed where the primary content is arranged in fields for representing structured data.
    ///
    /// ### Arguments
    ///
    /// - `title` - the embed title
    /// - `body` - the contents of the embed
    /// - `fields` - the data to display in the embed's fields
    pub(crate) async fn send_data_embed<T, U>(
        &self,
        title: impl ToString,
        body: impl ToString,
        fields: impl Iterator<Item = (T, U)>,
    ) -> Result<Message, SerenityError>
    where
        T: ToString,
        U: ToString,
    {
        self.send_custom_embed(title, body, |embed| {
            embed.colour(Colour::TEAL).fields(fields.map(|(name, value)| (name, value, true)))
        })
        .await
    }

    /// Sends the bot's help message to the channel.
    ///
    /// ### Arguments
    ///
    /// - `message` - the type of help message to send
    /// - `reply_context` - the bot context and channel to reply to
    pub(crate) async fn send_help(&self, message: HelpMessage, message_cache: &MessageCache) {
        handle_send_result(self.send_message_embed(message.title(), message.text()), message_cache)
            .await;
    }
}

impl From<&crate::EventData> for ReplyContext {
    fn from(value: &crate::EventData) -> Self {
        Self {
            channel_id: value.channel_id,
            message_id: value.message_id,
            context: value.context.clone(),
        }
    }
}

/// Mapping enum to select appropriate help messages for various commands and retrieve the associated text.
pub(crate) enum HelpMessage {
    Bugs,
    Main,
    Muses,
    Threads,
    Todos,
}

impl HelpMessage {
    /// Gets the appropriate `HelpMessage` for a given bot command.
    ///
    /// ### Arguments
    ///
    /// - `command` - the command string to fetch a help message for.
    pub fn from_command(command: &str) -> Option<Self> {
        match command {
            "tt?bug" | "tt?bugs" => Some(Self::Bugs),
            "tt!help" | "tt?help" => Some(Self::Main),
            "tt?muses" | "tt?addmuse" | "tt?removemuse" => Some(Self::Muses),
            "tt?threads" | "tt?replies" | "tt?add" | "tt?track" | "tt?remove" | "tt?untrack"
            | "tt?watch" | "tt?unwatch" | "tt?watching" | "tt?random" | "tt?category" => {
                Some(Self::Threads)
            },
            "tt?todos" | "tt?todolist" | "tt?todo" | "tt?done" => Some(Self::Todos),
            _ => None,
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
        }
    }
}

/// Log errors encountered when sending messages, and cache successful sent messages.
///
/// ### Arguments
///
/// - `task` - the async task that attempts to send a message.
/// - `message_cache` - the cache to store sent messages in.
pub(crate) async fn handle_send_result(
    task: impl Future<Output = Result<Message, SerenityError>>,
    message_cache: &MessageCache,
) {
    match task.await {
        Ok(msg) => {
            message_cache.store((msg.id, msg.channel_id).into(), msg).await;
        },
        Err(err) => error!("Error sending message: {:?}", err),
    };
}

/// Sends an error embed indicating the input command is not recognised.
///
/// ### Arguments
///
/// - `reply_context` - the context to use when sending the reply
/// - `command` - the unrecognised command
/// - `message_cache` - the cache to store sent message in
pub(crate) async fn send_unknown_command(
    reply_context: &ReplyContext,
    command: &str,
    message_cache: &MessageCache,
) {
    info!("Unknown command received: {}", command);
    reply_context
        .send_error_embed("Unknown command", UnknownCommand(command.to_owned()), message_cache)
        .await;
}

pub(crate) async fn submit_bug_report(
    message: &str,
    attachments: &[Attachment],
    reporting_user: &User,
    message_cache: &MessageCache,
    reply_context: &ReplyContext,
) -> anyhow::Result<()> {
    if message.trim().is_empty() {
        return Err(CommandError::MissingArguments("Please provide a summary of the bug, reproduction steps, and a screenshot if you're comfortable doing so.".to_owned()).into());
    }

    let mut report = MessageBuilder::new();
    report
        .push("__**Bug Report**__ from ")
        .push_line(reporting_user.mention())
        .push_line("")
        .push_line(message);

    let target_user = UserId(DEBUG_USER);

    let dm = target_user
        .to_user(&reply_context.context)
        .await?
        .direct_message(&reply_context.context, |msg| {
            msg.content(report)
                .add_files(
                    attachments
                        .iter()
                        .filter_map(|a| url::Url::parse(&a.url).ok())
                        .map(AttachmentType::Image),
                )
                .embed(|embed| {
                    embed
                        .title("Reported By")
                        .field(
                            "User",
                            format!("{} #{}", reporting_user.name, reporting_user.discriminator),
                            true,
                        )
                        .field("User ID", reporting_user.id, true)
                })
        })
        .await?;

    message_cache.store((dm.channel_id, dm.id).into(), dm).await;
    reply_context
        .send_success_embed(
            "Bug report submitted successfully!",
            "Your bug report has been sent.",
            message_cache,
        )
        .await;

    Ok(())
}
