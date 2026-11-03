-- The words of each part, kept with it, so that a search compares them
-- instead of working them out of the text again for every part it looks
-- at: done at query time, a search among many books' parts could take half
-- a minute. The column starts empty, so that adding it does not rewrite the
-- table; the indexer fills in the parts already stored (fulltext.rs), and a
-- search meanwhile works out the words of those it has not reached.
ALTER TABLE book_text ADD COLUMN tsv tsvector;
CREATE INDEX idx_book_text_tsv ON book_text USING GIN (tsv);
CREATE INDEX idx_book_text_unfilled ON book_text (id) WHERE tsv IS NULL;
-- idx_book_text_fts stays for now: a version before this one, still running
-- while an update rolls out, searches through it.
