use std::collections::BTreeMap;

use serenity::{http::Http, model::prelude::*, prelude::*};

use crate::{messaging::ReplyContext, watchers::ThreadWatcher};

/// Wrapper struct to simplify passing around user/guild ID pair.
pub(crate) struct GuildUser {
    pub user_id: UserId,
    pub guild_id: GuildId,
}

impl From<&EventData> for GuildUser {
    fn from(value: &EventData) -> Self {
        Self { user_id: value.user.id, guild_id: value.guild_id }
    }
}

impl From<&ThreadWatcher> for GuildUser {
    fn from(value: &ThreadWatcher) -> Self {
        Self { user_id: value.user_id, guild_id: value.guild_id }
    }
}

/// Metadata from the received message event.
pub(crate) struct EventData {
    pub user: User,
    pub guild_id: GuildId,
    pub channel_id: ChannelId,
    pub message_id: MessageId,
    pub context: Context,
}

impl EventData {
    /// Get the Http from the event context.
    pub fn http(&self) -> &Http {
        &self.context.http
    }

    /// Get a ReplyContext from the event data.
    pub fn reply_context(&self) -> ReplyContext {
        self.into()
    }

    /// Get the associated GuildUser for this event.
    pub fn user(&self) -> GuildUser {
        self.into()
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

/// If the given string starts with `tt!` or `tt?` (case-insensitive), returns true.
pub(crate) fn message_is_command(content: &str) -> bool {
    let prefix: String = content.chars().take(3).flat_map(|c| c.to_lowercase()).collect();
    prefix == "tt!" || prefix == "tt?"
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
