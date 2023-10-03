pub(crate) mod help;
pub(crate) mod muses;
pub(crate) mod threads;
pub(crate) mod todos;
pub(crate) mod watchers;

use crate::TitiError;

type CommandResult<T> = std::result::Result<T, TitiError>;

pub(crate) fn list() -> Vec<poise::Command<crate::Data, crate::TitiError>> {
    vec![
        help::help(),
        muses::add(),
        muses::remove(),
        muses::list(),
        threads::add(),
        threads::remove(),
        threads::set_category(),
        threads::send_list(),
        threads::send_random_thread(),
    ]
}
