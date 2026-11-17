-- The game's own figures for recorded jobs (#4756). RenCloud's scs-telemetry passes on the game's
-- "delivered" / "cancelled" events with what the game paid, the XP, cargo damage, the job's
-- distance and whether auto-park / auto-load were used, or the cancel penalty. NULL for manual
-- entries and for jobs recorded before this or with the old plugin. Not synced to the Hub (yet):
-- the hub_job_updated trigger's column list stays as it is.
ALTER TABLE jobs ADD COLUMN game_revenue INTEGER;
ALTER TABLE jobs ADD COLUMN earned_xp INTEGER;
ALTER TABLE jobs ADD COLUMN cargo_damage REAL;
ALTER TABLE jobs ADD COLUMN game_distance_km REAL;
ALTER TABLE jobs ADD COLUMN auto_park INTEGER;
ALTER TABLE jobs ADD COLUMN auto_load INTEGER;
ALTER TABLE jobs ADD COLUMN cancel_penalty INTEGER;

-- Tolls, fines and ferry/train fares. job_id is the job they were paid on (NULL when driving
-- without one); deleting the job deletes its costs. The truck is kept for per-truck totals.
CREATE TABLE expenses (
  id TEXT PRIMARY KEY,
  job_id TEXT,
  kind TEXT NOT NULL CHECK (kind IN ('TOLL', 'FINE', 'FERRY', 'TRAIN')),
  amount INTEGER NOT NULL,
  detail TEXT,
  truck_make TEXT,
  truck_model TEXT,
  occurred_at TEXT NOT NULL,
  FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE
);

CREATE INDEX expenses_job_id ON expenses (job_id);
CREATE INDEX expenses_occurred_at ON expenses (occurred_at);
