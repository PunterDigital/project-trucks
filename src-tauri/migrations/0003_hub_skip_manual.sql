-- Jobs typed in by hand never go to the Hub (#4751): nothing there can check them.
--
-- The insert/update triggers from 0002 now skip source = 'MANUAL'. Manual jobs that were queued
-- leave the queue; ones the Hub already has get a 'delete' while linked (the Hub also removes
-- them itself), and lose their "on the Hub" mark otherwise.

DROP TRIGGER hub_job_inserted;
DROP TRIGGER hub_job_updated;

CREATE TRIGGER hub_job_inserted AFTER INSERT ON jobs
WHEN (SELECT value FROM settings WHERE key = 'hub_linked') = 'true'
  AND NEW.source <> 'MANUAL'
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
  AND NEW.source <> 'MANUAL'
  AND (EXISTS (SELECT 1 FROM hub_jobs WHERE job_id = NEW.id)
       OR EXISTS (SELECT 1 FROM hub_outbox WHERE job_id = NEW.id))
BEGIN
  INSERT OR REPLACE INTO hub_outbox (job_id, op) VALUES (NEW.id, 'upsert');
END;

DELETE FROM hub_outbox
WHERE op = 'upsert' AND job_id IN (SELECT id FROM jobs WHERE source = 'MANUAL');

INSERT OR REPLACE INTO hub_outbox (job_id, op)
SELECT job_id, 'delete' FROM hub_jobs
WHERE job_id IN (SELECT id FROM jobs WHERE source = 'MANUAL')
  AND (SELECT value FROM settings WHERE key = 'hub_linked') = 'true';

DELETE FROM hub_jobs
WHERE job_id IN (SELECT id FROM jobs WHERE source = 'MANUAL')
  AND job_id NOT IN (SELECT job_id FROM hub_outbox WHERE op = 'delete');
