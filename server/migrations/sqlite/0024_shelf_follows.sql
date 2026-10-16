-- A shelf on this instance, shared with a user, that the user follows:
-- when they last looked at it (books added to the shelf after that are
-- new to them), and the key of their own Atom feed of the shelf.
CREATE TABLE shelf_follows (
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    shelf_id    INTEGER NOT NULL REFERENCES shelves(id) ON DELETE CASCADE,
    followed_at TEXT NOT NULL,
    seen_at     TEXT NOT NULL,
    feed_key    TEXT NOT NULL UNIQUE,
    PRIMARY KEY (user_id, shelf_id)
);
CREATE INDEX idx_shelf_follows_shelf ON shelf_follows (shelf_id);
