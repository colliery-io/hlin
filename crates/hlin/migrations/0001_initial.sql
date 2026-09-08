-- The shell's persistent state (decision HLIN-A-0006).
--
-- Two things live here, for two different reasons. Platform snapshots are the
-- shell's contract memory: without them, a restart forgets what every platform
-- last promised and an undeclared breaking change shipped across the restart
-- window goes undetected. Layouts are the product: the surfaces people compose
-- for themselves, which is the thing the whole system exists to let them do.
--
-- Runtime caches are deliberately absent. Per-principal option lists, in-flight
-- deduplication and staleness timers are process-local and are rebuilt on
-- start; persisting them would buy nothing and could serve a viewer data from
-- before a restart without anything saying so.

-- The last manifest seen for each platform, and its fingerprint.
CREATE TABLE platforms (
    platform_id              TEXT        PRIMARY KEY,
    manifest                 JSONB       NOT NULL,
    contract_hash            TEXT        NOT NULL,
    contract_version         TEXT        NOT NULL,
    observed_at              TIMESTAMPTZ NOT NULL,

    -- How many consecutive polls have now returned this same contract hash.
    -- The registry classifies a change only once this crosses the configured
    -- threshold, so a blue/green rollout flapping between two revisions does
    -- not raise a violation per flip (decision HLIN-A-0002).
    consecutive_observations INTEGER     NOT NULL DEFAULT 1
        CHECK (consecutive_observations > 0)
);

-- A surface someone composed.
CREATE TABLE layouts (
    id          UUID        PRIMARY KEY,

    -- The principal who created it. Exactly one owner; there are no team
    -- entities in v1 (decision HLIN-A-0007).
    owner       TEXT        NOT NULL,
    title       TEXT        NOT NULL,

    -- 'personal' is visible to its owner and anyone holding its link;
    -- 'published' is listed in the shell-wide gallery.
    visibility  TEXT        NOT NULL DEFAULT 'personal'
        CHECK (visibility IN ('personal', 'published')),

    -- Where this layout was copied from, when someone edited one they did not
    -- own. Kept as provenance; a deleted original does not take its forks with
    -- it, which is why this is SET NULL rather than CASCADE.
    forked_from UUID        REFERENCES layouts (id) ON DELETE SET NULL,

    -- The time range the author meant, so a shared layout opens the way they
    -- left it.
    time_range  JSONB,

    created_at  TIMESTAMPTZ NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL
);

CREATE INDEX layouts_by_owner ON layouts (owner, updated_at DESC);

-- The gallery listing: published layouts, most recently touched first.
CREATE INDEX layouts_published ON layouts (updated_at DESC)
    WHERE visibility = 'published';

-- One panel on one surface.
CREATE TABLE panel_instances (
    id             UUID        PRIMARY KEY,
    layout_id      UUID        NOT NULL REFERENCES layouts (id) ON DELETE CASCADE,

    -- The manifest reference. Deliberately not a foreign key: a layout may
    -- name a platform or panel that does not currently exist, and renders it
    -- as unavailable (unknown) rather than losing it. A platform coming back
    -- restores the panel.
    platform_id    TEXT        NOT NULL,
    panel_key      TEXT        NOT NULL,

    -- The viewer's choices. A null override means "whatever the manifest says",
    -- so a platform changing its default title or kind reaches everyone who
    -- did not override it.
    kind_override  TEXT,
    title_override TEXT,

    -- Values for this instance's own parameters, and composition-level
    -- customisations such as thresholds.
    selections     JSONB       NOT NULL DEFAULT '{}'::jsonb,
    customizations JSONB       NOT NULL DEFAULT '{}'::jsonb,

    -- Where it sits on the surface.
    position       JSONB       NOT NULL,

    created_at     TIMESTAMPTZ NOT NULL
);

CREATE INDEX panel_instances_by_layout ON panel_instances (layout_id);
