use std::time::Duration;

pub(crate) mod greetings;
pub(crate) mod help;
pub(crate) mod setting_names;

pub(crate) const DELETE_EMOJI: [&str; 2] = ["🚫", "🗑️"];

//pub(crate) const DEBUG_USER: u64 = 283711673934807042;

pub(crate) const THREAD_NAME_LENGTH: usize = 32;

#[cfg(not(debug_assertions))]
pub(crate) const SHARD_CHECKUP_INTERVAL: Duration = Duration::from_secs(300);

#[cfg(debug_assertions)]
pub(crate) const SHARD_CHECKUP_INTERVAL: Duration = Duration::from_secs(30);

pub(crate) const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(295);

#[cfg(not(debug_assertions))]
pub(crate) const WATCHER_UPDATE_INTERVAL: Duration = Duration::from_secs(900);
#[cfg(debug_assertions)]
pub(crate) const WATCHER_UPDATE_INTERVAL: Duration = Duration::from_secs(60);

pub(crate) const SCHEDULED_MESSAGE_INTERVAL: Duration = Duration::from_secs(300);

pub(crate) const CACHE_TRIM_INTERVAL: Duration = Duration::from_secs(2995);

pub(crate) const CACHE_LIFETIME: Duration = Duration::from_secs(6000);

pub(crate) const MAX_WATCHER_UPDATE_TASKS: usize = 5;

pub(crate) const MIN_WATCHER_BATCH_SIZE: usize = 10;

pub(crate) const MPSC_BUFFER_SIZE: usize = 32;

pub(crate) const MAX_EMBED_CHARS: usize = 2048;
