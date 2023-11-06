use std::collections::BTreeMap;

use serenity::{
    http::{CacheHttp, Http},
    model::prelude::*,
    prelude::*,
};
use tracing::{error, info};

use crate::db::ThreadWatcher;

/// Wrapper struct to simplify passing around user/guild ID pair.
pub(crate) struct GuildUser {
    pub user_id: UserId,
    pub guild_id: GuildId,
}

impl From<&ThreadWatcher> for GuildUser {
    fn from(value: &ThreadWatcher) -> Self {
        Self { user_id: value.user_id(), guild_id: value.guild_id() }
    }
}

/// Wrapper struct for the MessageId and ChannelId of a Discord message.
#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
pub(crate) struct ChannelMessage {
    pub channel_id: ChannelId,
    pub message_id: MessageId,
}

impl ChannelMessage {
    /// Retrieves the original message from Discord API
    pub async fn fetch(&self, http: impl AsRef<Http>) -> Result<Message, SerenityError> {
        self.channel_id.message(http, self.message_id).await
    }
}

impl From<(MessageId, ChannelId)> for ChannelMessage {
    fn from((message_id, channel_id): (MessageId, ChannelId)) -> Self {
        Self { message_id, channel_id }
    }
}

impl From<(ChannelId, MessageId)> for ChannelMessage {
    fn from((channel_id, message_id): (ChannelId, MessageId)) -> Self {
        Self { channel_id, message_id }
    }
}

/// Returns a `BTreeMap` which maps a derived key from `key_function` to a `Vec<TValue>` which contains the values that produced that key.
///
/// ### Arguments
///
/// - `items` - the initial set of items
/// - `key_function` - the function which produces key values from the input `TValue` items in the input vec
pub(crate) fn partition_into_map<TKey, TValue, F>(
    items: Vec<TValue>,
    key_function: F,
) -> BTreeMap<TKey, Vec<TValue>>
where
    TKey: Ord,
    F: Fn(&TValue) -> TKey,
{
    let mut map: BTreeMap<TKey, Vec<TValue>> = BTreeMap::new();

    for item in items {
        map.entry(key_function(&item)).or_default().push(item);
    }

    map
}

/// If the given string starts with `tt_` or `tt?` (case-insensitive), returns true.
pub(crate) fn message_is_command(content: &str) -> bool {
    let prefix: String = content.chars().take(3).flat_map(|c| c.to_lowercase()).collect();
    prefix == "tt_" || prefix == "tt?"
}

/// Trim the given string to the maximum length in characters.
pub(crate) fn substring(name: &str, max_length: usize) -> &str {
    if name.chars().count() > max_length {
        let (cutoff, _) = name.char_indices().nth(max_length - 1).unwrap();
        name[0..cutoff].trim()
    }
    else {
        name
    }
}

pub(crate) async fn get_channel_name(
    channel_id: ChannelId,
    cache_http: impl CacheHttp,
) -> Option<String> {
    channel_id.to_channel(cache_http.http()).await.map_or(None, |c| c.guild()).map(|gc| gc.name)
}

pub(crate) fn subdivide_string(s: &str, max_chunk_length: usize) -> Vec<&str> {
    let mut result = Vec::with_capacity(s.len() / max_chunk_length);
    let mut iter = s.chars();
    let mut pos = 0;

    while pos < s.len() {
        let mut len = 0;
        for ch in iter.by_ref().take(max_chunk_length) {
            len += ch.len_utf8();
        }
        result.push(&s[pos..pos + len]);
        pos += len;
    }

    result
}

pub(crate) fn split_into_chunks(s: &str, max_chunk_length: usize) -> Vec<String> {
    if s.len() <= max_chunk_length {
        return vec![s.to_owned()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    for line in s.split('\n') {
        if current.len() + line.len() > max_chunk_length {
            if current.is_empty() {
                let fragments = subdivide_string(line, max_chunk_length);
                for fragment in fragments {
                    current.push_str(fragment);

                    if current.len() >= max_chunk_length {
                        chunks.push(current);
                        current = String::new();
                    }
                }
            }
            else {
                chunks.push(current);
                current = line.to_owned();
            }
        }
        else {
            current.push('\n');
            current.push_str(line);
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

pub(crate) async fn delete_message(
    message: &Message,
    context: &impl CacheHttp,
    data: &crate::Data,
) {
    if let Err(e) = message.delete(context).await {
        error!("Unable to delete message with ID {:?}: {}", message.id, e);
    }
    else {
        info!("Message deleted successfully!");
        data.message_cache
            .remove(&ChannelMessage { message_id: message.id, channel_id: message.channel_id })
            .await;
    }
}

pub(crate) async fn register_guild_commands<U, E>(
    commands: &[poise::Command<U, E>],
    guild_id: GuildId,
    ctx: &impl AsRef<Http>,
) {
    let commands = poise::builtins::create_application_commands(commands);
    let result = guild_id
        .set_application_commands(ctx, |cmds| {
            *cmds = commands;
            cmds
        })
        .await;

    if let Err(e) = result {
        error!("Unable to register commands in guild {}: {}", guild_id, e);
    }
}

// pub(crate) fn find_string_option<'a>(
//     args: &'a [CommandDataOption],
//     name: &str,
// ) -> Option<&'a str> {
//     match find_named_option(args, name) {
//         Some(CommandDataOptionValue::String(s)) => Some(s),
//         _ => None,
//     }
// }

// pub(crate) fn find_channel_option<'a>(
//     args: &'a [CommandDataOption],
//     name: &str,
// ) -> Option<&'a PartialChannel> {
//     match find_named_option(args, name) {
//         Some(CommandDataOptionValue::Channel(s)) => Some(s),
//         _ => None,
//     }
// }

// pub(crate) fn find_integer_option(args: &[CommandDataOption], name: &str) -> Option<i64> {
//     match find_named_option(args, name) {
//         Some(&CommandDataOptionValue::Integer(i)) => Some(i),
//         _ => None,
//     }
// }

// fn find_named_option<'a>(
//     args: &'a [CommandDataOption],
//     name: &str,
// ) -> Option<&'a CommandDataOptionValue> {
//     args.iter().find(|opt| opt.name == name).and_then(|opt| opt.resolved.as_ref())
// }
