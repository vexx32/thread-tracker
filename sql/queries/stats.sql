SELECT
	COUNT(DISTINCT user_id) AS users,
	COUNT(DISTINCT guild_id) - 1 AS servers, -- -1 corrects for the zero-values added in the union below
	(SELECT COUNT(DISTINCT channel_id) FROM threads) AS threads_distinct,
    (SELECT COUNT(*) FROM threads) AS threads_total,
	(SELECT COUNT(*) FROM muses) AS muses,
	(SELECT COUNT(*) FROM todos) AS todos,
	(SELECT COUNT(*) FROM watchers) AS watchers,
    (SELECT COUNT(*) FROM scheduled_messages WHERE archived = false) AS scheduled_messages
FROM (
	SELECT user_id, guild_id FROM muses
	UNION
	SELECT user_id, guild_id FROM threads
	UNION
	SELECT user_id, guild_id FROM todos
	UNION
	SELECT user_id, 0 FROM scheduled_messages
	UNION
	SELECT user_id, 0 FROM user_settings
) AS users_and_guilds;
