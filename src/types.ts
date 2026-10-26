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
  /** Only sessions on this truck (send both). */
  truckMake?: string | null;
  truckModel?: string | null;
  from?: IsoDateTime | null;
  to?: IsoDateTime | null;
}

export interface JobFilter {
  limit?: number | null;
  offset?: number | null;
  status?: JobStatus | null;
  search?: string | null;
  /** Only jobs on this truck (send both). */
  truckMake?: string | null;
  truckModel?: string | null;
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
  pluginPromptDismissed: string | null;
  /** Check GitHub for a newer release when the app starts (default on). */
  checkUpdates: boolean;
}

export type PluginState = "noFolder" | "badFolder" | "notInstalled" | "outdated" | "installed";

export interface PluginStatus {
  state: PluginState;
  pluginPath: string | null;
  bundledSha: string | null;
}

/** Fields to change; leave one out to keep it. An empty path clears it. */
export interface SettingsPatch {
  autoCapture?: boolean;
  ets2InstallPath?: string;
  units?: "km" | "mi";
  pluginPromptDismissed?: string;
  checkUpdates?: boolean;
}

// --- App updates (src-tauri/src/updater.rs) ------------------------------

export type UpdateState =
  | "disabled"
  | "idle"
  | "checking"
  | "upToDate"
  | "available"
  | "downloading"
  | "installing"
  | "error";

export interface LatestRelease {
  /** Without the leading "v". */
  version: string;
  pageUrl: string;
  /** The app can download and run the installer itself (Windows); otherwise link to the page. */
  canInstall: boolean;
}

export interface UpdateStatus {
  currentVersion: string;
  state: UpdateState;
  latest: LatestRelease | null;
  received: number;
  total: number;
  /** The error for "error", the reason for "disabled". */
  message: string | null;
}

// --- Dashboard (M3.1) ----------------------------------------------------

/** Totals for one period. Mirrors `commands::stats::PeriodStats`. */
export interface PeriodStats {
  totalKm: number;
  deliveryKm: number;
  freeRoamKm: number;
  /** Delivered (on time or late); cancelled jobs are not deliveries. */
  deliveries: number;
  income: number;
  onTime: number;
  late: number;
}

/** One local day of the distance series (`day` is YYYY-MM-DD). */
export interface DistanceDay {
  day: string;
  deliveryKm: number;
  freeRoamKm: number;
}

export interface DashboardStats {
  lifetime: PeriodStats;
  thisMonth: PeriodStats;
  lastMonth: PeriodStats;
  /** 30 entries, oldest first, ending today. */
  last30Days: DistanceDay[];
  recentJobs: Job[];
}

export interface DashboardQuery {
  /** Minutes east of UTC (`-new Date().getTimezoneOffset()`). */
  utcOffsetMinutes?: number;
  now?: IsoDateTime;
}

// --- Truck detail (M3.2) -------------------------------------------------

/** One truck's figures. Mirrors `commands::stats::TruckStats`. */
export interface TruckStats {
  truck: Truck;
  /** Everything driven on this truck; `totalKm` equals the Trucks list. */
  lifetime: PeriodStats;
  /** Jobs of any status, the Trucks list's count. */
  totalJobs: number;
  /** Mean job distance (the per-row figure in the Jobs table); null with no jobs. */
  avgJobKm: number | null;
  /** 30 entries, oldest first, ending today. */
  last30Days: DistanceDay[];
}
