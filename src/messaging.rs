use poise::serenity_prelude::UserId;
use serenity::{utils::Colour, Result, http::CacheHttp};

use crate::{commands::CommandContext, consts::*, utils};

/// Mapping enum to select appropriate help messages for various commands and retrieve the associated text.
pub(crate) enum HelpMessage {
    Bugs,
    Main,
    Muses,
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

/// Send the target user a private/direct message.
pub(crate) async fn dm(ctx: impl CacheHttp, user_id: UserId, message: &str, embed_title: Option<&str>, embed_description: Option<&str>) -> Result<()> {
    let channel = user_id.create_dm_channel(&ctx).await?;
    channel.send_message(ctx.http(), |msg| {
        msg.content(message);
        if embed_title.is_some() || embed_description.is_some(){
            msg.embed(|embed|
                embed
                    .title(embed_title.unwrap())
                    .description(embed_description.unwrap())
                    .colour(Colour::PURPLE));
        }

        msg
    })
    .await?;


    Ok(())
}

/// Send an ephemeral reply.
pub(crate) async fn whisper<'a>(
    ctx: &CommandContext<'a>,
    title: &str,
    description: &str,
) -> Result<Vec<poise::ReplyHandle<'a>>> {
    send_chunked_reply(ctx, title, description, Colour::BLURPLE, true).await
}

/// Send an ephemeral error response.
pub(crate) async fn whisper_error<'a>(
    ctx: &CommandContext<'a>,
    title: &str,
    description: &str,
) -> Result<Vec<poise::ReplyHandle<'a>>> {
    send_chunked_reply(ctx, title, description, Colour::ROSEWATER, true).await
}

/// Send a reply.
pub(crate) async fn reply<'a>(
    ctx: &CommandContext<'a>,
    title: &str,
    description: &str,
) -> Result<Vec<poise::ReplyHandle<'a>>> {
    send_chunked_reply(ctx, title, description, Colour::PURPLE, false).await
}

/// Send an error response.
pub(crate) async fn reply_error<'a>(
    ctx: &CommandContext<'a>,
    title: &str,
    description: &str,
) -> Result<Vec<poise::ReplyHandle<'a>>> {
    send_chunked_reply(ctx, title, description, Colour::RED, false).await
}

/// Send a reply, divided into chunks if needed, to fit replies into Discord's message limit.
async fn send_chunked_reply<'a>(
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
