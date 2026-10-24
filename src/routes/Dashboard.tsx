import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { Gauge } from "lucide-react";

import { DistanceChart, OnTimeDonut } from "@/components/charts";
import { EmptyState } from "@/components/EmptyState";
import { JobStatusBadge } from "@/components/JobStatusBadge";
import { LoadError } from "@/components/LoadError";
import { StatsPageSkeleton } from "@/components/Skeletons";
import { StatCard, type StatDelta } from "@/components/StatCard";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

import { api } from "@/lib/api";
import { formatCurrency, formatKm, formatRelativeDate } from "@/lib/format";
import {
  useCaptureRefresh,
  useTelemetryStatus,
  type TelemetryStatus,
} from "@/lib/useTelemetryEvents";
import type { DashboardStats, PeriodStats } from "@/types";

const TELEMETRY_LABELS: Record<TelemetryStatus, string> = {
  not_connected: "ETS2 not connected",
  parked: "Parked",
  free_roam: "Driving (free roam)",
  delivery: "On a delivery",
};

function onTimePct(p: PeriodStats): number | null {
  const total = p.onTime + p.late;
  return total === 0 ? null : Math.round((p.onTime / total) * 100);
}

function formatPct(pct: number | null): string {
  return pct === null ? "–" : `${pct}%`;
}

const plain = new Intl.NumberFormat("en-GB", { maximumFractionDigits: 0 });

function signed(n: number, format: (n: number) => string): string {
  const s = format(Math.abs(n));
  return n > 0 ? `+${s}` : n < 0 ? `-${s}` : s;
}

/** Change of a this-month number against last month. */
function delta(
  current: number,
  previous: number,
  format: (n: number) => string = (n) => plain.format(n),
): StatDelta {
  const value = current - previous;
  return { value, label: signed(value, format), against: "vs last month" };
}

function pctDelta(current: PeriodStats, previous: PeriodStats): StatDelta | undefined {
  const now = onTimePct(current);
  const before = onTimePct(previous);
  if (now === null || before === null) return undefined;
  return delta(now, before, (n) => `${n} pts`);
}

export function Dashboard() {
  const navigate = useNavigate();
  const telemetry = useTelemetryStatus();
  const captured = useCaptureRefresh();
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let cancelled = false;
    api
      .getDashboardStats({ utcOffsetMinutes: -new Date().getTimezoneOffset() })
      .then((s) => {
        if (cancelled) return;
        setStats(s);
        setError(null);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [captured, attempt]);

  return (
    <div className="space-y-6">
      <div className="space-y-1">
        <h2 className="text-2xl font-semibold tracking-tight">Dashboard</h2>
        <p className="text-muted-foreground text-sm" data-testid="telemetry-status">
          {TELEMETRY_LABELS[telemetry]}
        </p>
      </div>

      {error ? (
        <LoadError
          what="the dashboard"
          message={error}
          onRetry={() => setAttempt((a) => a + 1)}
        />
      ) : !stats ? (
        <StatsPageSkeleton title={false} />
      ) : stats.lifetime.totalKm === 0 && stats.recentJobs.length === 0 ? (
        <EmptyDashboard />
      ) : (
        <DashboardBody stats={stats} onOpenJob={(id) => navigate(`/jobs/${id}`)} />
      )}
    </div>
  );
}

function EmptyDashboard() {
  return (
    <EmptyState
      icon={Gauge}
      title="Nothing tracked yet"
      description="Turn on auto-capture and drive in ETS2, or log a delivery by hand. Your kilometres, deliveries and on-time rate will show up here."
    >
      <Button asChild>
        <Link to="/jobs/new">Log a job</Link>
      </Button>
      <Button variant="outline" asChild>
        <Link to="/settings">Turn on auto-capture</Link>
      </Button>
    </EmptyState>
  );
}

function DashboardBody({
  stats,
  onOpenJob,
}: {
  stats: DashboardStats;
  onOpenJob: (id: string) => void;
}) {
  const { lifetime, thisMonth, lastMonth } = stats;
  return (
    <>
      <section className="space-y-3" aria-labelledby="lifetime-heading">
        <h3 id="lifetime-heading" className="text-sm font-medium">
          Lifetime
        </h3>
        <div className="grid gap-3 md:grid-cols-3 lg:grid-cols-5">
          <StatCard
            hero
            label="Total km driven"
            value={formatKm(lifetime.totalKm)}
            sub={`${formatKm(lifetime.deliveryKm)} delivery · ${formatKm(lifetime.freeRoamKm)} free roam`}
          />
          <StatCard label="Deliveries" value={plain.format(lifetime.deliveries)} />
          <StatCard label="Income" value={formatCurrency(lifetime.income)} />
          <StatCard
            label="On time"
            value={formatPct(onTimePct(lifetime))}
            sub={`${plain.format(lifetime.onTime)} on time · ${plain.format(lifetime.late)} late`}
          />
        </div>
      </section>

      <section className="space-y-3" aria-labelledby="month-heading">
        <h3 id="month-heading" className="text-sm font-medium">
          This month
        </h3>
        <div className="grid gap-3 md:grid-cols-2 lg:grid-cols-4">
          <StatCard
            label="Distance"
            value={formatKm(thisMonth.totalKm)}
            sub={`${formatKm(thisMonth.deliveryKm)} delivery · ${formatKm(thisMonth.freeRoamKm)} free roam`}
            delta={delta(thisMonth.totalKm, lastMonth.totalKm, formatKm)}
          />
          <StatCard
            label="Deliveries"
            value={plain.format(thisMonth.deliveries)}
            delta={delta(thisMonth.deliveries, lastMonth.deliveries)}
          />
          <StatCard
            label="Income"
            value={formatCurrency(thisMonth.income)}
            delta={delta(thisMonth.income, lastMonth.income, formatCurrency)}
          />
          <StatCard
            label="On time"
            value={formatPct(onTimePct(thisMonth))}
            delta={pctDelta(thisMonth, lastMonth)}
          />
        </div>
      </section>

      <div className="grid gap-3 lg:grid-cols-3">
        <Card size="sm" className="lg:col-span-2">
          <CardHeader>
            <CardTitle>Distance, last 30 days</CardTitle>
          </CardHeader>
          <CardContent>
            <DistanceChart days={stats.last30Days} />
          </CardContent>
        </Card>
        <Card size="sm">
          <CardHeader>
            <CardTitle>On-time deliveries</CardTitle>
          </CardHeader>
          <CardContent>
            <OnTimeDonut onTime={lifetime.onTime} late={lifetime.late} />
          </CardContent>
        </Card>
      </div>

      <section className="space-y-3" aria-labelledby="recent-heading">
        <div className="flex items-center justify-between">
          <h3 id="recent-heading" className="text-sm font-medium">
            Recent jobs
          </h3>
          <Button variant="ghost" size="sm" asChild>
            <Link to="/jobs">All jobs</Link>
          </Button>
        </div>
        {stats.recentJobs.length === 0 ? (
          <p className="text-muted-foreground text-sm">
            No deliveries yet.{" "}
            <Link to="/jobs/new" className="underline underline-offset-4">
              Log one by hand
            </Link>
            .
          </p>
        ) : (
          <div className="overflow-hidden rounded-md border">
            <Table data-testid="recent-jobs">
              <TableHeader>
                <TableRow>
                  <TableHead>Route</TableHead>
                  <TableHead>Cargo</TableHead>
                  <TableHead className="text-right">Distance</TableHead>
                  <TableHead className="text-right">Income</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Completed</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {stats.recentJobs.map((job) => (
                  <TableRow
                    key={job.id}
                    className="hover:bg-accent/50 cursor-pointer"
                    onClick={() => onOpenJob(job.id)}
                  >
                    <TableCell>
                      <div className="font-medium">
                        {job.sourceCity} → {job.destCity}
                      </div>
                      <div className="text-muted-foreground text-xs">
                        {job.sourceCompany} → {job.destCompany}
                      </div>
                    </TableCell>
                    <TableCell>{job.cargoName}</TableCell>
                    <TableCell className="text-right tabular-nums">
                      {formatKm(job.distanceKm)}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {formatCurrency(job.declaredIncome)}
                    </TableCell>
                    <TableCell>
                      <JobStatusBadge status={job.status} />
                    </TableCell>
                    <TableCell className="whitespace-nowrap text-sm">
                      {formatRelativeDate(job.completedAt)}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </section>
    </>
  );
}
