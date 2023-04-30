Titi can show a list of all your tracked threads and who last responded to each thread. If the last reply isn't from you or one of your muses (`tt?muses`), the name is shown in bold to indicate it's awaiting a reply.

Thread URLs can be found under `Copy Link` when you right click or long-press on the channel or thread in Discord. If you want to give multiple URLs at once, separate the links with spaces or linebreaks.

Parameters in _`italics`_ are optional.

____Add/Remove____
> **`tt!track`** _`category`_ `URLs` - Track new threads, optionally with a category.
> **`tt!untrack`** `URLs` - Remove tracked threads from your list.
> **`tt!untrack`** `categories` - Remove all tracked threads in the given categories.
> **`tt!untrack all`** - Remove all tracked threads.

____Change Categories____
> **`tt!category`** `category` `URLs` - Change the category of already-tracked threads. Use `unset` or `none` as the category to remove the category.

____List____
> **`tt!threads`** _`categories`_ — List tracked threads and to do-list items. Optionally, provide categories to filter the list.
> **`tt!random`** _`category`_ — Find a random tracked thread that you don't have the last reply in. Optionally, provide a category to filter the choices.
> **`tt!watch`** _`categories`_ — Same as `tt!threads`, but also periodically edits the message to update the generated list.
> **`tt!unwatch`** `URL` — Link a watched message to delete it and stop watching.
> **`tt!watching`** - List currently active watchers.
