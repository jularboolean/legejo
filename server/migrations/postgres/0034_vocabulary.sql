-- A book's vocabulary: how many words, how many distinct stems, how many
-- of the most common ones cover a share of the text. Kept once per file and
-- language, so that copies in several libraries count once.
CREATE TABLE vocabulary (
    file_sha256  TEXT NOT NULL,
    language     TEXT NOT NULL,
    version      BIGINT  NOT NULL,
    words        BIGINT  NOT NULL,
    stems        BIGINT  NOT NULL,
    hapax        BIGINT  NOT NULL,
    -- Stems needed for 90 % of a fixed-size sample; 0 when the book is
    -- shorter than the sample and cannot be compared.
    sample_need  BIGINT  NOT NULL,
    coverage     TEXT NOT NULL,
    curve        TEXT NOT NULL,
    candidates   TEXT NOT NULL,
    study        TEXT,
    computed_at  TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'utc', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
    PRIMARY KEY (file_sha256, language)
);
CREATE INDEX vocabulary_language ON vocabulary (language);

-- Words a reader already knows, by language and stem; the study lists of
-- every book in that language share them.
CREATE TABLE known_words (
    user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    language   TEXT NOT NULL,
    stem       TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'utc', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
    PRIMARY KEY (user_id, language, stem)
);
