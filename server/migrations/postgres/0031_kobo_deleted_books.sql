-- Books deleted from the library, so that a Kobo that has them is told to
-- let them go. A book that is gone has no row left to say so.
CREATE TABLE kobo_deleted_books (
    uuid       TEXT PRIMARY KEY,
    owner_id   BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    deleted_at TEXT NOT NULL
);
CREATE INDEX idx_kobo_deleted_books_owner ON kobo_deleted_books (owner_id, deleted_at);
