use std::{collections::HashMap, str::FromStr};

use anyhow::anyhow;
use chrono::{NaiveDate, NaiveDateTime, DateTime, TimeDelta, Months, Days};
use chrono_tz::Tz;
use poise::choice_parameter::ChoiceParameter;
use regex::Regex;
use serenity::{
    http::{CacheHttp, Http},
    model::prelude::*,
    prelude::*,
    utils::{ContentModifier::*, EmbedMessageBuilding, MessageBuilder},
};
use tracing::{error, info};

use crate::{
    commands::{CommandContext, CommandResult, CommandError},
    consts::setting_names::*,
    messaging::{send_invalid_command_call_error, whisper, whisper_error, reply}, db::{update_user_setting, get_user_setting, add_scheduled_message},
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
    subcommands("add_message", "set_timezone")
)]
pub(crate) async fn schedule(ctx: CommandContext<'_>) -> CommandResult<()> {
    send_invalid_command_call_error(ctx).await
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
    #[description = "How often to repeat, in minutes (m), hours (h), days (d), weeks (w), or years (y)"]
    repeat: Option<String>,
    #[description = "The channel to send the message to when it's time to be sent"]
    #[channel_types("NewsThread", "PrivateThread", "PublicThread", "Text")]
    channel: GuildChannel,
) -> CommandResult<()> {
    let data = ctx.data();
    let author = ctx.author();

    let parsed_datetime = match NaiveDateTime::parse_from_str(&datetime, "%Y-%m-%d %H:%M:%S") {
        Ok(val) => val,
        Err(e) => return Err(CommandError::detailed("Error parsing input datetime", e)),
    };
    let user_timezone = get_user_setting(&data.database, author.id, USER_TIMEZONE).await?
        .map(|opt| chrono_tz::Tz::from_str(&opt.value).unwrap_or(chrono_tz::Tz::UTC))
        .unwrap_or(chrono_tz::Tz::UTC);

    let target_datetime = match parsed_datetime.and_local_timezone(user_timezone).earliest() {
        Some(dt) => dt,
        None => return Err(CommandError::new(format!("Could not create a local datetime for {} in timezone {}", parsed_datetime, user_timezone))),
    };

    // If a repeat was specified, verify that adding it to the target datetime won't cause an error.
    if let Some(repeat) = &repeat {
        let _ = apply_repeat_duration(repeat, target_datetime)?;
    }

    let repeat = repeat.unwrap_or_else(|| "None".to_owned());
    let success = add_scheduled_message(
        &data.database,
        author.id,
        &datetime,
        &repeat,
        &title,
        &message,
        channel.id
    ).await?;

    if success {
        reply(
            &ctx,
            "Added scheduled message successfully",
            &format!("Added scheduled message for '{}' with repeat '{}', title:\n{}", datetime, repeat, title)
        ).await?;
    }
    else {
        whisper_error(&ctx, "Failed to add scheduled message", "Scheduled message was not added to the database, but no error was encountered.").await?;
    }

    Ok(())
}

fn apply_repeat_duration(repeat: &str, current_datetime: DateTime<Tz>) -> anyhow::Result<DateTime<Tz>> {
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
                }
                else {
                    let changed_datetime = match time_period {
                        "y" => new_datetime.checked_add_months(Months::new(12 * number as u32)),
                        "d" => new_datetime.checked_add_days(Days::new(number)),
                        "w" => new_datetime.checked_add_days(Days::new(number * 7)),
                        _ => None,
                    };

                    if let Some(dt) = changed_datetime {
                        new_datetime = dt;
                    }
                    else {
                        unrecognised.push(token);
                    }
                }
            },
            None => unrecognised.push(token),
        }
    }

    if let Some(dt) = new_datetime.checked_add_signed(time_delta) {
        new_datetime = dt;
    }
    else {
        return Err(anyhow!("Total parsed time delta was {}, which did not produce a valid datetime when added to {}", time_delta, new_datetime));
    }

    if unrecognised.is_empty() {
        Ok(new_datetime)
    }
    else {
        Err(anyhow!("Unrecognised tokens in repeat duration: {}", unrecognised.join(", ")))
    }
}

/// Set the timezone used for all messages scheduled by you.
#[poise::command(slash_command, guild_only, rename = "timezone", category = "Scheduling")]
pub(crate) async fn set_timezone(
    ctx: CommandContext<'_>,
    #[description = "The timezone identifier"]
    id: TimeZoneParameter,
) -> CommandResult<()> {
    const REPLY_TITLE: &str = "User timezone";
    let result = update_user_setting(
        &ctx.data().database,
        ctx.author().id,
        USER_TIMEZONE,
        id.name())
        .await;

    let mut message = MessageBuilder::new();
    match result {
        Ok(true) => {
            message.push_line(format!("Your timezone has been set to {}.", id.name()));
        },
        Ok(false) => {
            message.push_line(format!("Your timezone was already set to {}.", id.name()));
        }
        Err(e) => {
            return Err(anyhow!("Error updating timezone setting: {}", e).into());
        }
    }

    whisper(&ctx, "Successfully updated timezone", &message.build()).await?;

    Ok(())
}
