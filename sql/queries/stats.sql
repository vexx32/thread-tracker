SELECT
	COUNT(DISTINCT user_id) AS users,
	COUNT(DISTINCT guild_id) AS servers,
	(SELECT COUNT(DISTINCT channel_id) FROM threads) AS threads_distinct,
    (SELECT COUNT(*) FROM threads) AS threads_total,
	(SELECT COUNT(*) FROM muses) AS muses,
	(SELECT COUNT(*) FROM todos) AS todos,
	(SELECT COUNT(*) FROM watchers) AS watchers
FROM (
	SELECT user_id, guild_id FROM muses
	UNION
	SELECT user_id, guild_id FROM threads
	UNION
	SELECT user_id, guild_id FROM todos
) AS users_and_guilds;
