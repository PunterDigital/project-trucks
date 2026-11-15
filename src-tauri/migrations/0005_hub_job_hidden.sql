-- "Hide on Hub" from the app (#4654): whether the driver hid a synced job on the Hub. Filled from
-- the Hub's sync responses (`hidden`, Hub #30) and from the app's own toggle (`PATCH /jobs/{id}`).

ALTER TABLE hub_jobs ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;
