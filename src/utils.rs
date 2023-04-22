use std::collections::BTreeMap;
use serenity::{
    http::Http,
    model::prelude::*,
    prelude::*,
};

use crate::{
    messaging::ReplyContext,
    CommandError::{self, *}, watchers::ThreadWatcher
};

/// Wrapper struct to simplify passing around user/guild ID pair.
pub(crate) struct GuildUser {
    pub user_id: UserId,
    pub guild_id: GuildId,
}

impl From<&EventData> for GuildUser {
    fn from(value: &EventData) -> Self {
        Self {
            user_id: value.user_id,
            guild_id: value.guild_id,
        }
    }
}

impl From<&ThreadWatcher> for GuildUser {
    fn from(value: &ThreadWatcher) -> Self {
        Self {
            user_id: value.user_id,
            guild_id: value.guild_id,
        }
    }
}

/// Metadata from the received message event.
pub(crate) struct EventData {
    pub user_id: UserId,
    pub guild_id: GuildId,
    pub channel_id: ChannelId,
    pub context: Context,
}

impl EventData {
    pub fn http(&self) -> &Http {
        &self.context.http
    }

    pub fn reply_context(&self) -> ReplyContext {
        self.into()
    }

    pub fn user(&self) -> GuildUser {
        self.into()
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
pub(crate) struct ChannelMessage {
    pub channel_id: ChannelId,
    pub message_id: MessageId,
}

impl ChannelMessage {
    /// Retrieves the message from Discord API
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
pub(crate) fn partition_into_map<TKey, TValue, F>(items: Vec<TValue>, key_function: F) -> BTreeMap<TKey, Vec<TValue>>
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

/// Returns `Err` if `unrecognised_args` is not empty.
pub(crate) fn error_on_additional_arguments(unrecognised_args: Vec<&str>) -> Result<(), CommandError> {
    if !unrecognised_args.is_empty() {
        return Err(UnrecognisedArguments(unrecognised_args.join(", ")));
    }

    Ok(())
}
