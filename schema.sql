CREATE TABLE IF NOT EXISTS threads (
  id serial PRIMARY KEY,
  user_id BIGINT NOT NULL,
  guild_id BIGINT NOT NULL,
  channel_id BIGINT NOT NULL,
  category varchar(100) NULL
);
