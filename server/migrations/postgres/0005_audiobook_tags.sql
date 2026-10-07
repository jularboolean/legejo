-- Audiobooks are sorted the way books are: one category and any number of tags.

ALTER TABLE audiobooks ADD COLUMN category TEXT;

CREATE TABLE audiobook_tags (
    audiobook_id BIGINT NOT NULL REFERENCES audiobooks(id) ON DELETE CASCADE,
    tag          TEXT NOT NULL,
    PRIMARY KEY (audiobook_id, tag)
);
