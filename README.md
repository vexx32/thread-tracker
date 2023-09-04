# Thread Tracker

A simple Discord bot built in Rust with [Serenity](https://docs.rs/serenity/latest/serenity/) and kept running with [Shuttle.rs](https://shuttle.rs).

## Get Started

- [Top.gg](https://top.gg/bot/385572136082735106)
- [Invite Thread Tracker to your server](https://discord.com/api/oauth2/authorize?client_id=385572136082735106&permissions=84992&scope=bot)

## Usage

Use `/tt_help` to have Thread Tracker list its available commands.

## Data Stored

Thread Tracker stores only the relevant Discord user ID, server ID, and thread or channel IDs.
To Do entries store the relevant text, and some thread and message information will be cached in memory, but not stored permanently.

## Permissions

Thread Tracker requires very few permissions:

- Read Messages
- Send Messages
- View Message History
- Embed Links
