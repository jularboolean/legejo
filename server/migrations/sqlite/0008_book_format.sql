-- Books in other formats than EPUB: 'pdf' and 'cbz' are stored and handed
-- out as they are. The file is books/{uuid}.{format}.

ALTER TABLE books ADD COLUMN format TEXT NOT NULL DEFAULT 'epub';
