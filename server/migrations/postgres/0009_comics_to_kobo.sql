-- Comics now go to Kobo e-readers as well. A device is told of books newer
-- than its last sync, so the comics already in a library are dated to now:
-- otherwise they would never reach a device that has synced since.

UPDATE books SET created_at = to_char(now() AT TIME ZONE 'utc', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') WHERE format = 'cbz';
