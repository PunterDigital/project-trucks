// Thin typed wrapper around Tauri's `invoke`. Every command our React tree
// calls goes through here, so the rest of the frontend never imports
// `@tauri-apps/api/core` directly. This is the single seam between the UI and
// the Rust backend.

import { invoke } from "@tauri-apps/api/core";

import type {
  DbPing,
  DrivingSession,
  Job,
  JobFilter,
  JobPatch,
  JobsPage,
  NewJob,
  NewSession,
  SessionFilter,
  SessionsPage,
} from "@/types";

export const api = {
  // -- Health / misc ----
  dbPing: () => invoke<DbPing>("db_ping"),

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
};
