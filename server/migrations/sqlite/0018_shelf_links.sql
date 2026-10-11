-- A shelf shared by a private link: an address with a token in it that
-- gives the shelf as an OPDS feed without a login, one per person it is
-- given to, so that one can be taken back without disturbing the others.
CREATE TABLE shelf_links (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    shelf_id     INTEGER NOT NULL REFERENCES shelves(id) ON DELETE CASCADE,
    token        TEXT NOT NULL UNIQUE,
    name         TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    expires_at   TEXT,
    last_used_at TEXT
);
CREATE INDEX idx_shelf_links_shelf ON shelf_links (shelf_id);

-- The other end: such a link, from this or another instance, that a user
-- here follows. The shelf is read from its feed each time it is opened.
CREATE TABLE linked_shelves (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    url         TEXT NOT NULL,
    title       TEXT NOT NULL,
    description TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (user_id, url)
);
