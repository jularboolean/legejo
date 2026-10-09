-- The fediverse is something each user turns on for themselves: the page for
-- following shelves on other instances stays out of the way until then.
-- Those who already follow a shelf or federate one of their own keep it.

ALTER TABLE users ADD COLUMN fediverse BIGINT NOT NULL DEFAULT 0;

UPDATE users SET fediverse = 1
WHERE id IN (SELECT owner_id FROM shelves WHERE visibility = 'federated')
   OR id IN (SELECT user_id FROM ap_follows);
