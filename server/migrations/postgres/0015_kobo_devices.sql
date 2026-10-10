-- Which Kobo belongs to whom. A device names itself in every request
-- (x-kobo-deviceid); the ones it makes with the sync token in the path tell
-- us the user, and the ones it makes to the site root (its "reading
-- services", for highlights and notes) carry only the device id.

CREATE TABLE kobo_devices (
    device_id    TEXT PRIMARY KEY,
    user_id      BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    model        TEXT,
    last_seen_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'utc', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')
);
CREATE INDEX idx_kobo_devices_user ON kobo_devices (user_id);
