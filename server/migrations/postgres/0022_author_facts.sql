-- What Wikidata says about an author, kept so that it is asked once in a
-- while and never while a page waits. One row per author on the instance,
-- by the same key the web reader uses for a person (the folded name).
-- state: 'ok' (a person was found for certain), 'none' (no certain match),
-- 'error' (Wikidata did not answer). facts holds the found person as JSON.
CREATE TABLE author_facts (
    author_key TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    state      TEXT NOT NULL,
    qid        TEXT,
    facts      TEXT,
    fetched_at TEXT NOT NULL
);

-- An author whose line a user has said is about the wrong person.
CREATE TABLE author_facts_hidden (
    user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    author_key TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'utc', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
    PRIMARY KEY (user_id, author_key)
);
