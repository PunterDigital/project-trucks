import { Link } from "react-router-dom";

import { useSettings } from "@/lib/settings";
import { trackingState, type TrackingState } from "@/lib/tracking";
import { useTelemetryStatus, type TelemetryStatus } from "@/lib/useTelemetryEvents";
import { cn } from "@/lib/utils";

const LABELS: Record<TrackingState, string> = {
  off: "Not tracking",
  waiting: "Waiting for ETS2",
  tracking: "Tracking",
};

const TRACKING_DETAIL: Record<Exclude<TelemetryStatus, "not_connected">, string> = {
  delivery: "Recording a delivery",
  free_roam: "Recording a drive (free roam)",
  parked: "Game connected, parked",
};

function detail(state: TrackingState, status: TelemetryStatus): string {
  if (state === "off") return "Auto-capture is off. Click to turn it on in Settings.";
  if (state === "waiting" || status === "not_connected")
    return "Auto-capture is on. Start ETS2 with the telemetry plugin installed.";
  return TRACKING_DETAIL[status];
}

export function Header() {
  const settings = useSettings();
  const status = useTelemetryStatus();
  const on = settings?.autoCapture ?? false;
  const state = trackingState(on, status);
  const title = detail(state, status);

  return (
    <header className="flex h-14 items-center justify-between border-b px-6">
      <h1 className="text-base font-semibold tracking-tight">ProjectTrucks</h1>
      <Link
        to="/settings"
        title={title}
        aria-label={`${LABELS[state]}: ${title}`}
        data-testid="auto-capture-pill"
        data-capture={on ? "on" : "off"}
        data-state={state}
        className={cn(
          "inline-flex items-center gap-2 rounded-full border px-3 py-1 text-xs font-medium transition-colors",
          "focus-visible:ring-ring/50 outline-none focus-visible:ring-[3px]",
          state === "tracking" &&
            "border-emerald-500/40 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
          state === "waiting" &&
            "border-amber-500/40 bg-amber-500/10 text-amber-700 dark:text-amber-300",
          state === "off" && "bg-secondary text-secondary-foreground",
        )}
      >
        <span className="relative flex size-2" aria-hidden>
          {state === "tracking" && (
            <span className="absolute inline-flex size-full animate-ping rounded-full bg-emerald-500 opacity-75" />
          )}
          <span
            className={cn(
              "relative inline-flex size-2 rounded-full",
              state === "tracking" && "bg-emerald-500",
              state === "waiting" && "bg-amber-500",
              state === "off" && "bg-muted-foreground/50",
            )}
          />
        </span>
        <span data-testid="tracking-label">{LABELS[state]}</span>
      </Link>
    </header>
  );
}
