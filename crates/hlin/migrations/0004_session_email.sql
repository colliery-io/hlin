-- The email address the identity provider gave at sign-in, where it gave one.
--
-- Carried to platforms in the token (HLIN-S-0004), for rules that turn on it:
-- which domain may post, which address is the owner. Denormalised at sign-in,
-- like the rest of the row, because a session is a record of what the
-- authority said at a moment.
--
-- Nullable, because providers are not obliged to send it and sessions issued
-- before this column existed have none. They get one at their next sign-in.
ALTER TABLE sessions ADD COLUMN email TEXT;
