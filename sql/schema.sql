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

ALTER TABLE todos ADD COLUMN IF NOT EXISTS category varchar(100);
