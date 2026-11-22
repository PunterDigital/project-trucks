-- The Hub takes the game's own figures for a job too (#5227): the same trigger as 0004, plus
-- the columns that go up (game_revenue, earned_xp, cargo_damage, cancel_penalty). A job's
-- costs come from `expenses`, which are written with the job in one transaction, so its first
-- upload already carries them.
DROP TRIGGER hub_job_updated;

CREATE TRIGGER hub_job_updated AFTER UPDATE OF
  truck_make, truck_model, source_city, source_company, dest_city, dest_company, cargo_id,
  cargo_name, cargo_mass_kg, declared_income, distance_km, duration_sec, game_duration_min,
  avg_speed_kmh, fuel_used_l, damage_delta, started_at, completed_at, status, source, flagged,
  flag_reason, game_revenue, earned_xp, cargo_damage, cancel_penalty
ON jobs
WHEN (SELECT value FROM settings WHERE key = 'hub_linked') = 'true'
  AND NEW.source <> 'MANUAL'
  AND (EXISTS (SELECT 1 FROM hub_jobs WHERE job_id = NEW.id)
       OR EXISTS (SELECT 1 FROM hub_outbox WHERE job_id = NEW.id))
BEGIN
  INSERT OR REPLACE INTO hub_outbox (job_id, op) VALUES (NEW.id, 'upsert');
END;
