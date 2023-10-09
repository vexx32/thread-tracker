use serenity::{
    builder::CreateEmbed,
    model::prelude::*,
    prelude::*,
    utils::Colour,
};

use crate::consts::*;

/// Wrapper struct to keep track of which channel and message is being replied to.
pub(crate) struct ReplyContext {
    channel_id: ChannelId,
    context: Context,
}

impl ReplyContext {
    /// Create a new ReplyContext.
    pub(crate) fn new(channel_id: ChannelId, ctx: Context) -> Self {
        Self { channel_id, context: ctx }
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
            })
            .await
    }

    // /// Sends a 'success' confirmation embed.
    // ///
    // /// ### Arguments
    // ///
    // /// - `title` - the title of the embed
    // /// - `body` - the content of the embed
    // /// - `message_cache` - the cache to store sent messages in
    // pub(crate) async fn send_success_embed(
    //     &self,
    //     title: impl ToString,
    //     body: impl ToString,
    //     message_cache: &MessageCache,
    // ) {
    //     handle_send_result(self.send_embed(title, body, Some(Colour::FABLED_PINK)), message_cache)
    //         .await;
    // }

    // /// Sends an error embed.
    // ///
    // /// ### Argumentds
    // ///
    // /// - `title` - the title of the embed
    // /// - `body` - the content of the embed
    // /// - `message_cache` - the cache to store sent messages in
    // pub(crate) async fn send_error_embed(
    //     &self,
    //     title: impl ToString,
    //     body: impl ToString,
    //     message_cache: &MessageCache,
    // ) {
    //     error!("{}", body.to_string());
    //     handle_send_result(self.send_embed(title, body, Some(Colour::DARK_ORANGE)), message_cache)
    //         .await;
    // }

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

    // /// Sends an embed where the primary content is arranged in fields for representing structured data.
    // ///
    // /// ### Arguments
    // ///
    // /// - `title` - the embed title
    // /// - `body` - the contents of the embed
    // /// - `fields` - the data to display in the embed's fields
    // pub(crate) async fn send_data_embed<T, U>(
    //     &self,
    //     title: impl ToString,
    //     body: impl ToString,
    //     fields: impl Iterator<Item = (T, U)>,
    // ) -> Result<Message, SerenityError>
    // where
    //     T: ToString,
    //     U: ToString,
    // {
    //     self.send_custom_embed(title, body, |embed| {
    //         embed.colour(Colour::TEAL).fields(fields.map(|(name, value)| (name, value, true)))
    //     })
    //     .await
    // }
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
    pub fn from_category(category: Option<&str>) -> Self {
        match category.map(|s| s.to_ascii_lowercase()).as_deref() {
            Some("bugs") => Self::Bugs,
            Some("muses") => Self::Muses,
            Some("threads") => Self::Threads,
            Some("todos") => Self::Todos,
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

// /// Log errors encountered when sending messages, and cache successful sent messages.
// ///
// /// ### Arguments
// ///
// /// - `task` - the async task that attempts to send a message.
// /// - `message_cache` - the cache to store sent messages in.
// pub(crate) async fn handle_send_result(
//     task: impl Future<Output = Result<Message, SerenityError>>,
//     message_cache: &MessageCache,
// ) {
//     match task.await {
//         Ok(msg) => {
//             message_cache.store((msg.id, msg.channel_id).into(), msg).await;
//         },
//         Err(err) => error!("Error sending message: {:?}", err),
//     };
// }

// pub(crate) async fn submit_bug_report(
//     message: &str,
//     attachments: &[Attachment],
//     reporting_user: &User,
//     message_cache: &MessageCache,
//     reply_context: &ReplyContext,
// ) -> anyhow::Result<()> {
//     if message.trim().is_empty() {
//         return Ok(());
//     }

//     let mut report = MessageBuilder::new();
//     report
//         .push("__**Bug Report**__ from ")
//         .push_line(reporting_user.mention())
//         .push_line("")
//         .push_line(message);

//     let target_user = UserId(DEBUG_USER);

//     let dm = target_user
//         .to_user(&reply_context.context)
//         .await?
//         .direct_message(&reply_context.context, |msg| {
//             msg.content(report)
//                 .add_files(
//                     attachments
//                         .iter()
//                         .filter_map(|a| url::Url::parse(&a.url).ok())
//                         .map(AttachmentType::Image),
//                 )
//                 .embed(|embed| {
//                     embed
//                         .title("Reported By")
//                         .field(
//                             "User",
//                             format!("{} #{}", reporting_user.name, reporting_user.discriminator),
//                             true,
//                         )
//                         .field("User ID", reporting_user.id, true)
//                 })
//         })
//         .await?;

//     message_cache.store((dm.channel_id, dm.id).into(), dm).await;
//     reply_context
//         .send_success_embed(
//             "Bug report submitted successfully!",
//             "Your bug report has been sent.",
//             message_cache,
//         )
//         .await;

//     Ok(())
// }
