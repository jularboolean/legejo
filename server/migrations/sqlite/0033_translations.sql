-- A highlight translated by the librarian's model into the reader's
-- language, kept with it so that it is asked for once.
ALTER TABLE annotations ADD COLUMN translation TEXT;
ALTER TABLE annotations ADD COLUMN translation_language TEXT;

-- What a request to the model was for: a question to the librarian, or a
-- translation. Each has its own limit per hour.
ALTER TABLE ai_usage ADD COLUMN kind TEXT NOT NULL DEFAULT 'question';
