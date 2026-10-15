-- Where in the book a highlight stands, for reading it off a list: the
-- chapter, how far through the book (0-1), and the page of the printed
-- edition when the book has a page list. JSON, written by the web reader.
ALTER TABLE annotations ADD COLUMN place TEXT;
