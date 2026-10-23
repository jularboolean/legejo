-- How long a book's text is, in characters with each run of white space
-- counted as one, for an estimate of the time it takes to read; 0 until it
-- has been counted, or for a file without text. And the file hash it was
-- counted from: a book whose file changed since is counted again.
ALTER TABLE books ADD COLUMN text_chars BIGINT NOT NULL DEFAULT 0;
ALTER TABLE books ADD COLUMN text_chars_sha256 TEXT;
