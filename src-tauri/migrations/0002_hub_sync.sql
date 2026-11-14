-- ProjectTrucks Hub sync (#4634).
--
-- hub_outbox holds what still has to reach the Hub: an 'upsert' (send the job) or a 'delete'.
-- hub_jobs remembers which jobs the Hub has, so edits and deletes of those follow them there.
--
-- The triggers below fill the outbox, so every write path (manual entry, edits, deletes and
-- jobs recorded from telemetry) is covered without each one calling the sync code. They only
-- fire while the `hub_linked` setting is 'true': nothing is queued, let alone sent, before the
-- driver links an account. Jobs from before linking are queued only by "Sync history".

CREATE TABLE hub_outbox (
  job_id     TEXT PRIMARY KEY,
  op         TEXT NOT NULL,                 -- 'upsert' | 'delete'
  queued_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f', 'now')),
  attempts   INTEGER NOT NULL DEFAULT 0,    -- failed sends of this row; given up at 5
  last_error TEXT
);

CREATE TABLE hub_jobs (
  job_id    TEXT PRIMARY KEY,
  synced_at TEXT NOT NULL,
  hub_url   TEXT                            -- the job's page on the Hub
);

CREATE TRIGGER hub_job_inserted AFTER INSERT ON jobs
WHEN (SELECT value FROM settings WHERE key = 'hub_linked') = 'true'
BEGIN
  INSERT OR REPLACE INTO hub_outbox (job_id, op) VALUES (NEW.id, 'upsert');
END;

-- Every column the Hub receives (not `notes`, which never leaves the PC, nor `session_id`).
-- Only jobs the Hub already has or is about to get: editing an old, never-synced job doesn't
-- upload it behind the driver's back.
CREATE TRIGGER hub_job_updated AFTER UPDATE OF
  truck_make, truck_model, source_city, source_company, dest_city, dest_company, cargo_id,
  cargo_name, cargo_mass_kg, declared_income, distance_km, duration_sec, avg_speed_kmh,
  fuel_used_l, damage_delta, started_at, completed_at, status, source, flagged, flag_reason
ON jobs
WHEN (SELECT value FROM settings WHERE key = 'hub_linked') = 'true'
  AND (EXISTS (SELECT 1 FROM hub_jobs WHERE job_id = NEW.id)
       OR EXISTS (SELECT 1 FROM hub_outbox WHERE job_id = NEW.id))
BEGIN
  INSERT OR REPLACE INTO hub_outbox (job_id, op) VALUES (NEW.id, 'upsert');
END;

-- A job that never reached the Hub just leaves the queue; one the Hub has is deleted there too.
CREATE TRIGGER hub_job_deleted AFTER DELETE ON jobs
BEGIN
  DELETE FROM hub_outbox WHERE job_id = OLD.id;
  INSERT INTO hub_outbox (job_id, op)
    SELECT OLD.id, 'delete'
    WHERE EXISTS (SELECT 1 FROM hub_jobs WHERE job_id = OLD.id)
      AND (SELECT value FROM settings WHERE key = 'hub_linked') = 'true';
END;
