pub(crate) const DELETE_EMOJI: [&str; 2] = ["🚫", "🗑️"];

pub(crate) const HELP_MAIN_TITLE: &str = "Help";
pub(crate) const HELP_MAIN: &str = r#"
Thanks for using Thread Tracker! You can call me Titi. To report bugs or make feature requests, please visit the [Github](https://github.com/vexx32/thread-tracker) page.

**`tt!help`** - Shows this help message. You can use it if you ever have any questions about the current functionality of Thread Tracker.

__**Thread Commands**__
> `tt!threads`, `tt!track`, `tt!untrack`, `tt!category`, `tt!watch`, `tt!unwatch`
> Track your Discord threads and let you know who last responded to them. Use **`tt?threads`** for more information.

__**Muse Commands**__
> `tt!muses`, `tt!addmuse`, `tt!removemuse`
> Register muse names to help Titi determine which replies are yours. Use **`tt?muses`** for more information.

__**To Do Commands**__
> `tt!todos`, `tt!todo`, `tt!done`
> A personal to do list that you can update as needed. Use **`tt?todos`** for more information.

_Titi's responses can be deleted by the user that triggered the request reacting with_ :no_entry_sign: _or_ :wastebasket:
"#;

pub(crate) const HELP_MUSES_TITLE: &str = "View or change registered muses.";
pub(crate) const HELP_MUSES: &str = r#"
When using a bot like [Tupperbox](https://tupperbox.app), which lets you send responses from the bot under specific names, Titi can't normally tell which responses are from you. Registering a muse name lets Titi know which responses are yours.

When listing threads (`tt?threads`), Titi will list the person who last responded to a thread in bold if it isn't you or one of your muses. It'll also ensure that Titi picks threads that you haven't responded to when using `tt!random`.

> **`tt!muses`** — List the currently registered muses
> **`tt!addmuse`** `name` — Register a muse name
> **`tt!removemuse`** `name` — Remove a registered muse name
"#;

pub(crate) const HELP_THREADS_TITLE: &str = "View or change tracked threads";
pub(crate) const HELP_THREADS: &str = r#"
Titi can show a list of all your tracked threads and who last responded to each thread. If the last reply isn't from you or one of your muses (`tt?muses`), the name is shown in bold to indicate it's awaiting a reply.

Thread URLs can be found under `Copy Link` when you right click or long-press on the channel or thread in Discord. If a command takes multiple URLs, separate the links with spaces or linebreaks.

Parameters in _`italics`_ are optional.

__**Add/Remove**__
> **`tt!track`** _`category`_ `URLs` - Track new threads, optionally with a category.
> **`tt!untrack`** `URLs` - Remove tracked threads from your list.
> **`tt!untrack`** `categories` - Remove all tracked threads in the given categories.
> **`tt!untrack all`** - Remove all tracked threads.

__**Change Categories**__
> **`tt!category`** `category` `URLs` - Change the category of already-tracked threads. Use `unset` or `none` as the category to remove the category.

__**List**__
> **`tt!threads`** _`categories`_ — List tracked threads and to do-list items. Optionally, provide categories to filter the list.
> **`tt!random`** _`category`_ — Find a random tracked thread that you don't have the last reply in. Optionally, provide a category to filter the choices.
> **`tt!watch`** _`categories`_ — Same as `tt!threads`, but also periodically edits the message to update the generated list.
> **`tt!unwatch`** `URL` — Link a watched message to delete it and stop watching.
"#;

pub(crate) const HELP_TODOS_TITLE: &str = "View or change to do-list entries.";
pub(crate) const HELP_TODOS: &str = r#"
Titi can help keep track of your to do list! To Do list entries will also be shown when listing threads, and can occupy the same categories as your normal threads.

Note that using categories with to do-list items requires you prefix the category with `!`, for example `!Bob`.

> **`tt!todos`** — List all to do-list entries.
> **`tt!todo`** _`!category`_ `todo text` — Add a to do-list item, optionally with a category.
> **`tt!done`** `todo text` — Remove a to do-list entry.
> **`tt!done`** `!category` — Remove all to do-list entries from the given category.
> **`tt!done !all`** — Remove all to do entries.
"#;
