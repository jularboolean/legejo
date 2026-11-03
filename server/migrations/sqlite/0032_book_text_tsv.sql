-- Nothing to do: SQLite searches the text through book_text_fts, which keeps
-- its own index. The number matches the Postgres migration that stores the
-- words of each part.
SELECT 1;
