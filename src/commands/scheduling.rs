use std::{collections::HashMap, str::FromStr};

use anyhow::anyhow;
use chrono::{DateTime, Days, Months, NaiveDateTime, TimeDelta, Utc};
use poise::choice_parameter::ChoiceParameter;
use regex::Regex;
use serenity::{
    model::prelude::*,
    prelude::*,
    utils::MessageBuilder,
};
use tracing::{error, info, warn};

use crate::{
    commands::{CommandContext, CommandError, CommandResult},
    consts::setting_names::*,
    db::{
        Database,
        add_scheduled_message, delete_scheduled_message, get_scheduled_message, get_user_setting,
        update_user_setting, update_scheduled_message, list_scheduled_messages,
    },
    messaging::{reply, reply_error, send_invalid_command_call_error, whisper, whisper_error},
    utils::truncate_string,
};

#[derive(Debug)]
#[repr(transparent)]
pub struct TimeZoneParameter(chrono_tz::Tz);

impl ChoiceParameter for TimeZoneParameter {
    fn list() -> Vec<poise::CommandParameterChoice> {
        chrono_tz::TZ_VARIANTS
            .iter()
            .map(|tz| poise::CommandParameterChoice {
                name: tz.name().to_owned(),
                localizations: HashMap::new(),
                __non_exhaustive: (),
            })
            .collect()
    }

    fn from_index(index: usize) -> Option<Self> {
        chrono_tz::TZ_VARIANTS.get(index).map(|&tz| Self(tz))
    }

    fn from_name(name: &str) -> Option<Self> {
        chrono_tz::TZ_VARIANTS.iter().find(|tz| tz.name() == name).map(|&tz| Self(tz))
    }

    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn localized_name(&self, _locale: &str) -> Option<&'static str> {
        None
    }
}

/// Manage scheduled messages
#[poise::command(
    slash_command,
    guild_only,
    rename = "tt_schedule",
    category = "Scheduling",
    subcommands("add_message", "remove_message", "update_message", "list__messages", "set_timezone")
)]
pub(crate) async fn schedule(ctx: CommandContext<'_>) -> CommandResult<()> {
    send_invalid_command_call_error(ctx).await
}

#[poise::command(slash_command, guild_only, rename = "list", category = "Scheduling")]
pub(crate) async fn list_messages(
    ctx: CommandContext<'_>,
) -> CommandResult<()> {
    const REPLY_TITLE: &str = "List scheduled messages";
    let data = ctx.data();
    let author = ctx.author();
    let messages = list_scheduled_messages(&data.database, author.id).await?;

    if messages.is_empty() {
        reply(&ctx, REPLY_TITLE, "You have no scheduled messages.").await?;
    }
    else {
        let mut content = MessageBuilder::new();
        for msg in messages {
            content
                .push("- ")
                .push_bold(msg.id.to_string())
                .push(": ")
                .push(&msg.title)
                .push(" in ")
                .mention(&msg.channel_id())
                .push(" @ ")
                .push(&msg.datetime);

            if !msg.repeat.is_empty() && msg.repeat != "None" {
                content
                    .push(" (repeating every ")
                    .push(msg.repeat)
                    .push(")");
            }

            content.push_line("");
        }

        reply(&ctx, REPLY_TITLE, &content.build()).await?;
    }

    Ok(())
}

/// Update an existing scheduled message
#[poise::command(slash_command, guild_only, rename = "update", category = "Scheduling")]
pub(crate) async fn update_message(
    ctx: CommandContext<'_>,
    #[description = "The numeric ID of the message to delete"] message_id: i64,
    #[description = "The title of the message"] title: Option<String>,
    #[description = "The message to send"] message: Option<String>,
    #[description = "When to send the message (format: yyyy-MM-dd hh:mm:ss)"] datetime: Option<
        String,
    >,
    #[description = "How often to repeat, in minutes (m), hours (h), days (d), weeks (w), or years (y)"]
    repeat: Option<String>,
    #[description = "The channel to send the message to when it's time to be sent"]
    #[channel_types("NewsThread", "PrivateThread", "PublicThread", "Text")]
    channel: Option<GuildChannel>,
) -> CommandResult<()> {
    const REPLY_TITLE: &str = "Update scheduled message";
    let author = ctx.author();

    match (&title, &message, &datetime, &repeat, &channel) {
        (None, None, None, None, None) => {
            whisper(&ctx, REPLY_TITLE, "No message properties to update have been supplied.")
                .await?;

            Ok(())
        },
        _ => {
            let data = ctx.data();
            match get_scheduled_message(&data.database, message_id).await? {
                Some(existing_message) if existing_message.user_id() == author.id => {
                    // Validate fields that need validating; the inner method will take care of updates or not, depending on
                    // whether the optional parameters are supplied or not.

                    let mut parsed_datetime = None;
                    if let Some(d) = &datetime {
                        // Check the datetime parses successfully
                        parsed_datetime = Some(parse_datetime_to_utc(&data.database, d, author.id).await?);
                    }

                    if let Some(r) = &repeat {
                        // Check the repeat is valid and can be applied to the scheduled time successfully.
                        let dt = match parsed_datetime {
                            Some(d) => d,
                            None => match DateTime::parse_from_rfc3339(&existing_message.datetime) {
                                Ok(dt) => dt.to_utc(),
                                Err(e) => return Err(CommandError::detailed("Error parsing already stored datetime!", e)),
                            },
                        };

                        apply_repeat_duration(r, dt)?;
                    }

                    let channel_id = channel.map(|c| c.id.get());

                    match update_scheduled_message(&data.database, message_id, parsed_datetime, repeat, title, message, channel_id).await {
                        Ok(true) => {
                            reply(&ctx, REPLY_TITLE, "Scheduled message updated successfully.").await?;
                            Ok(())
                        }
                        Ok(false) => Err(CommandError::new("Could not find or update scheduled message.")),
                        Err(e) => Err(CommandError::detailed("Error updating scheduled message.", e)),
                    }
                },
                _ => {
                    Err(CommandError::new(format!(
                        "Unable to find message with id {}",
                        message_id
                    )))
                },
            }
        }
    }
}

/// Delete a scheduled message
#[poise::command(slash_command, guild_only, rename = "remove", category = "Scheduling")]
pub(crate) async fn remove_message(
    ctx: CommandContext<'_>,
    #[description = "The numeric ID of the message to delete"] message_id: i64,
) -> CommandResult<()> {
    const REPLY_TITLE: &str = "Remove scheduled message";

    let data = ctx.data();
    let author = ctx.author();

    match get_scheduled_message(&data.database, message_id).await? {
        Some(message) if message.user_id() == author.id => {
            match delete_scheduled_message(&data.database, message_id).await {
                Ok(true) => {
                    reply(&ctx, REPLY_TITLE, "Scheduled message deleted successfully.").await?
                },
                Ok(false) => {
                    reply_error(
                        &ctx,
                        REPLY_TITLE,
                        "Scheduled message was not found or could not be deleted.",
                    )
                    .await?
                },
                Err(e) => {
                    return Err(CommandError::detailed("Error deleting scheduled message", e))
                },
            };
        },
        _ => {
            return Err(CommandError::new(format!(
                "Could not find a message with the ID {}",
                message_id
            )))
        },
    };

    Ok(())
}

/// Add a new scheduled message
#[poise::command(slash_command, guild_only, rename = "add", category = "Scheduling")]
pub(crate) async fn add_message(
    ctx: CommandContext<'_>,
    #[description = "The title of the message"] title: String,
    #[description = "The message to send"] message: String,
    #[description = "When to send the message (format: yyyy-MM-dd hh:mm:ss)"] datetime: String,
    #[description = "How often to repeat, in minutes (m), hours (h), days (d), weeks (w), or years (y)"]
    repeat: Option<String>,
    #[description = "The channel to send the message to when it's time to be sent"]
    #[channel_types("NewsThread", "PrivateThread", "PublicThread", "Text")]
    channel: GuildChannel,
) -> CommandResult<()> {
    let data = ctx.data();
    let author = ctx.author();

    let target_datetime = parse_datetime_to_utc(&data.database, &datetime, author.id).await?;

    // If a repeat was specified, verify that adding it to the target datetime won't cause an error.
    if let Some(repeat) = &repeat {
        apply_repeat_duration(repeat, target_datetime)?;
    }

    let repeat = repeat.unwrap_or_else(|| "None".to_owned());
    let success = add_scheduled_message(
        &data.database,
        author.id,
        target_datetime,
        &repeat,
        &title,
        &message,
        channel.id,
    )
    .await?;

    if success {
        reply(
            &ctx,
            "Added scheduled message successfully",
            &format_scheduled_message(&title, &message, &datetime, Some(&repeat), channel.id),
        )
        .await?;
    } else {
        whisper_error(
            &ctx,
            "Failed to add scheduled message",
            "Scheduled message was not added to the database, but no error was encountered.",
        )
        .await?;
    }

    Ok(())
}

fn apply_repeat_duration(
    repeat: &str,
    current_datetime: DateTime<Utc>,
) -> anyhow::Result<DateTime<Utc>> {
    if repeat.is_empty() {
        return Err(anyhow!("The repeat duration is empty."));
    }

    let mut new_datetime = current_datetime;

    // If this fails, this function is useless anyway and we need to rewrite the regex.
    let regex = Regex::new("([0-9]+)([hmsdwmy])").unwrap();
    let mut unrecognised = Vec::new();
    let mut time_delta = TimeDelta::seconds(0);

    let repeat_lower = repeat.to_lowercase();
    for token in repeat_lower.split_whitespace() {
        match regex.captures(token) {
            Some(captures) => {
                // If this matches, there has to be a group 0 and 1, and group 0 has to contain all numbers, so these unwraps are safe.
                let number: u64 = captures.get(0).unwrap().as_str().parse().unwrap();
                let time_period = captures.get(1).unwrap().as_str();

                let changed_delta = match time_period {
                    "h" => time_delta.checked_add(&TimeDelta::hours(number as i64)),
                    "m" => time_delta.checked_add(&TimeDelta::minutes(number as i64)),
                    "s" => time_delta.checked_add(&TimeDelta::seconds(number as i64)),
                    _ => None,
                };

                if let Some(delta) = changed_delta {
                    time_delta = delta;
                } else {
                    let changed_datetime = match time_period {
                        "y" => new_datetime.checked_add_months(Months::new(12 * number as u32)),
                        "d" => new_datetime.checked_add_days(Days::new(number)),
                        "w" => new_datetime.checked_add_days(Days::new(number * 7)),
                        _ => None,
                    };

                    if let Some(dt) = changed_datetime {
                        new_datetime = dt;
                    } else {
                        unrecognised.push(token);
                    }
                }
            },
            None => unrecognised.push(token),
        }
    }

    if let Some(dt) = new_datetime.checked_add_signed(time_delta) {
        new_datetime = dt;
    } else {
        return Err(anyhow!("Total parsed time delta was {}, which did not produce a valid datetime when added to {}", time_delta, new_datetime));
    }

    if unrecognised.is_empty() {
        Ok(new_datetime)
    } else {
        Err(anyhow!("Unrecognised tokens in repeat duration: {}", unrecognised.join(", ")))
    }
}

/// Set the timezone used for all messages scheduled by you.
#[poise::command(slash_command, guild_only, rename = "timezone", category = "Scheduling")]
pub(crate) async fn set_timezone(
    ctx: CommandContext<'_>,
    #[description = "The timezone identifier"] id: TimeZoneParameter,
) -> CommandResult<()> {
    const REPLY_TITLE: &str = "User timezone";
    let result =
        update_user_setting(&ctx.data().database, ctx.author().id, USER_TIMEZONE, id.name()).await;

    let mut message = MessageBuilder::new();
    match result {
        Ok(true) => {
            message.push_line(format!("Your timezone has been set to {}.", id.name()));
        },
        Ok(false) => {
            message.push_line(format!("Your timezone was already set to {}.", id.name()));
        },
        Err(e) => {
            return Err(CommandError::detailed("Error updating timezone setting", e));
        },
    }

    whisper(&ctx, REPLY_TITLE, &message.build()).await?;

    Ok(())
}

/// Format a single scheduled message for display.
fn format_scheduled_message(
    title: &str,
    message: &str,
    datetime: &str,
    repeat: Option<&str>,
    channel: ChannelId,
) -> String {
    let mut content = MessageBuilder::new();
    content
        .push_bold("Datetime: ")
        .push_line(datetime)
        .push_bold("Repeat: ")
        .push_line(repeat.unwrap_or("None"))
        .push_bold("Channel: ")
        .mention(&channel)
        .push_line("")
        .push_bold("Title: ")
        .push_line(title)
        .push_bold_line("Message:")
        .push_line(truncate_string(message, 500));

    content.build()
}

/// Parse a string into a valid datetime.
async fn parse_datetime_to_utc(database: &Database, datetime: &str, user_id: UserId) -> anyhow::Result<DateTime<Utc>> {
    let parsed_datetime = match NaiveDateTime::parse_from_str(datetime, "%Y-%m-%d %H:%M:%S") {
        Ok(val) => val,
        Err(e) => return Err(CommandError::detailed("Error parsing input datetime", e).into()),
    };
    let user_timezone = get_user_setting(database, user_id, USER_TIMEZONE).await?
        .map(|opt| chrono_tz::Tz::from_str(&opt.value).unwrap_or(chrono_tz::Tz::UTC))
        .unwrap_or(chrono_tz::Tz::UTC);

    match parsed_datetime.and_local_timezone(user_timezone).earliest() {
        Some(dt) => Ok(dt.to_utc()),
        None => Err(CommandError::new(format!(
            "Could not construct a local datetime for {} in timezone {}",
            parsed_datetime, user_timezone
        )).into()),
    }
}

/// Validate datetime is current or future
async fn validate_datetime(datetime: DateTime<Utc>) -> bool {
    let current_time = chrono::offset::Utc::now();
    datetime > current_time
}
