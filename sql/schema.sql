CREATE TABLE IF NOT EXISTS threads (
    id serial PRIMARY KEY,
    user_id BIGINT NOT NULL,
    guild_id BIGINT NOT NULL,
    channel_id BIGINT NOT NULL,
    category varchar(100) NULL
);

CREATE TABLE IF NOT EXISTS watchers (
    id serial PRIMARY KEY,
    user_id BIGINT NOT NULL,
    guild_id BIGINT NOT NULL,
    channel_id BIGINT NOT NULL,
    message_id BIGINT NOT NULL,
    categories varchar(200) NULL
);

CREATE TABLE IF NOT EXISTS muses (
    id serial PRIMARY KEY,
    user_id BIGINT NOT NULL,
    guild_id BIGINT NOT NULL,
    muse_name varchar(100) NOT NULL
);

CREATE TABLE IF NOT EXISTS todos (
    id serial PRIMARY KEY,
    user_id BIGINT NOT NULL,
    guild_id BIGINT NOT NULL,
    content varchar(300) NOT NULL,
    category varchar(100) NULL
);

CREATE TABLE IF NOT EXISTS subscriptions (
    id serial PRIMARY KEY,
    user_id BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS user_settings (
    id serial PRIMARY KEY,
    user_id BIGINT NOT NULL,
    name varchar(300) NOT NULL,
    value varchar(300) NOT NULL
);

CREATE TABLE IF NOT EXISTS scheduled_messages (
    id serial PRIMARY KEY,
    user_id BIGINT NOT NULL,
    channel_id BIGINT NOT NULL,
    datetime varchar(60) NOT NULL,
    repeat varchar(60) NULL,
    title varchar(300) NOT NULL,
    message varchar(2000) NOT NULL,
    archived BOOLEAN NOT NULL
);

CREATE TABLE IF NOT EXISTS server_nicknames (
    id serial PRIMARY KEY,
    user_id BIGINT NOT NULL,
    guild_id BIGINT NOT NULL,
    nickname varchar(300) NOT NULL
);
