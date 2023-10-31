pub(crate) mod help;
pub(crate) mod muses;
pub(crate) mod stats;
pub(crate) mod threads;
pub(crate) mod todos;
pub(crate) mod watchers;

use crate::CommandError;

type CommandResult<T> = std::result::Result<T, CommandError>;

pub(crate) fn list() -> Vec<poise::Command<crate::Data, crate::CommandError>> {
    vec![
        help::help(),
        muses::add(),
        muses::remove(),
        muses::list(),
        stats::send_statistics(),
        threads::add(),
        threads::remove(),
        threads::set_category(),
        threads::send_list(),
        threads::send_random_thread(),
        todos::add(),
        todos::remove(),
        todos::list(),
        // watchers::add(),
        // watchers::remove(),
        // watchers::list(),
    ]
}
