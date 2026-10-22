-- More about each book on a followed shelf, for its page on the Fediverse
-- page: the whole description (the summary is cut short for Mastodon), and
-- what else the sending instance knows. series_index is text: a number such
-- as 2.5 kept as it came, without a nullable float.
ALTER TABLE ap_remote_books ADD COLUMN description TEXT;
ALTER TABLE ap_remote_books ADD COLUMN publisher TEXT;
ALTER TABLE ap_remote_books ADD COLUMN series TEXT;
ALTER TABLE ap_remote_books ADD COLUMN series_index TEXT;
ALTER TABLE ap_remote_books ADD COLUMN category TEXT;
ALTER TABLE ap_remote_books ADD COLUMN first_published BIGINT;
ALTER TABLE ap_remote_books ADD COLUMN edition_date TEXT;
-- The book's own page on its instance.
ALTER TABLE ap_remote_books ADD COLUMN page_url TEXT;
-- When the shelf's outbox was last read: it is read again, now and then,
-- when someone opens the shelf, so that books received before an instance
-- sent these details get them too.
ALTER TABLE ap_remote_actors ADD COLUMN outbox_read_at TEXT;
-- The cover of a shelf followed by a private link, as its instance shows
-- it: its own picture (the address, never handed to the browser), or the
-- seed that picks the same one of the drawn covers.
ALTER TABLE linked_shelves ADD COLUMN cover_url TEXT;
ALTER TABLE linked_shelves ADD COLUMN cover_seed BIGINT;
