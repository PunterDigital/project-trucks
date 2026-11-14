import { Truck } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { formatCurrency, formatDuration, formatKm } from "@/lib/format";
import type { CurrentJob } from "@/types";

const tonnes = new Intl.NumberFormat("en-GB", { maximumFractionDigits: 1 });

function place(city: string | null, company: string | null): string {
  if (city && company) return `${city} (${company})`;
  return city ?? company ?? "Unknown";
}

/** "3h 20m left" / "45m late", from game minutes. */
function deadlineText(minutes: number): string {
  if (minutes >= 0) return `${formatDuration(minutes * 60)} left`;
  return `${formatDuration(-minutes * 60)} late`;
}

/**
 * The delivery on the truck right now (#4620): cargo, route, what the route advisor says is
 * left and how far along it is. Fed by the `telemetry:job` event; renders nothing without a job.
 */
export function JobInProgress({ job }: { job: CurrentJob | null }) {
  if (!job) return null;
  const pct = job.progressPct === null ? null : Math.round(job.progressPct);
  const late = job.minutesToDeadline !== null && job.minutesToDeadline < 0;

  return (
    <Card data-testid="job-in-progress">
      <CardHeader className="flex flex-row items-center justify-between gap-2 space-y-0">
        <CardTitle className="flex items-center gap-2 text-base">
          <Truck className="size-4" aria-hidden />
          Job in progress
        </CardTitle>
        {job.parked ? (
          <Badge variant="secondary" data-testid="job-parked">
            Parked
          </Badge>
        ) : (
          <Badge data-testid="job-driving">Driving</Badge>
        )}
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-1">
          <p className="text-lg font-semibold" data-testid="job-cargo">
            {job.cargoName ?? "Cargo"}
            {job.cargoMassKg > 0 && (
              <span className="text-muted-foreground ml-2 text-sm font-normal">
                {tonnes.format(job.cargoMassKg / 1000)} t
              </span>
            )}
          </p>
          <p className="text-muted-foreground text-sm" data-testid="job-route">
            {place(job.sourceCity, job.sourceCompany)} →{" "}
            {place(job.destinationCity, job.destinationCompany)}
          </p>
        </div>

        <div className="space-y-1.5">
          <div className="flex items-baseline justify-between text-sm">
            <span className="text-muted-foreground">Completion</span>
            <span className="font-medium tabular-nums" data-testid="job-progress-label">
              {pct === null ? "No route set" : `${pct}%`}
            </span>
          </div>
          <div
            className="bg-muted h-2 overflow-hidden rounded-full"
            role="progressbar"
            aria-label="Delivery completion"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={pct ?? undefined}
            data-testid="job-progress"
          >
            <div
              className="bg-primary h-full rounded-full transition-[width] duration-700"
              style={{ width: `${pct ?? 0}%` }}
            />
          </div>
        </div>

        <dl className="grid grid-cols-2 gap-x-4 gap-y-3 text-sm sm:grid-cols-4">
          <div>
            <dt className="text-muted-foreground text-xs">Distance left</dt>
            <dd className="font-medium tabular-nums" data-testid="job-distance-left">
              {job.distanceLeftKm === null ? "–" : formatKm(job.distanceLeftKm)}
            </dd>
          </div>
          <div>
            <dt className="text-muted-foreground text-xs">Time left (game)</dt>
            <dd className="font-medium tabular-nums" data-testid="job-time-left">
              {job.routeMinutesLeft === null ? "–" : formatDuration(job.routeMinutesLeft * 60)}
            </dd>
          </div>
          <div>
            <dt className="text-muted-foreground text-xs">Deadline</dt>
            <dd
              className={late ? "text-destructive font-medium" : "font-medium"}
              data-testid="job-deadline"
            >
              {job.minutesToDeadline === null ? "–" : deadlineText(job.minutesToDeadline)}
            </dd>
          </div>
          <div>
            <dt className="text-muted-foreground text-xs">Pay</dt>
            <dd className="font-medium tabular-nums" data-testid="job-income">
              {formatCurrency(job.income)}
            </dd>
          </div>
        </dl>
        <p className="text-muted-foreground text-xs" data-testid="job-driven">
          Driven so far: {formatKm(job.drivenKm)}
        </p>
      </CardContent>
    </Card>
  );
}
