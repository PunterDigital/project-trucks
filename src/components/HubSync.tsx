// Where a job stands with the ProjectTrucks Hub (#4654): a small badge for the Jobs list and a
// panel for the job page with "Hide on Hub". Nothing shows while the app isn't linked.

import { useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { api } from "@/lib/api";
import { SYNC_LABELS, publishJobHubState, useHubStatus, useJobHubStates } from "@/lib/hub";
import { cn } from "@/lib/utils";
import type { JobHubState } from "@/types";

const BADGE_CLASS: Record<JobHubState["state"], string> = {
  synced: "border-transparent bg-sky-100 text-sky-800 dark:bg-sky-950 dark:text-sky-300",
  pending: "border-transparent bg-secondary text-secondary-foreground",
  refused: "border-transparent bg-red-100 text-red-800 dark:bg-red-950 dark:text-red-300",
  local: "",
  notSynced: "",
};

export function HubSyncBadge({ state }: { state: JobHubState | undefined }) {
  // Only what's worth a glance in a list: on the Hub, on its way, refused.
  if (!state || state.state === "local" || state.state === "notSynced") return null;
  const title =
    state.state === "refused"
      ? `The Hub refused this job: ${state.error ?? "no reason given"}`
      : state.state === "synced" && state.hidden
        ? "On the Hub, hidden from everyone but you"
        : SYNC_LABELS[state.state];
  return (
    <Badge
      data-testid="hub-sync-badge"
      data-state={state.state}
      title={title}
      className={cn(BADGE_CLASS[state.state])}
    >
      {state.state === "synced"
        ? state.hidden
          ? "Hub ✓ hidden"
          : "Hub ✓"
        : SYNC_LABELS[state.state]}
    </Badge>
  );
}

/** Job page: sync state, a link to the Hub's page and the "Hide on Hub" switch. */
export function JobHubPanel({ jobId }: { jobId: string }) {
  const status = useHubStatus();
  const states = useJobHubStates([jobId]);
  const state = states.get(jobId);
  const [busy, setBusy] = useState(false);

  if (!status?.linked || !state) return null;

  const setHidden = (hidden: boolean) => {
    setBusy(true);
    api
      .hubSetHidden(jobId, hidden)
      .then((next) => {
        publishJobHubState(next);
        toast.success(hidden ? "Hidden on the Hub" : "Shown on the Hub");
      })
      .catch((e) => toast.error(String(e)))
      .finally(() => setBusy(false));
  };

  return (
    <div className="space-y-3 text-sm" data-testid="job-hub-panel" data-state={state.state}>
      <div className="flex flex-wrap items-center gap-3">
        <span data-testid="job-hub-state">{describe(state)}</span>
        {state.url && (
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => api.hubOpenJob(jobId).catch((e) => toast.error(String(e)))}
          >
            Open on the Hub
          </Button>
        )}
      </div>
      {state.url && (
        <label className="flex items-center gap-2">
          <Checkbox
            aria-label="Hide on Hub"
            checked={state.hidden}
            disabled={busy}
            onCheckedChange={(v) => setHidden(v === true)}
          />
          <span>Hide on Hub</span>
          <span className="text-muted-foreground">
            Only you see a hidden job there, and it doesn&apos;t count anywhere.
          </span>
        </label>
      )}
    </div>
  );
}

function describe(state: JobHubState): string {
  switch (state.state) {
    case "synced":
      return state.hidden ? "On the Hub (hidden)." : "On the Hub.";
    case "pending":
      return "Waiting to sync.";
    case "refused":
      return `The Hub refused this job: ${state.error ?? "no reason given"}. Retry from Settings → ProjectTrucks Hub.`;
    case "local":
      return "Logged by hand, so it stays on this PC.";
    case "notSynced":
      return "Not on the Hub: recorded before this app was linked. Use Sync history in Settings to send it.";
  }
}
