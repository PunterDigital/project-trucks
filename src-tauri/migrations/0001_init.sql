-- Initial schema. Mirrors PLAN.md §5.
--
-- driving_sessions is the source of truth for km driven. Every distance the
-- truck moves while connected to the game lands here, whether it's free-roam
-- or part of a job. Truck rollups are derived from this table.
--
-- jobs carries delivery-specific fields and points back to its session row.

CREATE TABLE driving_sessions (
  id              TEXT PRIMARY KEY,
  kind            TEXT NOT NULL,         -- 'FREE_ROAM' | 'JOB'
  truck_make      TEXT,
  truck_model     TEXT,
  start_odometer  REAL NOT NULL,
  end_odometer    REAL NOT NULL,
  distance_km     REAL NOT NULL,         -- end_odometer - start_odometer, denormalised for fast sums
  started_at      TEXT NOT NULL,         -- ISO 8601
  ended_at        TEXT NOT NULL,         -- ISO 8601
  status          TEXT NOT NULL          -- 'COMPLETED' | 'CANCELLED' (cancelled only meaningful for JOB sessions)
);
CREATE INDEX idx_sessions_ended_at ON driving_sessions(ended_at);
CREATE INDEX idx_sessions_kind     ON driving_sessions(kind);

CREATE TABLE jobs (
  id              TEXT PRIMARY KEY,
  session_id      TEXT,                  -- FK to driving_sessions.id; nullable for MANUAL entries
  truck_make      TEXT,
  truck_model     TEXT,
  source_city     TEXT NOT NULL,
  source_company  TEXT NOT NULL,
  dest_city       TEXT NOT NULL,
  dest_company    TEXT NOT NULL,
  cargo_id        TEXT NOT NULL,
  cargo_name      TEXT NOT NULL,
  cargo_mass_kg   REAL NOT NULL,
  declared_income INTEGER NOT NULL,      -- game currency
  distance_km     REAL NOT NULL,         -- mirror of session.distance_km for telemetry rows
  duration_sec    INTEGER NOT NULL,
  avg_speed_kmh   REAL NOT NULL,
  fuel_used_l     REAL,
  damage_delta    REAL,                  -- 0..1
  started_at      TEXT NOT NULL,         -- ISO 8601
  completed_at    TEXT NOT NULL,         -- ISO 8601
  status          TEXT NOT NULL,         -- 'DELIVERED' | 'LATE' | 'CANCELLED'
  source          TEXT NOT NULL,         -- 'TELEMETRY' | 'MANUAL'
  flagged         INTEGER NOT NULL DEFAULT 0,
  flag_reason     TEXT,
  notes           TEXT,
  FOREIGN KEY (session_id) REFERENCES driving_sessions(id) ON DELETE SET NULL
);
CREATE INDEX idx_jobs_completed_at ON jobs(completed_at);
CREATE INDEX idx_jobs_status       ON jobs(status);

CREATE TABLE trucks (
  id                       TEXT PRIMARY KEY,    -- hash of (make, model)
  make                     TEXT NOT NULL,
  model                    TEXT NOT NULL,
  first_seen_at            TEXT NOT NULL,
  total_distance_km        REAL NOT NULL DEFAULT 0,   -- all driving
  total_delivery_distance  REAL NOT NULL DEFAULT 0,
  total_free_roam_distance REAL NOT NULL DEFAULT 0,
  total_jobs               INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
