-- The state of each EPUB file: what the health check found, the hash of the
-- file as it was uploaded (repairs change the stored file), and the KOReader
-- ids of earlier versions of a file, so a copy on a device still finds its book.

ALTER TABLE books ADD COLUMN health TEXT;
ALTER TABLE books ADD COLUMN health_issues INTEGER NOT NULL DEFAULT 0;
ALTER TABLE books ADD COLUMN upload_sha256 TEXT;

CREATE TABLE book_file_ids (
    md5     TEXT NOT NULL,
    book_id INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    PRIMARY KEY (md5, book_id)
);
