// Thin typed wrapper around Tauri's `invoke`. Every command our React tree
// calls goes through here, so the rest of the frontend never imports
// `@tauri-apps/api/core` directly. This is the single seam between the UI and
// the Rust backend.

import { invoke } from "@tauri-apps/api/core";

import type {
  CurrentJob,
  DashboardQuery,
  DashboardStats,
  DbPing,
  DrivingSession,
  HubStatus,
  JobHubState,
  Job,
  JobExpense,
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
  getCurrentJob: () => invoke<CurrentJob | null>("get_current_job"),

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

  // -- ProjectTrucks Hub ----
  getHubStatus: () => invoke<HubStatus>("get_hub_status"),

  /** Starts linking: the status then carries the code to approve on the website. */
  hubLinkStart: () => invoke<HubStatus>("hub_link_start"),

  hubLinkCancel: () => invoke<HubStatus>("hub_link_cancel"),

  hubUnlink: () => invoke<HubStatus>("hub_unlink"),

  /** Queues every older job the Hub doesn't have yet. */
  hubSyncHistory: () => invoke<HubStatus>("hub_sync_history"),

  /** Retries refused jobs and syncs straight away. */
  hubSyncNow: () => invoke<HubStatus>("hub_sync_now"),

  /** Opens the approval page ("approve") or the driver's Hub profile ("profile"). */
  hubOpen: (target: "approve" | "profile") => invoke<void>("hub_open", { target }),

  /** Opens one job's page on the Hub. */
  hubOpenJob: (jobId: string) => invoke<void>("hub_open", { target: "job", jobId }),

  /** Where each job stands with the Hub (empty while unlinked). */
  hubJobStates: (ids: string[]) => invoke<JobHubState[]>("hub_job_states", { ids }),

  /** "Hide on Hub" / "Show on Hub". */
  hubSetHidden: (jobId: string, hidden: boolean) =>
    invoke<JobHubState>("hub_set_hidden", { jobId, hidden }),

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

  /** Tolls, fines and fares paid on a job, oldest first (#4756). */
  getJobExpenses: (jobId: string) =>
    invoke<JobExpense[]>("get_job_expenses", { jobId }),

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
  // The Rust side opens the "Save as" dialog itself and never takes a path from here (#5613).
  /** Asks where, then writes every job as CSV; `null` when the user cancels. */
  exportJobsCsv: () =>
    invoke<{ path: string; count: number } | null>("export_jobs_csv"),

  /** Asks where, then snapshots the database (VACUUM INTO); the saved path, `null` if cancelled. */
  backupDatabase: () => invoke<string | null>("backup_database"),
};
