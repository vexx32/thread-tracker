Titi can show a list of all your tracked threads and who last responded to each thread. If the last reply isn't from you or one of your muses (`/tt_help muses`), the name is shown in bold to indicate it's awaiting a reply.

Thread URLs can be found under `Copy Link` when you right click or long-press on the channel or thread in Discord. If you want to give multiple URLs at once, separate the links with spaces or linebreaks.

Parameters in _`italics`_ are optional.

__**Add/Remove**__
> **`/tt_track`** `channel` _`category`_ - Track new threads, optionally with a category.
> **`/tt_untrack`** `channel` - Remove a tracked thread from your list.
> **`/tt_untrack`** `category` - Remove all tracked threads in the given categories. Use `all` as the category to untrack everything.

__**Change Categories**__
> **`/tt_category`** `category` `URLs` - Change the category of already-tracked threads. Use `unset` or `none` as the category to remove the category.

__**List**__
> **`/tt_replies`** _`categories`_ — List tracked threads and to do-list items. Optionally, provide categories to filter the list.
> **`/tt_random`** _`category`_ — Find a random tracked thread that you don't have the last reply in. Optionally, provide a category to filter the choices.

__**Watch**__
> **`/tt_watch`** _`categories`_ — Similar to `tt_replies`, but also periodically edits the message to update the generated list.
> **`/tt_unwatch`** `URL` — Link a watched message to delete it and stop watching.
> **`/tt_watching`** - List currently active watchers.
