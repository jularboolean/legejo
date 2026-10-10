-- Which Kobo belongs to whom. A device names itself in every request
-- (x-kobo-deviceid); the ones it makes with the sync token in the path tell
-- us the user, and the ones it makes to the site root (its "reading
-- services", for highlights and notes) carry only the device id.

CREATE TABLE kobo_devices (
    device_id    TEXT PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    model        TEXT,
    last_seen_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX idx_kobo_devices_user ON kobo_devices (user_id);
