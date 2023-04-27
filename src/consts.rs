use std::time::Duration;

pub(crate) mod help;

pub(crate) const DELETE_EMOJI: [&str; 2] = ["🚫", "🗑️"];
pub(crate) const DEBUG_USER: u64 = 283711673934807042;

pub(crate) const THREAD_NAME_LENGTH: usize = 32;

pub(crate) const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(295);
pub(crate) const WATCHER_UPDATE_INTERVAL: Duration = Duration::from_secs(600);
pub(crate) const CACHE_TRIM_INTERVAL: Duration = Duration::from_secs(2995);

pub(crate) const CACHE_LIFETIME: Duration = Duration::from_secs(6000);
