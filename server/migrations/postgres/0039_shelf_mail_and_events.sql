-- Mail about followed shelves: one message at most a day, listing the books
-- put on them since the follower last looked or was last told. On unless
-- the follower turns it off on the account page.
ALTER TABLE users ADD COLUMN follow_mail BIGINT NOT NULL DEFAULT 1;
ALTER TABLE users ADD COLUMN follow_mailed_at TEXT;
ALTER TABLE shelf_follows ADD COLUMN mailed_at TEXT;

-- What has happened on a shared shelf, for its owner: who follows it, who
-- took a copy of a book, what was downloaded through a private link or by
-- another instance. Names are kept as they were, so an entry stays readable
-- after the book or the user is gone.
CREATE TABLE shelf_events (
    id         BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    shelf_id   BIGINT NOT NULL REFERENCES shelves(id) ON DELETE CASCADE,
    at         TEXT NOT NULL,
    kind       TEXT NOT NULL,
    actor_id   BIGINT REFERENCES users(id) ON DELETE SET NULL,
    actor_name TEXT,
    book_id    BIGINT,
    book_title TEXT,
    detail     TEXT
);
CREATE INDEX idx_shelf_events_shelf ON shelf_events (shelf_id, id);
CREATE INDEX idx_shelf_events_at ON shelf_events (at);
