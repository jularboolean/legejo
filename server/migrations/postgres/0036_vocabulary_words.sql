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
    n            BIGINT NOT NULL,
    name         BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (file_sha256, language, stem)
);
CREATE INDEX vocabulary_words_stem ON vocabulary_words (language, stem);

CREATE TABLE vocabulary_indexed (
    file_sha256  TEXT NOT NULL,
    language     TEXT NOT NULL,
    computed_at  TEXT NOT NULL,
    PRIMARY KEY (file_sha256, language)
);

-- Which language pack (language:version:hash) a book's words were counted
-- with; '' when the language has none.
ALTER TABLE vocabulary ADD COLUMN pack TEXT NOT NULL DEFAULT '';

-- Each word of a reader's books in a language summed over the books, so
-- that the words page reads one indexed table instead of every book's words.
-- Built when the page is read and found out of date (words.rs), never by
-- the worker.
CREATE TABLE word_sums (
    user_id  BIGINT NOT NULL,
    language TEXT NOT NULL,
    stem     TEXT NOT NULL,
    word     TEXT NOT NULL,
    -- The forms one of the books writes it in, space-joined (the first by
    -- the sort of the books' lists; a portable string aggregate does not
    -- exist across the two dialects).
    forms    TEXT NOT NULL,
    total    BIGINT NOT NULL,
    books    BIGINT NOT NULL,
    PRIMARY KEY (user_id, language, stem)
);
CREATE INDEX word_sums_total ON word_sums (user_id, language, total DESC);

-- Per reader and book: the words there are to learn (names aside) and how
-- many of them the reader knows.
CREATE TABLE book_known (
    user_id   BIGINT NOT NULL,
    book_id   BIGINT NOT NULL,
    learnable BIGINT NOT NULL,
    known_n   BIGINT NOT NULL,
    PRIMARY KEY (user_id, book_id)
);

-- When a reader's summary in a language was built and from what, so that a
-- read can tell whether it is current.
CREATE TABLE word_summaries (
    user_id     BIGINT NOT NULL,
    language    TEXT NOT NULL,
    built_at    TEXT NOT NULL,
    -- The number of the reader's books with an indexed study list and the
    -- latest indexing among them when the summary was built.
    books       BIGINT NOT NULL,
    indexed_at  TEXT NOT NULL,
    -- Set when known words changed in bulk; the book shares are then rebuilt.
    known_dirty BIGINT NOT NULL DEFAULT 0,
    -- How many words the reader knew in the language when the book shares
    -- were last brought up to date: another number means they changed
    -- behind the summary's back (another process), and the shares are
    -- rebuilt.
    known       BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, language)
);
CREATE INDEX books_file ON books (file_sha256, language);

-- In how many indexed books of a language each candidate word occurs, for
-- the words that are common in one book and rare in the others.
CREATE TABLE vocabulary_df (
    language TEXT NOT NULL,
    stem     TEXT NOT NULL,
    docs     BIGINT NOT NULL,
    PRIMARY KEY (language, stem)
);
