-- Highlights and notes in a book. Made in the web reader (source 'web',
-- placed by a range CFI), or, later, brought in from a Kobo (source 'kobo',
-- with the device's own id and location kept as it sent them). The passage
-- itself is always stored, so an annotation can be shown and exported
-- whatever its origin.

CREATE TABLE annotations (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    book_id    INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source     TEXT NOT NULL DEFAULT 'web',
    cfi        TEXT,
    text       TEXT NOT NULL,
    note       TEXT,
    color      TEXT,
    kobo_id    TEXT,
    location   TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX idx_annotations_book ON annotations (book_id, user_id);
