-- Audiobooks: a set of audio files with metadata, listened to through a
-- podcast feed that the secret `feed_key` gives access to.

CREATE TABLE audiobooks (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    uuid        TEXT NOT NULL UNIQUE,
    owner_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title       TEXT NOT NULL DEFAULT '',
    author      TEXT,
    narrator    TEXT,
    language    TEXT,
    description TEXT,
    cover_mime  TEXT,
    feed_key    TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'utc', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
    updated_at  TEXT
);
CREATE INDEX idx_audiobooks_owner ON audiobooks (owner_id);

-- One file is one part, in listening order by `position`.
CREATE TABLE audiobook_files (
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    audiobook_id BIGINT NOT NULL REFERENCES audiobooks(id) ON DELETE CASCADE,
    uuid         TEXT NOT NULL UNIQUE,
    position     BIGINT NOT NULL,
    title        TEXT,
    filename     TEXT NOT NULL,
    mime         TEXT NOT NULL,
    seconds      BIGINT NOT NULL DEFAULT 0,
    bytes        BIGINT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'utc', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
);
CREATE INDEX idx_audiobook_files_book ON audiobook_files (audiobook_id, position);
