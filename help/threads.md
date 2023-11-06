Titi can show a list of all your tracked threads and who last responded to each thread. If the last reply isn't from you or one of your muses (`/tt_help muses`), the name is shown in bold to indicate it's awaiting a reply.

Thread tracking commands will present a list of accessible channels/threads for you to pick from. If you don't see the thread you're looking for, make sure it is not archived by reopening the thread or replying to it once.

Parameters in _`italics`_ are optional.

## Add/Remove Threads

> **`/tt_track`** `thread` _`category`_ - Track new threads, optionally with a category.
> **`/tt_untrack`** `thread` - Remove a tracked thread from your list.
> **`/tt_untrack`** `category` - Remove all tracked threads in the given categories. Use `all` as the category to untrack everything.

## Change Categories

> **`/tt_category`** `thread` `category` - Change the category of already-tracked threads. Use `unset` or `none` as the category to remove the category.

## List Threads

> **`/tt_threads`** _`categories`_ — List tracked threads and to do-list items. Optionally, provide categories to filter the list.
> **`/tt_replies`** _`categories`_ — List tracked threads which are awaiting your reply. Optionally, provide categories to filter the list.
> **`/tt_random`** _`category`_ — Find a random tracked thread that you don't have the last reply in. Optionally, provide a category to filter the choices.

## Watchers

> **`/tt_watch`** _`categories`_ — Similar to `tt_threads`, but also periodically edits the message to update the generated list.
> **`/tt_unwatch`** `URL` — Link a watched message to delete it and stop watching.
> **`/tt_watching`** - List currently active watchers.
