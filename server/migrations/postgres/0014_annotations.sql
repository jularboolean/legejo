-- Highlights and notes in a book. Made in the web reader (source 'web',
-- placed by a range CFI), or, later, brought in from a Kobo (source 'kobo',
-- with the device's own id and location kept as it sent them). The passage
-- itself is always stored, so an annotation can be shown and exported
-- whatever its origin.

CREATE TABLE annotations (
    id         BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    book_id    BIGINT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source     TEXT NOT NULL DEFAULT 'web',
    cfi        TEXT,
    text       TEXT NOT NULL,
    note       TEXT,
    color      TEXT,
    kobo_id    TEXT,
    location   TEXT,
    created_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'utc', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
    updated_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'utc', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
);
CREATE INDEX idx_annotations_book ON annotations (book_id, user_id);
