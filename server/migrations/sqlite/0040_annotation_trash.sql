-- Highlights a Kobo has removed: kept for 30 days instead of being deleted
-- at once, so that one removed by mistake on the device can be brought back.
CREATE TABLE annotation_trash (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    book_id              INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    user_id              INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source               TEXT NOT NULL,
    cfi                  TEXT,
    text                 TEXT NOT NULL,
    note                 TEXT,
    color                TEXT,
    kobo_id              TEXT,
    location             TEXT,
    kobo_json            TEXT,
    place                TEXT,
    translation          TEXT,
    translation_language TEXT,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    trashed_at           TEXT NOT NULL
);
CREATE INDEX idx_annotation_trash_user ON annotation_trash (user_id, trashed_at);
