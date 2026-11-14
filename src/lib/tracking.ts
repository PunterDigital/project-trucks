import type { TelemetryStatus } from "@/lib/useTelemetryEvents";

/** What the header's telemetry indicator shows (#4621). */
export type TrackingState = "off" | "waiting" | "tracking";

export function trackingState(autoCapture: boolean, status: TelemetryStatus): TrackingState {
  if (!autoCapture) return "off";
  return status === "not_connected" ? "waiting" : "tracking";
}
