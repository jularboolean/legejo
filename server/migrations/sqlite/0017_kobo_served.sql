-- What each device was last handed for a book (the ETag of the answer), so
-- that checkforchanges can name the books whose highlights have changed here
-- since: the device fetches those again and takes the changes.
CREATE TABLE kobo_served (
    device_id TEXT NOT NULL REFERENCES kobo_devices(device_id) ON DELETE CASCADE,
    book_id   INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    etag      TEXT NOT NULL,
    served_at TEXT NOT NULL,
    PRIMARY KEY (device_id, book_id)
);
