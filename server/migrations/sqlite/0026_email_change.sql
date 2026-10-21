-- A change of email address waiting to be confirmed from the new address.
-- The address in use stays until then (verify_token is for registration,
-- and an account with one cannot log in).
ALTER TABLE users ADD COLUMN pending_email TEXT;
ALTER TABLE users ADD COLUMN email_token TEXT;
ALTER TABLE users ADD COLUMN email_token_expires TEXT;
