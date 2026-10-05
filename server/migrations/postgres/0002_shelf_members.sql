-- Restricted shelves: a private shelf that the listed users can also see.
-- `visibility` stays 'private' for them, so a server that predates this
-- migration keeps the shelf hidden.

ALTER TABLE shelves ADD COLUMN restricted BIGINT NOT NULL DEFAULT 0;

CREATE TABLE shelf_members (
    shelf_id BIGINT NOT NULL REFERENCES shelves(id) ON DELETE CASCADE,
    user_id  BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    added_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'utc', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
    PRIMARY KEY (shelf_id, user_id)
);
CREATE INDEX idx_shelf_members_user ON shelf_members (user_id);
