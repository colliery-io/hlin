-- What a platform has been caught doing (decision HLIN-A-0002).
--
-- The shell detects a breaking change shipped without a major bump, and until
-- now said so in a single log line at the moment of classification. That is a
-- signal an operator sees only if they happen to be looking, and it is gone
-- when the log rotates — so the one question the mechanism exists to answer,
-- "has this platform ever done this?", had no answer.
--
-- On the platform row rather than in a table of its own: an operator wants to
-- know whether and when, not to page through a history, and a platform that
-- violates repeatedly is one problem rather than many. The count carries the
-- repetition without the rows.
ALTER TABLE platforms
    ADD COLUMN last_violation JSONB;
