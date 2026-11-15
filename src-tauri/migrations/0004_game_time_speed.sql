-- Average speed per in-game hour (#4771).
--
-- ETS2's odometer counts game kilometres, which pass ~19x faster than real time on roads, so
-- distance / real time put every recorded delivery at several hundred km/h and flagged it
-- "Average speed over 150 km/h". Jobs now keep their in-game duration and the 150 km/h check
-- uses it; every recorded job also has to stay under 3,000 km per real hour.
ALTER TABLE jobs ADD COLUMN game_duration_min INTEGER;

-- Recorded jobs flagged by the old check alone: no game time was kept for them, so only the
-- real-time limit can judge them now. Ones under it lose the speed reason (and the flag, when
-- it was the only reason). A flag the driver set or changed by hand has other text and stays.
UPDATE jobs SET flagged = 0, flag_reason = NULL
WHERE source = 'TELEMETRY'
  AND flag_reason = 'Average speed over 150 km/h'
  AND duration_sec > 0
  AND distance_km / (duration_sec / 3600.0) <= 3000;

UPDATE jobs SET flag_reason = 'Distance under 1 km'
WHERE source = 'TELEMETRY'
  AND flag_reason = 'Distance under 1 km; Average speed over 150 km/h'
  AND duration_sec > 0
  AND distance_km / (duration_sec / 3600.0) <= 3000;

-- The Hub gets the new column too: same trigger as 0003, plus game_duration_min. (The flag
-- updates above ran with the old trigger, which already queues a changed flag.)
DROP TRIGGER hub_job_updated;

CREATE TRIGGER hub_job_updated AFTER UPDATE OF
  truck_make, truck_model, source_city, source_company, dest_city, dest_company, cargo_id,
  cargo_name, cargo_mass_kg, declared_income, distance_km, duration_sec, game_duration_min,
  avg_speed_kmh, fuel_used_l, damage_delta, started_at, completed_at, status, source, flagged,
  flag_reason
ON jobs
WHEN (SELECT value FROM settings WHERE key = 'hub_linked') = 'true'
  AND NEW.source <> 'MANUAL'
  AND (EXISTS (SELECT 1 FROM hub_jobs WHERE job_id = NEW.id)
       OR EXISTS (SELECT 1 FROM hub_outbox WHERE job_id = NEW.id))
BEGIN
  INSERT OR REPLACE INTO hub_outbox (job_id, op) VALUES (NEW.id, 'upsert');
END;
