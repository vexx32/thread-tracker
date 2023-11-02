use serenity::{utils::Colour, Result};

use crate::{consts::*, utils, CommandContext};

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
            Some("threads" | "thread tracking") => Self::Threads,
            Some("todos" | "todo list") => Self::Todos,
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

pub(crate) async fn reply_ephemeral<'a>(
    ctx: &CommandContext<'a>,
    title: &str,
    description: &str,
) -> Result<Vec<poise::ReplyHandle<'a>>> {
    send_chunked_reply(ctx, title, description, Colour::BLURPLE, true).await
}

pub(crate) async fn reply<'a>(
    ctx: &CommandContext<'a>,
    title: &str,
    description: &str,
) -> Result<Vec<poise::ReplyHandle<'a>>> {
    send_chunked_reply(ctx, title, description, Colour::PURPLE, false).await
}

pub(crate) async fn reply_error<'a>(
    ctx: &CommandContext<'a>,
    title: &str,
    description: &str,
) -> Result<Vec<poise::ReplyHandle<'a>>> {
    send_chunked_reply(ctx, title, description, Colour::RED, false).await
}

pub(crate) async fn send_chunked_reply<'a>(
    ctx: &CommandContext<'a>,
    title: &str,
    description: &str,
    colour: Colour,
    ephemeral: bool,
) -> Result<Vec<poise::ReplyHandle<'a>>> {
    let messages = utils::split_into_chunks(description, MAX_EMBED_CHARS);
    let mut results = Vec::new();

    for msg in messages {
        results.push(
            ctx.send(|reply| {
                reply
                    .embed(|embed| embed.title(title).description(msg).colour(colour))
                    .ephemeral(ephemeral)
            })
            .await?,
        );
    }

    Ok(results)
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
