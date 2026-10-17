-- The text of the EPUBs, for searching inside the books, kept only while an
-- admin has the search turned on. Stored once per text, not per book: an
-- imported copy, or a file whose metadata was rewritten, points to the same
-- rows. A section is cut into parts of a bounded size, so that a book in one
-- long file still indexes (a tsvector holds at most 1 MB) and a hit says
-- where it is. `section` is the index in booktext::sections, `pos` the byte
-- offset of the part in that section's text.
CREATE TABLE book_text (
    id        BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    text_hash TEXT NOT NULL,
    section   BIGINT NOT NULL,
    pos       BIGINT NOT NULL,
    -- Where the part starts in the book, 0–1, as booktext measures it.
    place     DOUBLE PRECISION NOT NULL,
    title     TEXT,
    body      TEXT NOT NULL,
    UNIQUE (text_hash, section, pos)
);
-- Must use the exact expression the queries use.
CREATE INDEX idx_book_text_fts ON book_text USING GIN (to_tsvector('simple', body));

-- The text a book's file holds (the key in book_text), and the file hash it
-- was read from: a book whose file changed since is looked at again.
ALTER TABLE books ADD COLUMN text_hash TEXT;
ALTER TABLE books ADD COLUMN text_sha256 TEXT;
CREATE INDEX idx_books_text_hash ON books (text_hash);
