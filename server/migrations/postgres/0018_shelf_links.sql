-- A shelf shared by a private link: an address with a token in it that
-- gives the shelf as an OPDS feed without a login, one per person it is
-- given to, so that one can be taken back without disturbing the others.
CREATE TABLE shelf_links (
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    shelf_id     BIGINT NOT NULL REFERENCES shelves(id) ON DELETE CASCADE,
    token        TEXT NOT NULL UNIQUE,
    name         TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'utc', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
    expires_at   TEXT,
    last_used_at TEXT
);
CREATE INDEX idx_shelf_links_shelf ON shelf_links (shelf_id);

-- The other end: such a link, from this or another instance, that a user
-- here follows. The shelf is read from its feed each time it is opened.
CREATE TABLE linked_shelves (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id     BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    url         TEXT NOT NULL,
    title       TEXT NOT NULL,
    description TEXT,
    created_at  TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'utc', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
    UNIQUE (user_id, url)
);
