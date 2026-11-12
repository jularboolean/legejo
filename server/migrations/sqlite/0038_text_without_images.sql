-- The text of a book no longer holds its images' alt text, which was put in
-- as "[image: …]". The books whose stored text has it are read again by the
-- search, and their length for the reading time counted again; the text
-- they leave behind is swept away once no book points to it.
UPDATE books SET text_sha256 = NULL, text_chars_sha256 = NULL
WHERE text_hash IN (SELECT DISTINCT text_hash FROM book_text WHERE body LIKE '%[image:%');
