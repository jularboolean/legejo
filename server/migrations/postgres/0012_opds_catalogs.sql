-- OPDS catalogs of other libraries that a user browses and fetches books
-- from. `search_template` is the catalog's OpenSearch address with
-- {searchTerms} in it, when the catalog can be searched.

CREATE TABLE opds_catalogs (
    id              BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id         BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title           TEXT NOT NULL,
    url             TEXT NOT NULL,
    search_template TEXT,
    created_at      TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'utc', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
    UNIQUE (user_id, url)
);
