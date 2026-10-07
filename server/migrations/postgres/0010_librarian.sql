-- The librarian: a language model, when the operator has set one up, picks
-- books from a user's library for a question. What the catalogue says about
-- the user's books is sent to the model, so each user turns it on for
-- themselves.

ALTER TABLE users ADD COLUMN librarian BIGINT NOT NULL DEFAULT 0;

-- One row per question asked, for the limit per user and the operator's view
-- of the use.

CREATE TABLE ai_usage (
    id                BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id           BIGINT REFERENCES users(id) ON DELETE SET NULL,
    at                TEXT NOT NULL,
    model             TEXT NOT NULL,
    prompt_tokens     BIGINT NOT NULL DEFAULT 0,
    completion_tokens BIGINT NOT NULL DEFAULT 0
);
CREATE INDEX idx_ai_usage_at ON ai_usage (at);
