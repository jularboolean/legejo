-- Vocabulary and study lists as an option a reader turns on (Account →
-- Options): off for new readers, on for those who have already ticked words.
ALTER TABLE users ADD COLUMN words INTEGER NOT NULL DEFAULT 0;
UPDATE users SET words = 1 WHERE id IN (SELECT DISTINCT user_id FROM known_words);
