-- A layout holds items of several types, not only panels (decision
-- HLIN-A-0015, task HLIN-T-0111).
--
-- The items are one JSON list on the layout row, each tagged by its `type`:
-- panel, bar, page-selector, content. A layout is edited as one thing and
-- read as one thing, so one column fits it better than a table per type.
--
-- Nothing stored before this must be kept, so the old panels are not carried
-- over: every layout starts with no items.
ALTER TABLE layouts
    ADD COLUMN items JSONB NOT NULL DEFAULT '[]'::jsonb;

DROP TABLE panel_instances;
