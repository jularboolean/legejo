-- OPDS catalogs of other libraries that a user browses and fetches books
-- from. `search_template` is the catalog's OpenSearch address with
-- {searchTerms} in it, when the catalog can be searched.

CREATE TABLE opds_catalogs (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title           TEXT NOT NULL,
    url             TEXT NOT NULL,
    search_template TEXT,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (user_id, url)
);
