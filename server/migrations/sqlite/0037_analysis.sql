-- A book's analysis (analysis/): its style and readability, its phrases,
-- the people and places it names and its grammar, as one JSON document.
-- Computed when a reader first asks for it and kept once per file and
-- language; `pack` is the language pack it was computed with ('' without).
CREATE TABLE book_analysis (
    file_sha256  TEXT NOT NULL,
    language     TEXT NOT NULL,
    version      INTEGER NOT NULL,
    pack         TEXT NOT NULL DEFAULT '',
    data         TEXT NOT NULL,
    computed_at  TEXT NOT NULL,
    PRIMARY KEY (file_sha256, language)
);
