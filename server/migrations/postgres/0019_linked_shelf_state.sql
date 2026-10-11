-- Whether a shelf followed by link answered the last time it was asked:
-- 'ok', 'unreachable' (its instance did not answer) or 'gone' (the link no
-- longer works), since when, and when it was last asked.
ALTER TABLE linked_shelves ADD COLUMN state TEXT NOT NULL DEFAULT 'ok';
ALTER TABLE linked_shelves ADD COLUMN state_since TEXT;
ALTER TABLE linked_shelves ADD COLUMN checked_at TEXT;
