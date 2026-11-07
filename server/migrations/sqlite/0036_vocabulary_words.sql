-- The words of each book's study list, one row each, so that the words of a
-- reader's books in a language can be summed in the database (how much of
-- each book they know, the words to learn next) instead of reading every
-- book's list. Filled from vocabulary.study; vocabulary_indexed says which
-- computation of a book's vocabulary the rows come from.
CREATE TABLE vocabulary_words (
    file_sha256  TEXT NOT NULL,
    language     TEXT NOT NULL,
    stem         TEXT NOT NULL,
    word         TEXT NOT NULL,
    -- The forms the book writes it in, most common first, joined by spaces.
    forms        TEXT NOT NULL,
    n            INTEGER NOT NULL,
    name         INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (file_sha256, language, stem)
);
CREATE INDEX vocabulary_words_stem ON vocabulary_words (language, stem);

CREATE TABLE vocabulary_indexed (
    file_sha256  TEXT NOT NULL,
    language     TEXT NOT NULL,
    computed_at  TEXT NOT NULL,
    PRIMARY KEY (file_sha256, language)
);
