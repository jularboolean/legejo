-- Sharing an audiobook: private (the owner alone), restricted (the listed
-- users too) or instance (everyone with an account here).

ALTER TABLE audiobooks ADD COLUMN visibility TEXT NOT NULL DEFAULT 'private';

CREATE TABLE audiobook_members (
    audiobook_id BIGINT NOT NULL REFERENCES audiobooks(id) ON DELETE CASCADE,
    user_id      BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    added_at     TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'utc', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
    PRIMARY KEY (audiobook_id, user_id)
);
CREATE INDEX idx_audiobook_members_user ON audiobook_members (user_id);

-- Each listener has an own feed address, so that one person's access can end
-- without the others' addresses changing. The owner's key stays in
-- audiobooks.feed_key.
CREATE TABLE audiobook_feeds (
    audiobook_id BIGINT NOT NULL REFERENCES audiobooks(id) ON DELETE CASCADE,
    user_id      BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    feed_key     TEXT NOT NULL UNIQUE,
    PRIMARY KEY (audiobook_id, user_id)
);
