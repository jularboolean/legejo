-- The time spent reading a book in the web reader, in seconds: only while
-- the page is in view and the reader has done something lately. Added to,
-- never set, so that devices reading the same book add up.
CREATE TABLE reader_time (
    user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    book_id    BIGINT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    seconds    BIGINT NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (user_id, book_id)
);
