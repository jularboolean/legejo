-- The text at the web reader's position, so that the place can be found in
-- the KEPUB a Kobo reads, and the bookmark once placed there.
ALTER TABLE reading_progress ADD COLUMN text TEXT;
ALTER TABLE reading_progress ADD COLUMN kobo_json TEXT;
