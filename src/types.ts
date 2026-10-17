// Wire-format types for the Tauri command surface.
//
// These mirror src-tauri/src/db/models.rs by hand. Rust uses snake_case for
// struct fields; serde renames everything to camelCase, so that's what the
// frontend sees. The enums serialize as SCREAMING_SNAKE_CASE strings.
//
// Keep this file in lockstep with the Rust models. If we add or rename a field
// there, update it here in the same change.

// --- Enums ---------------------------------------------------------------

export type SessionKind = "FREE_ROAM" | "JOB";
export type SessionStatus = "COMPLETED" | "CANCELLED";
export type JobStatus = "DELIVERED" | "LATE" | "CANCELLED";
export type JobSource = "TELEMETRY" | "MANUAL";

// Dates come across as ISO 8601 strings. We keep them as strings on the TS side
// and parse to Date only where we need to format/compare.
export type IsoDateTime = string;

// --- Stored rows ---------------------------------------------------------

export interface DrivingSession {
  id: string;
  kind: SessionKind;
  truckMake: string | null;
  truckModel: string | null;
  startOdometer: number;
  endOdometer: number;
  distanceKm: number;
  startedAt: IsoDateTime;
  endedAt: IsoDateTime;
  status: SessionStatus;
}

export interface Job {
  id: string;
  sessionId: string | null;
  truckMake: string | null;
  truckModel: string | null;
  sourceCity: string;
  sourceCompany: string;
  destCity: string;
  destCompany: string;
  cargoId: string;
  cargoName: string;
  cargoMassKg: number;
  declaredIncome: number;
  distanceKm: number;
  durationSec: number;
  avgSpeedKmh: number;
  fuelUsedL: number | null;
  damageDelta: number | null;
  startedAt: IsoDateTime;
  completedAt: IsoDateTime;
  status: JobStatus;
  source: JobSource;
  flagged: boolean;
  flagReason: string | null;
  notes: string | null;
}

export interface Truck {
  id: string;
  make: string;
  model: string;
  firstSeenAt: IsoDateTime;
  totalDistanceKm: number;
  totalDeliveryDistance: number;
  totalFreeRoamDistance: number;
  totalJobs: number;
}

// --- Input shapes (sent to write commands) -------------------------------

export interface NewSession {
  kind: SessionKind;
  truckMake: string | null;
  truckModel: string | null;
  startOdometer: number;
  endOdometer: number;
  startedAt: IsoDateTime;
  endedAt: IsoDateTime;
  status: SessionStatus;
}

export interface NewJob {
  sessionId: string | null;
  truckMake: string | null;
  truckModel: string | null;
  sourceCity: string;
  sourceCompany: string;
  destCity: string;
  destCompany: string;
  cargoId: string;
  cargoName: string;
  cargoMassKg: number;
  declaredIncome: number;
  distanceKm: number;
  durationSec: number;
  fuelUsedL: number | null;
  damageDelta: number | null;
  startedAt: IsoDateTime;
  completedAt: IsoDateTime;
  status: JobStatus;
  source: JobSource;
  notes: string | null;
}

// --- Filter / page / patch shapes ---------------------------------------

export interface SessionFilter {
  limit?: number | null;
  offset?: number | null;
  kind?: SessionKind | null;
  from?: IsoDateTime | null;
  to?: IsoDateTime | null;
}

export interface JobFilter {
  limit?: number | null;
  offset?: number | null;
  status?: JobStatus | null;
  search?: string | null;
  from?: IsoDateTime | null;
  to?: IsoDateTime | null;
}

export interface SessionsPage {
  items: DrivingSession[];
  total: number;
}

export interface JobsPage {
  items: Job[];
  total: number;
}

export interface JobPatch {
  /** `undefined` = leave alone. `""` = clear. */
  notes?: string;
  flagged?: boolean;
  /** `undefined` = leave alone. `""` = clear. */
  flagReason?: string;
}

// --- Misc ----------------------------------------------------------------

export interface DbPing {
  ok: boolean;
  message: string;
}

// --- Settings ------------------------------------------------------------

export interface Settings {
  autoCapture: boolean;
  ets2InstallPath: string | null;
  units: "km" | "mi";
}

/** Fields to change; leave one out to keep it. An empty path clears it. */
export interface SettingsPatch {
  autoCapture?: boolean;
  ets2InstallPath?: string;
  units?: "km" | "mi";
}
