-- The admin role (decision HLIN-A-0016, task HLIN-T-0117).
--
-- 1. A layout may be `public`: open to anonymous viewers. Only an admin sets
--    it; the shell checks that, not the database.
ALTER TABLE layouts DROP CONSTRAINT layouts_visibility_check;
ALTER TABLE layouts ADD CONSTRAINT layouts_visibility_check
    CHECK (visibility IN ('personal', 'published', 'public'));

CREATE INDEX layouts_public ON layouts (updated_at DESC)
    WHERE visibility = 'public';

-- 2. The one org default surface. One row at most: the key can only be TRUE.
--    Deleting the layout clears it.
CREATE TABLE org_default (
    singleton BOOLEAN     PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    layout_id UUID        NOT NULL REFERENCES layouts (id) ON DELETE CASCADE,
    set_at    TIMESTAMPTZ NOT NULL
);

-- 3. The values of the admin claim, read at sign-in under `oidc`. Kept apart
--    from `groups`, which go to platforms. Sessions issued before this have
--    none until the next sign-in.
ALTER TABLE sessions ADD COLUMN admin_claim JSONB NOT NULL DEFAULT '[]'::jsonb;
