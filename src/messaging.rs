use std::{borrow::Cow, fmt::Display};

use anyhow::anyhow;
use poise::{serenity_prelude::*, CreateReply, ReplyHandle};
use serenity::{
    builder::{CreateEmbed, CreateMessage},
    http::CacheHttp,
    model::Colour,
    Result,
};
use tracing::error;

use crate::{
    commands::{CommandContext, CommandResult},
    consts::*,
    utils,
};

/// Button options for a confirmation prompt.
pub(crate) enum ConfirmationResponse {
    Confirm,
    Cancel,
}

impl Display for ConfirmationResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Confirm => "confirm",
            Self::Cancel => "cancel",
        };

        write!(f, "{}", s)
    }
}

impl Into<String> for ConfirmationResponse {
    fn into(self) -> String {
        self.to_string()
    }
}

/// Send the target user a private/direct message.
pub(crate) async fn dm(
    ctx: impl CacheHttp,
    user_id: UserId,
    message: &str,
    embed_title: Option<&str>,
    embed_description: Option<&str>,
) -> Result<()> {
    let channel = user_id.create_dm_channel(&ctx).await?;

    let mut message = CreateMessage::new().content(message);

    match (embed_title, embed_description) {
        (Some(_), _) | (_, Some(_)) => {
            let embed = CreateEmbed::new()
                .title(embed_title.unwrap_or(""))
                .description(embed_description.unwrap_or(""))
                .colour(Colour::PURPLE);
            message = message.embed(embed);
        },
        _ => {},
    }

    channel.send_message(ctx, message).await?;

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
        let embed = CreateEmbed::default().title(title).description(msg).colour(colour);
        let reply = CreateReply::default().embed(embed).ephemeral(ephemeral);
        results.push(ctx.send(reply).await?);
    }

    Ok(results)
}

/// Send an ephemeral reply message to confirm a user action.
/// Use the returned message handle to wait for an interaction response from the buttons,
/// using the `ConfirmationResponse` enum IDs to check which button was selected.
pub(crate) async fn send_confirmation_prompt<'a, S>(
    ctx: &CommandContext<'a>,
    title: S,
    description: S,
) -> anyhow::Result<ReplyHandle<'a>>
where
    S: Into<Cow<'a, str>>,
{
    let embed = CreateEmbed::default()
        .title(title.into())
        .description(description.into())
        .colour(Colour::BLURPLE);
    let components = vec![CreateActionRow::Buttons(vec![
        CreateButton::new(ConfirmationResponse::Confirm)
            .label("Confirm")
            .style(ButtonStyle::Danger),
        CreateButton::new(ConfirmationResponse::Cancel)
            .label("Cancel")
            .style(ButtonStyle::Secondary),
    ])];

    let reply = CreateReply::default()
        .embed(embed)
        .ephemeral(true)
        .components(components);
    Ok(ctx.send(reply).await?)
}

pub(crate) async fn edit_message<'a, S>(
    ctx: CommandContext<'a>,
    handle: ReplyHandle<'a>,
    title: Option<S>,
    description: Option<S>,
    colour: Option<Colour>,
    remove_components: bool,
) -> anyhow::Result<()>
where
    S: Into<Cow<'a, str>>,
{
    let mut embed = CreateEmbed::default();
    if let Some(t) = title {
        embed = embed.title(t.into())
    }

    if let Some(d) = description {
        embed = embed.description(d.into())
    }

    if let Some(c) = colour {
        embed = embed.colour(c);
    }

    let mut reply = CreateReply::default().embed(embed);

    if remove_components {
        reply = reply.components(Vec::new());
    }

    Ok(handle.edit(ctx, reply).await?)
}

pub(crate) async fn send_message<'a, S>(
    ctx: impl CacheHttp,
    channel_id: ChannelId,
    title: S,
    description: S,
    colour: Colour,
) -> anyhow::Result<()>
where
    S: Into<Cow<'a, str>>,
{
    let Some(channel) = channel_id.to_channel(&ctx).await?.guild() else {
        return Err(anyhow!(
            "This method can only be used to send messages to guild channels"
        ));
    };

    let embed = CreateEmbed::new()
        .title(title.into())
        .description(description.into())
        .colour(colour);
    let message = CreateMessage::default().add_embed(embed);
    channel.send_message(ctx, message).await?;

    Ok(())
}

pub(crate) async fn send_invalid_command_call_error(ctx: CommandContext<'_>) -> CommandResult<()> {
    const ERROR_TEXT: &'static str = "The command you called is not intended to be called directly. This may happen if command registrations have been recently updated. Check for any subcommands or other options when trying to enter the command and use those as well instead of only this base command.";
    let result = whisper_error(&ctx, "Invalid command called", ERROR_TEXT).await;

    if let Err(e) = result {
        error!("Error sending an error response to the user: {}", e);
    }

    Ok(())
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
