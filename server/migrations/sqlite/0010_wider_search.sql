-- Wider search: a language model, when the operator has set one up, turns a
-- search query into related search terms.

-- The terms a query gave, kept so that the same query is only paid for once.
CREATE TABLE search_expansions (
    query      TEXT NOT NULL,
    model      TEXT NOT NULL,
    terms      TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (query, model)
);

-- One row per request to the model, for the operator's view of the use.
CREATE TABLE ai_usage (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id           INTEGER REFERENCES users(id) ON DELETE SET NULL,
    at                TEXT NOT NULL,
    model             TEXT NOT NULL,
    prompt_tokens     INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_ai_usage_at ON ai_usage (at);
