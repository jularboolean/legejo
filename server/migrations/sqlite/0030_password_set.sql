-- Whether the owner of an account knows its password. An account made at a
-- single sign-on has a random one that nobody does. Those of the past are
-- found in the log, less the ones that have chosen a password since.
ALTER TABLE users ADD COLUMN password_set INTEGER NOT NULL DEFAULT 1;
UPDATE users SET password_set = 0
 WHERE id IN (SELECT actor_id FROM activity_log WHERE action = 'user.oidc_created')
   AND id NOT IN (SELECT actor_id FROM activity_log WHERE action = 'user.password_reset' AND actor_id IS NOT NULL);
