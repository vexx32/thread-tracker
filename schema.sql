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
