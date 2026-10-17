import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import { useTelemetryStatus } from "@/lib/useTelemetryEvents";
import type { DbPing } from "@/types";

export function Dashboard() {
  const [ping, setPing] = useState<DbPing | null>(null);
  const [error, setError] = useState<string | null>(null);
  const telemetry = useTelemetryStatus();

  useEffect(() => {
    api
      .dbPing()
      .then(setPing)
      .catch((e: unknown) => setError(String(e)));
  }, []);

  const status = error
    ? `error: ${error}`
    : ping
      ? ping.message
      : "checking...";
  const tone = error
    ? "text-destructive"
    : ping?.ok
      ? "text-emerald-600 dark:text-emerald-400"
      : "text-muted-foreground";

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <h2 className="text-2xl font-semibold tracking-tight">Dashboard</h2>
        <p className="text-muted-foreground">
          Lifetime stats, recent jobs, and trend charts will live here.
        </p>
      </div>
      <p className={`text-sm font-mono ${tone}`}>db_ping: {status}</p>
      <p className="text-sm font-mono text-muted-foreground">
        telemetry: {telemetry}
      </p>
    </div>
  );
}
