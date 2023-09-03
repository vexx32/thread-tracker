use std::time::Duration;

use serenity::model::prelude::ChannelType;

pub(crate) mod help;

pub(crate) const DELETE_EMOJI: [&str; 2] = ["🚫", "🗑️"];
pub(crate) const DEBUG_USER: u64 = 283711673934807042;

pub(crate) const THREAD_NAME_LENGTH: usize = 32;

pub(crate) const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(295);
pub(crate) const WATCHER_UPDATE_INTERVAL: Duration = Duration::from_secs(900);
pub(crate) const CACHE_TRIM_INTERVAL: Duration = Duration::from_secs(2995);

pub(crate) const CACHE_LIFETIME: Duration = Duration::from_secs(6000);

pub(crate) const MAX_WATCHER_UPDATE_TASKS: usize = 3;

pub(crate) const TRACKABLE_CHANNEL_TYPES: [ChannelType; 4] = [ChannelType::NewsThread, ChannelType::PrivateThread, ChannelType::PublicThread, ChannelType::Text];

pub(crate) const MAX_EMBED_CHARS: usize = 2048;

//pub(crate) const MAX_EMBEDS_PER_MESSAGE: usize = 10;
