Titi can send scheduled messages! Messages can be scheduled as a one-off or repeating message.

One-off messages will still be stored after sending, you will need to manually delete them to get rid of them.
However, you can re-schedule these messages at a later date with `/tt_schedule update`.

Note that if you have not set a timezone setting for yourself, UTC will be assumed.
All message scheduling is handled in UTC; an automatic conversion will be made from your chosen local time zone to UTC when scheduling a message.

See the [List of tz database time zones](https://en.wikipedia.org/wiki/List_of_tz_database_time_zones#List) ("TZ identifier" column) for a list of acceptable timezone identifiers.

Parameters in _`italics`_ are optional.

- **`/tt_schedule list`** - List currently or previously scheduled messages
- **`/tt_schedule add`** `title` `message` `datetime` `channel` _`repeat`_ - Add a new scheduled message
- **`/tt_schedule remove`** `id` - Remove a previously scheduled message
- **`/tt_schedule update`** `id` _`title` `message` `datetime` `channel` `repeat`_ - Update an existing scheduled message
- **`/tt_schedule timezone`** `name` - Set the applicable local timezone for messages you schedule, using a tz database timezone identifier
