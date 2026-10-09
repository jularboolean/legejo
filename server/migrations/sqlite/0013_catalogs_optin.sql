-- Catalogs of other libraries are something each user turns on for
-- themselves, once an admin has turned them on for the instance: the server
-- then fetches from addresses the user gives it.

ALTER TABLE users ADD COLUMN catalogs INTEGER NOT NULL DEFAULT 0;
