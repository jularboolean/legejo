-- A highlight from a Kobo, whole as the device sent it. Handed back to the
-- device when it asks what the server has for the book, in the device's own
-- shape; without that answer the device takes the book to have none.
ALTER TABLE annotations ADD COLUMN kobo_json TEXT;
