// Thin typed wrapper around Tauri's `invoke`. Every command our React tree
// calls goes through here, so the rest of the frontend never imports
// `@tauri-apps/api/core` directly. This is the single seam between the UI and
// the Rust backend.

import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";

import type {
  DashboardQuery,
  DashboardStats,
  DbPing,
  DrivingSession,
  Job,
  JobFilter,
  JobPatch,
  JobsPage,
  NewJob,
  NewSession,
  SessionFilter,
  PluginStatus,
  SessionsPage,
  Settings,
  SettingsPatch,
  Truck,
  TruckStats,
  UpdateStatus,
} from "@/types";
import type { TelemetryStatus } from "@/lib/useTelemetryEvents";

export const api = {
  // -- Health / misc ----
  dbPing: () => invoke<DbPing>("db_ping"),

  // -- Telemetry ----
  getTelemetryStatus: () => invoke<TelemetryStatus>("get_telemetry_status"),

  // -- Settings ----
  getSettings: () => invoke<Settings>("get_settings"),

  updateSettings: (patch: SettingsPatch) =>
    invoke<Settings>("update_settings", { patch }),

  detectEts2: () => invoke<string | null>("detect_ets2"),

  telemetryPluginStatus: () => invoke<PluginStatus>("telemetry_plugin_status"),

  installTelemetryPlugin: () => invoke<PluginStatus>("install_telemetry_plugin"),

  // -- App updates ----
  getUpdateStatus: () => invoke<UpdateStatus>("get_update_status"),

  checkForUpdates: () => invoke<UpdateStatus>("check_for_updates"),

  /** Downloads the installer, starts it and exits the app (Windows). */
  installUpdate: () => invoke<UpdateStatus>("install_update"),

  openReleasePage: () => invoke<void>("open_release_page"),

  // -- Sessions ----
  createSession: (input: NewSession) =>
    invoke<DrivingSession>("create_session", { input }),

  listSessions: (filter?: SessionFilter) =>
    invoke<SessionsPage>("list_sessions", { filter: filter ?? null }),

  deleteSession: (id: string) => invoke<void>("delete_session", { id }),

  // -- Jobs ----
  createJob: (input: NewJob) => invoke<Job>("create_job", { input }),

  listJobs: (filter?: JobFilter) =>
    invoke<JobsPage>("list_jobs", { filter: filter ?? null }),

  getJob: (id: string) => invoke<Job | null>("get_job", { id }),

  updateJob: (id: string, patch: JobPatch) =>
    invoke<Job | null>("update_job", { id, patch }),

  deleteJob: (id: string) => invoke<void>("delete_job", { id }),

  // -- Trucks ----
  listTrucks: () => invoke<Truck[]>("list_trucks"),

  getTruck: (id: string) => invoke<Truck | null>("get_truck", { id }),

  // -- Dashboard ----
  getDashboardStats: (query?: DashboardQuery) =>
    invoke<DashboardStats>("get_dashboard_stats", { query: query ?? null }),

  getTruckStats: (id: string, query?: DashboardQuery) =>
    invoke<TruckStats | null>("get_truck_stats", { id, query: query ?? null }),

  // -- Export / backup ----
  /** Native "Save as" dialog; `null` when the user cancels. */
  pickSavePath: (options: {
    defaultPath: string;
    filters: { name: string; extensions: string[] }[];
  }) => save(options),

  /** Writes every job to a CSV file at `path`; returns the number of jobs written. */
  exportJobsCsv: (path: string) => invoke<number>("export_jobs_csv", { path }),

  /** Snapshots the database to `path` (VACUUM INTO). */
  backupDatabase: (path: string) => invoke<void>("backup_database", { path }),
};
