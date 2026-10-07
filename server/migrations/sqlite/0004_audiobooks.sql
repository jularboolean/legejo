-- Audiobooks: a set of audio files with metadata, listened to through a
-- podcast feed that the secret `feed_key` gives access to.

CREATE TABLE audiobooks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    uuid        TEXT NOT NULL UNIQUE,
    owner_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title       TEXT NOT NULL DEFAULT '',
    author      TEXT,
    narrator    TEXT,
    language    TEXT,
    description TEXT,
    cover_mime  TEXT,
    feed_key    TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at  TEXT
);
CREATE INDEX idx_audiobooks_owner ON audiobooks (owner_id);

-- One file is one part, in listening order by `position`.
CREATE TABLE audiobook_files (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    audiobook_id INTEGER NOT NULL REFERENCES audiobooks(id) ON DELETE CASCADE,
    uuid         TEXT NOT NULL UNIQUE,
    position     INTEGER NOT NULL,
    title        TEXT,
    filename     TEXT NOT NULL,
    mime         TEXT NOT NULL,
    seconds      INTEGER NOT NULL DEFAULT 0,
    bytes        INTEGER NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX idx_audiobook_files_book ON audiobook_files (audiobook_id, position);
