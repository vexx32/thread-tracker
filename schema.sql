-- DROP TABLE IF EXISTS threads;

CREATE TABLE IF NOT EXISTS threads (
  id serial PRIMARY KEY,
  user_id BIGINT NULL,
  guild_id BIGINT NOT NULL,
  channel_id BIGINT NOT NULL
);
