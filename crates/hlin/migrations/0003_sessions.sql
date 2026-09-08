-- Sessions the shell issues for itself, when it authenticates people rather
-- than being told who they are (HLIN-S-0005, `oidc`).
--
-- Server-side rather than a signed cookie carrying the claims, and that is a
-- decision with consequences worth writing down.
--
-- A self-contained cookie cannot be withdrawn. Logging out clears the browser's
-- copy and nothing else; a copy taken before that keeps working until it
-- expires, and there is no answer to "revoke this person now" short of rotating
-- a key and ending everyone's session at once. A row can simply be deleted.
--
-- It also keeps the IdP's tokens out of the browser. The forwarding
-- credentialers need them (`forward-bearer`, `token-exchange`), and a cookie
-- large enough to carry an access token is a cookie that goes to the platform
-- on every request the browser makes.
--
-- What it costs is a lookup per request and a table that grows. The lookup is
-- one indexed read on the primary key; the growth is what the sweeper is for.
--
-- A restart does not sign anybody out, which is the behaviour to want: the
-- alternative logs out an organisation because a shell was redeployed.
CREATE TABLE sessions (
    -- The SHA-256 of the cookie's value, hex, never the value itself.
    --
    -- The cookie is a bearer credential: whoever holds it is the person. A
    -- dump of this table is therefore a set of live sessions if the values are
    -- stored as they are sent, and a set of useless hashes if they are not.
    -- The shell has the value on every request and can hash it, so storing the
    -- value buys nothing at all.
    id         TEXT        PRIMARY KEY,

    -- Who they are. Denormalised from the IdP's claims at sign-in rather than
    -- re-derived per request: the shell is not the authority on identity and a
    -- session is a record of what the authority said, at a moment.
    subject    TEXT        NOT NULL,
    name       TEXT,
    groups     JSONB       NOT NULL DEFAULT '[]'::jsonb,

    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);

-- The sweeper's read, and nothing else's.
CREATE INDEX sessions_by_expiry ON sessions (expires_at);

-- One sign-in that has been started and not yet finished.
--
-- The state and nonce exist to bind the browser that started a sign-in to the
-- one that finishes it, and the PKCE verifier to bind the request that asked
-- for a code to the one that redeems it. All three are worthless if the shell
-- cannot remember what it sent, and holding them in a cookie would mean the
-- browser carrying the verifier that is supposed to prove the browser did not
-- forge it.
--
-- Rows are single-use: the callback deletes the row it claims, so a replayed
-- callback finds nothing and is refused. They are also short-lived — a sign-in
-- a person abandoned is not a sign-in that should still work an hour later.
CREATE TABLE pending_logins (
    -- The `state` parameter, as sent. A lookup key that is used once and then
    -- gone, rather than a credential held over time.
    state         TEXT        PRIMARY KEY,

    -- Echoed in the id token, and checked there.
    nonce         TEXT        NOT NULL,

    -- The PKCE verifier whose challenge went to the authorization endpoint.
    code_verifier TEXT        NOT NULL,

    -- Where the person was going before they were asked who they were. A path
    -- on this shell, checked before it is stored: an open redirect is a real
    -- vulnerability and the login route is exactly where they are found.
    redirect_to   TEXT        NOT NULL,

    created_at    TIMESTAMPTZ NOT NULL,
    expires_at    TIMESTAMPTZ NOT NULL
);

CREATE INDEX pending_logins_by_expiry ON pending_logins (expires_at);
