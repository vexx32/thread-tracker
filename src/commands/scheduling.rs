use std::str::FromStr;

use anyhow::anyhow;
use chrono::{DateTime, Days, FixedOffset, Months, NaiveDateTime, TimeDelta, Utc};
use chrono_tz::Tz;
use regex::Regex;
use serenity::{all::CacheHttp, model::prelude::*, utils::MessageBuilder};
use tracing::{error, info};

use crate::{
    commands::{CommandContext, CommandError, CommandResult},
    consts::setting_names::*,
    db::{self, Database},
    messaging::{reply, reply_error, send_invalid_command_call_error, send_message, whisper, whisper_error},
    utils::truncate_string,
};

/// Manage scheduled messages
#[poise::command(
    slash_command,
    guild_only,
    rename = "tt_schedule",
    category = "Scheduling",
    subcommands(
        "add_message",
        "remove_message",
        "update_message",
        "list_messages",
        "get_message",
        "set_timezone"
    )
)]
pub(crate) async fn schedule(ctx: CommandContext<'_>) -> CommandResult<()> {
    send_invalid_command_call_error(ctx).await
}

#[poise::command(slash_command, guild_only, rename = "list", category = "Scheduling")]
pub(crate) async fn list_messages(ctx: CommandContext<'_>) -> CommandResult<()> {
    const REPLY_TITLE: &str = "List scheduled messages";
    let data = ctx.data();
    let author = ctx.author();
    let messages = db::list_scheduled_messages_for_user(&data.database, author.id).await?;

    info!("Listing all scheduled messages for user {} ({})", author.name, author.id);

    if messages.is_empty() {
        reply(&ctx, REPLY_TITLE, "You have no scheduled messages.").await?;
    } else {
        let mut content = MessageBuilder::new();
        for msg in messages {
            let local_datetime = parse_and_display_local_time(&msg.datetime, author.id, &data.database).await?;
            content
                .push("- ")
                .push_bold(msg.id.to_string())
                .push(": ")
                .push(&msg.title)
                .push(" in ")
                .mention(&msg.channel_id())
                .push(" @ ")
                .push(&local_datetime);

            if !msg.repeat.is_empty() && msg.repeat != "None" {
                content.push(" (every ").push(msg.repeat).push(")");
            }

            content.push_line("");
        }

        reply(&ctx, REPLY_TITLE, &content.build()).await?;
    }

    Ok(())
}


/// Get a scheduled message
#[poise::command(slash_command, guild_only, rename = "get", category = "Scheduling")]
pub(crate) async fn get_message(
    ctx: CommandContext<'_>,
    #[description = "The numeric ID of the message to retrieve"]
    message_id: i32,
) -> CommandResult<()> {
    let data = ctx.data();
    let author = ctx.author();

    let message = match db::get_scheduled_message(&data.database, message_id).await? {
        Some(msg) if msg.user_id() == author.id => { msg },
        _ => return Err(CommandError::new(format!("Unable to find the message with id {}", message_id))),
    };

    let local_datetime = parse_and_display_local_time(&message.datetime, author.id, &data.database).await?;

    let response = format_scheduled_message(
        Some(message.id),
        &message.title,
        &local_datetime,
        &message.datetime,
        Some(&message.repeat),
        message.channel_id());

    reply(&ctx, "Get scheduled message information", &response).await?;

    Ok(())
}

/// Update an existing scheduled message
#[poise::command(slash_command, guild_only, rename = "update", category = "Scheduling")]
pub(crate) async fn update_message(
    ctx: CommandContext<'_>,
    #[description = "The numeric ID of the message to delete"] message_id: i32,
    #[description = "The title of the message"] title: Option<String>,
    #[description = "The message to send"] message: Option<String>,
    #[description = "When to send the message (format: yyyy-MM-dd hh:mm:ss)"] datetime: Option<String>,
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
            match db::get_scheduled_message(&data.database, message_id).await? {
                Some(existing_message) if existing_message.user_id() == author.id => {
                    // Validate fields that need validating; the inner method will take care of updates or not, depending on
                    // whether the optional parameters are supplied or not.

                    let mut parsed_datetime = None;
                    if let Some(d) = &datetime {
                        // Check the datetime parses successfully and is actually in the future
                        let dt = parse_datetime_to_utc(&data.database, d, author.id).await?;
                        if !validate_datetime(dt) {
                            return Err(CommandError::new(format!(
                                "The target datetime {} is invalid as it is not in the future.",
                                dt.to_rfc3339()
                            )));
                        }

                        parsed_datetime = Some(dt);
                    }

                    if let Some(r) = &repeat {
                        // Check the repeat is valid and can be applied to the scheduled time successfully.
                        let dt = match parsed_datetime {
                            Some(d) => d,
                            None => {
                                match DateTime::parse_from_rfc3339(&existing_message.datetime) {
                                    Ok(dt) => dt.to_utc(),
                                    Err(e) => {
                                        return Err(CommandError::detailed(
                                            "Error parsing already stored datetime!",
                                            e,
                                        ))
                                    },
                                }
                            },
                        };

                        apply_repeat_duration(r, dt)?;
                    }

                    let channel_id = channel.map(|c| c.id.get());

                    match db::update_scheduled_message(
                        &data.database,
                        message_id,
                        parsed_datetime,
                        repeat,
                        title,
                        message,
                        channel_id,
                    )
                    .await
                    {
                        Ok(true) => {
                            reply(&ctx, REPLY_TITLE, "Scheduled message updated successfully.")
                                .await?;
                            Ok(())
                        },
                        Ok(false) => {
                            Err(CommandError::new("Could not find or update scheduled message."))
                        },
                        Err(e) => {
                            Err(CommandError::detailed("Error updating scheduled message.", e))
                        },
                    }
                },
                _ => {
                    Err(CommandError::new(format!("Unable to find message with id {}", message_id)))
                },
            }
        },
    }
}

/// Delete a scheduled message
#[poise::command(slash_command, guild_only, rename = "remove", category = "Scheduling")]
pub(crate) async fn remove_message(
    ctx: CommandContext<'_>,
    #[description = "The numeric ID of the message to delete"]
    message_id: i32,
) -> CommandResult<()> {
    const REPLY_TITLE: &str = "Remove scheduled message";

    let data = ctx.data();
    let author = ctx.author();

    match db::get_scheduled_message(&data.database, message_id).await? {
        Some(message) if message.user_id() == author.id => {
            match db::delete_scheduled_message(&data.database, message_id).await {
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
    #[description = "The title of the message"]
    title: String,
    #[description = "The message to send"]
    message: String,
    #[description = "When to send the message (format: yyyy-MM-dd hh:mm:ss)"]
    datetime: String,
    #[description = "The channel to send the message to when it's time to be sent"]
    #[channel_types("NewsThread", "PrivateThread", "PublicThread", "Text")]
    channel: GuildChannel,
    #[description = "How often to repeat, in minutes (m), hours (h), days (d), weeks (w), or years (y)"]
    repeat: Option<String>,
) -> CommandResult<()> {
    let data = ctx.data();
    let author = ctx.author();

    let target_datetime = parse_datetime_to_utc(&data.database, &datetime, author.id).await?;

    if !validate_datetime(target_datetime) {
        return Err(CommandError::new(format!(
            "The target datetime {} is invalid as it is not in the future.",
            target_datetime.to_rfc3339()
        )));
    }

    // If a repeat was specified, verify that adding it to the target datetime won't cause an error.
    if let Some(repeat) = &repeat {
        apply_repeat_duration(repeat, target_datetime)?;
    }

    let repeat = repeat.unwrap_or_else(|| "None".to_owned());
    let success = db::add_scheduled_message(
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
        let local_datetime = display_as_local_time(target_datetime.fixed_offset(), author.id, &data.database).await?;
        reply(
            &ctx,
            "Added scheduled message successfully",
            &format_scheduled_message(None, &title, &message, &local_datetime, Some(&repeat), channel.id),
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

/// Set the timezone used for all messages scheduled by you.
#[poise::command(slash_command, guild_only, rename = "timezone", category = "Scheduling")]
pub(crate) async fn set_timezone(
    ctx: CommandContext<'_>,
    #[description = "The timezone identifier, for example 'Australia/Sydney'"]
    name: String,
) -> CommandResult<()> {
    const REPLY_TITLE: &str = "User timezone";
    let timezone = match get_timezone(&name) {
        Some(tz) => tz,
        None => return Err(CommandError::new(format!("Unknown timezone '{}'", name))),
    };

    let result =
        db::update_user_setting(&ctx.data().database, ctx.author().id, USER_TIMEZONE, timezone.name()).await;

    let mut message = MessageBuilder::new();
    match result {
        Ok(true) => {
            message.push_line(format!("Your timezone has been set to {}.", timezone.name()));
        },
        Ok(false) => {
            message.push_line(format!("Your timezone was already set to {}.", timezone.name()));
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
    id: Option<i32>,
    title: &str,
    message: &str,
    datetime: &str,
    repeat: Option<&str>,
    channel: ChannelId,
) -> String {
    let mut content = MessageBuilder::new();
    if let Some(id) = id {
        content.push_bold("Id: ").push_line(id.to_string());
    }

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

/// Parse an RFC3339 datetime string and return a local time equivalent in RFC2822 format.
async fn parse_and_display_local_time(datetime: &str, user_id: UserId, database: &Database) -> CommandResult<String> {
    let parsed_datetime = match DateTime::parse_from_rfc3339(datetime) {
        Ok(dt) => dt,
        Err(e) => return Err(CommandError::detailed("Error parsing stored message datetime", e)),
    };

    display_as_local_time(parsed_datetime, user_id, database).await
}

/// Convert a datetime to the user's local timezone and format it for display using RFC2822 standards.
async fn display_as_local_time(datetime: DateTime<FixedOffset>, user_id: UserId, database: &Database) -> CommandResult<String> {
    let timezone = get_user_timezone(database, user_id).await?;
    let local_time = datetime.with_timezone(&timezone);

    Ok(local_time.to_rfc2822())
}

/// Parse a string into a valid UTC datetime.
async fn parse_datetime_to_utc(
    database: &Database,
    datetime: &str,
    user_id: UserId,
) -> anyhow::Result<DateTime<Utc>> {
    let parsed_datetime = match NaiveDateTime::parse_from_str(datetime, "%Y-%m-%d %H:%M:%S") {
        Ok(val) => val,
        Err(e) => return Err(CommandError::detailed("Error parsing input datetime", e).into()),
    };
    let user_timezone = get_user_timezone(database, user_id).await?;

    match parsed_datetime.and_local_timezone(user_timezone).earliest() {
        Some(dt) => Ok(dt.to_utc()),
        None => Err(CommandError::new(format!(
            "Could not construct a local datetime for {} in timezone {}",
            parsed_datetime, user_timezone
        ))
        .into()),
    }
}

/// Get the currently set timezone for the user, or UTC if none is set.
async fn get_user_timezone(database: &Database, user_id: UserId) -> db::Result<Tz> {
    Ok(db::get_user_setting(database, user_id, USER_TIMEZONE)
        .await?
        .map(|opt| chrono_tz::Tz::from_str(&opt.value).unwrap_or(chrono_tz::Tz::UTC))
        .unwrap_or(chrono_tz::Tz::UTC))
}

/// Apply the given repeat duration to the current datetime and return the resulting datetime.
pub(crate) fn apply_repeat_duration(
    repeat: &str,
    current_datetime: DateTime<Utc>,
) -> anyhow::Result<DateTime<Utc>> {
    if repeat.is_empty() {
        return Err(anyhow!("The repeat duration is empty."));
    }

    let mut new_datetime = current_datetime;

    // If this fails, this function is useless anyway and we need to rewrite the regex.
    let regex = Regex::new("([0-9]+)([a-zA-Z]+)").unwrap();
    let mut unrecognised = Vec::new();
    let mut time_delta = TimeDelta::seconds(0);

    for token in repeat.split_whitespace() {
        match regex.captures(token) {
            Some(captures) => {
                // If this matches, there has to be a group 0 and 1, and group 0 has to contain all numbers, so these unwraps are safe.
                let number: u64 = captures.get(1).unwrap().as_str().parse().unwrap();
                let time_period = captures.get(2).unwrap().as_str();

                let changed_delta = match time_period {
                    "h" | "hr"  | "hrs"  | "hour"   | "hours" => time_delta.checked_add(&TimeDelta::hours(number as i64)),
                    "m" | "min" | "mins" | "minute" | "minutes" => time_delta.checked_add(&TimeDelta::minutes(number as i64)),
                    "s" | "sec" | "secs" | "second" | "seconds" => time_delta.checked_add(&TimeDelta::seconds(number as i64)),
                    _ => None,
                };

                if let Some(delta) = changed_delta {
                    time_delta = delta;
                } else {
                    let changed_datetime = match time_period {
                        "y" | "yr" | "year" | "yrs"   | "years" => new_datetime.checked_add_months(Months::new(12 * number as u32)),
                        "d" | "dy" | "dys"  | "day"   | "days" => new_datetime.checked_add_days(Days::new(number)),
                        "w" | "wk" | "wks"  | "week"  | "weeks" => new_datetime.checked_add_days(Days::new(number * 7)),
                        "M" | "mo" | "mos"  | "month" | "months" => new_datetime.checked_add_months(Months::new(number as u32)),
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

    // Safeguard to ensure that the new scheduled time is always in the future.
    // This preserves the repeat offets precisely, while also ensuring that
    // we don't end up with a new scheduled time that happens to have already
    // elapsed, for example if the bot has been down for a period of time.
    while new_datetime <= chrono::offset::Utc::now() {
        if let Some(dt) = new_datetime.checked_add_signed(time_delta) {
            new_datetime = dt;
        } else {
            return Err(anyhow!("Total parsed time delta was {}, which did not produce a valid datetime when added to {}", time_delta, new_datetime));
        }
    }

    if unrecognised.is_empty() {
        Ok(new_datetime)
    } else {
        Err(anyhow!("Unrecognised tokens in repeat duration: {}", unrecognised.join(", ")))
    }
}

/// Validate datetime is current or future
fn validate_datetime(datetime: DateTime<Utc>) -> bool {
    let current_time = chrono::offset::Utc::now();
    datetime > current_time
}

/// Archive a scheduled message, flagging it as having been already sent and not to be re-sent again.
pub(crate) async fn archive_scheduled_message(database: &Database, message_id: i32) {
    if let Err(e) = db::archive_scheduled_message(database, message_id).await {
        error!("Unable to flag scheduled message {} as archived: {}", message_id, e);
    }
}

/// Get a timezone value from a given timezone name, for example 'Australia/Sydney'
fn get_timezone(name: &str) -> Option<Tz> {
    chrono_tz::TZ_VARIANTS.iter().find(|&tz| tz.name() == name).cloned()
}

/// Send out any scheduled messages, and re-schedule any repeating ones.
pub(crate) async fn send_scheduled_messages(
    database: Database,
    ctx: impl CacheHttp,
) -> anyhow::Result<()> {
    info!("Sending out any scheduled messages.");

    let messages = db::get_all_scheduled_messages(&database).await?;

    for message in messages.iter().filter(|m| !m.archived) {
        let scheduled_time = match DateTime::parse_from_rfc3339(&message.datetime) {
            Ok(dt) => dt.to_utc(),
            Err(e) => {
                error!(
                    "Error parsing scheduled message's timestamp '{}' with title '{}': {}",
                    &message.datetime, &message.title, e
                );
                continue;
            },
        };

        if scheduled_time > chrono::offset::Utc::now() {
            continue;
        }

        info!(
            "Sending out scheduled message {} with title '{}', scheduled for {}",
            message.id, message.title, message.datetime
        );

        if message.repeat.is_empty() || message.repeat == "None" {
            info!("Flagging message {} as sent/archived.", message.id);
            archive_scheduled_message(&database, message.id).await;
        } else {
            info!("Rescheduling message {} after {}", message.id, message.repeat);

            match apply_repeat_duration(&message.repeat, scheduled_time) {
                Ok(next) => {
                    if let Err(e) = db::update_scheduled_message(
                        &database,
                        message.id,
                        Some(next),
                        None,
                        None,
                        None,
                        None::<u64>,
                    )
                    .await
                    {
                        error!("Unable to re-schedule repeating message: {} -- archiving message as a fallback.", e);
                        archive_scheduled_message(&database, message.id).await;
                    }
                },
                Err(e) => {
                    error!("Unable to re-schedule repeating message: {} -- archiving message as a fallback.", e);
                    archive_scheduled_message(&database, message.id).await;
                },
            };
        }

        if let Err(e) = send_message(
            &ctx,
            message.channel_id(),
            &message.title,
            &message.message,
            Colour::FABLED_PINK,
        ).await
        {
            error!("Unable to send scheduled message, archiving it instead: {}", e);
            archive_scheduled_message(&database, message.id).await;
        }
    }

    Ok(())
}
