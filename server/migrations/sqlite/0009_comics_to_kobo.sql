-- Comics now go to Kobo e-readers as well. A device is told of books newer
-- than its last sync, so the comics already in a library are dated to now:
-- otherwise they would never reach a device that has synced since.

UPDATE books SET created_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE format = 'cbz';
