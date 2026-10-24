import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { ArrowLeft, Truck as TruckIcon } from "lucide-react";

import { DistanceChart } from "@/components/charts";
import { JobsTable, Pager } from "@/components/JobsTable";
import { StatCard } from "@/components/StatCard";
import { Badge } from "@/components/ui/badge";
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

import { api } from "@/lib/api";
import { formatDate, formatDuration, formatKm, formatRelativeDate } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useCaptureRefresh } from "@/lib/useTelemetryEvents";
import type {
  DrivingSession,
  JobsPage,
  SessionKind,
  SessionsPage,
  SessionStatus,
  Truck,
  TruckStats,
} from "@/types";

const PAGE_SIZE = 25;

const plain = new Intl.NumberFormat("en-GB", { maximumFractionDigits: 0 });

type Load =
  | { kind: "loading" }
  | { kind: "missing" }
  | { kind: "error"; message: string }
  | { kind: "ready"; stats: TruckStats };

export function TruckDetail() {
  const { id } = useParams<{ id: string }>();
  const captured = useCaptureRefresh();
  const [load, setLoad] = useState<Load>({ kind: "loading" });

  // A new id shows the loading state at once (during render, not in the effect), so the
  // previous truck's numbers are never shown under the new id.
  const [loadedId, setLoadedId] = useState(id);
  if (loadedId !== id) {
    setLoadedId(id);
    setLoad({ kind: "loading" });
  }

  useEffect(() => {
    if (!id) return;
    let cancelled = false;
    api
      .getTruckStats(id, { utcOffsetMinutes: -new Date().getTimezoneOffset() })
      .then((stats) => {
        if (cancelled) return;
        setLoad(stats ? { kind: "ready", stats } : { kind: "missing" });
      })
      .catch((e: unknown) => {
        if (!cancelled) setLoad({ kind: "error", message: String(e) });
      });
    return () => {
      cancelled = true;
    };
  }, [id, captured]);

  return (
    <div className="space-y-6">
      <Button variant="ghost" size="sm" asChild>
        <Link to="/trucks">
          <ArrowLeft className="mr-2 h-4 w-4" />
          Back to trucks
        </Link>
      </Button>

      {load.kind === "loading" && (
        <>
          <h2 className="text-2xl font-semibold tracking-tight">Truck</h2>
          <p className="text-muted-foreground text-sm">Loading…</p>
        </>
      )}
      {load.kind === "error" && (
        <>
          <h2 className="text-2xl font-semibold tracking-tight">Truck</h2>
          <p className="text-destructive text-sm">Error: {load.message}</p>
        </>
      )}
      {load.kind === "missing" && <MissingTruck />}
      {load.kind === "ready" && <TruckBody stats={load.stats} captured={captured} />}
    </div>
  );
}

function MissingTruck() {
  return (
    <div className="rounded-md border border-dashed p-10 text-center">
      <TruckIcon className="text-muted-foreground mx-auto mb-3 h-8 w-8" />
      <h2 className="text-lg font-medium">Truck not found</h2>
      <p className="text-muted-foreground mt-1 text-sm">
        There is no truck with this id. It may have been removed when its last job was deleted.
      </p>
    </div>
  );
}

function onTimePct(onTime: number, late: number): string {
  const total = onTime + late;
  return total === 0 ? "–" : `${Math.round((onTime / total) * 100)}%`;
}

function TruckBody({ stats, captured }: { stats: TruckStats; captured: number }) {
  const { truck, lifetime } = stats;
  return (
    <>
      <div className="space-y-1">
        <h2 className="text-2xl font-semibold tracking-tight">
          {truck.make} {truck.model}
        </h2>
        <p className="text-muted-foreground text-sm">
          First seen {formatRelativeDate(truck.firstSeenAt)}
        </p>
      </div>

      <section className="space-y-3" aria-labelledby="truck-stats-heading">
        <h3 id="truck-stats-heading" className="sr-only">
          Truck stats
        </h3>
        <div className="grid gap-3 md:grid-cols-3 lg:grid-cols-5">
          <StatCard
            hero
            label="Total distance"
            value={formatKm(lifetime.totalKm)}
            sub={`${formatKm(lifetime.deliveryKm)} delivery · ${formatKm(lifetime.freeRoamKm)} free roam`}
          />
          <StatCard label="Jobs" value={plain.format(stats.totalJobs)} />
          <StatCard
            label="Average job distance"
            value={stats.avgJobKm === null ? "–" : formatKm(stats.avgJobKm)}
          />
          <StatCard
            label="On time"
            value={onTimePct(lifetime.onTime, lifetime.late)}
            sub={`${plain.format(lifetime.onTime)} on time · ${plain.format(lifetime.late)} late`}
          />
        </div>
      </section>

      <Card size="sm">
        <CardHeader>
          <CardTitle>Distance, last 30 days</CardTitle>
        </CardHeader>
        <CardContent>
          <DistanceChart days={stats.last30Days} />
        </CardContent>
      </Card>

      <Tabs defaultValue="jobs">
        <TabsList>
          <TabsTrigger value="jobs">Jobs</TabsTrigger>
          <TabsTrigger value="sessions">All sessions</TabsTrigger>
        </TabsList>
        <TabsContent value="jobs">
          <TruckJobs truck={truck} captured={captured} />
        </TabsContent>
        <TabsContent value="sessions">
          <TruckSessions truck={truck} captured={captured} />
        </TabsContent>
      </Tabs>
    </>
  );
}

/** Loading flag derived from which request the last response belongs to (see Jobs.tsx). */
type Settled = { key: string; error: string | null } | null;

function TruckJobs({ truck, captured }: { truck: Truck; captured: number }) {
  const navigate = useNavigate();
  const [page, setPage] = useState(0);
  const [data, setData] = useState<JobsPage | null>(null);
  const [settled, setSettled] = useState<Settled>(null);
  const requestKey = `${truck.id}:${page}:${captured}`;

  useEffect(() => {
    let cancelled = false;
    api
      .listJobs({
        truckMake: truck.make,
        truckModel: truck.model,
        limit: PAGE_SIZE,
        offset: page * PAGE_SIZE,
      })
      .then((p) => {
        if (cancelled) return;
        setData(p);
        setSettled({ key: requestKey, error: null });
      })
      .catch((e: unknown) => {
        if (!cancelled) setSettled({ key: requestKey, error: String(e) });
      });
    return () => {
      cancelled = true;
    };
  }, [truck.id, truck.make, truck.model, page, captured, requestKey]);

  const loading = settled?.key !== requestKey;
  const error = loading ? null : (settled?.error ?? null);

  return (
    <div className="space-y-3" data-testid="truck-jobs">
      {error && <p className="text-destructive text-sm">Error: {error}</p>}
      {!data && loading && <p className="text-muted-foreground text-sm">Loading…</p>}
      {data && data.items.length === 0 && (
        <div className="text-muted-foreground rounded-md border border-dashed p-10 text-center text-sm">
          No jobs on this truck yet. Free-roam kilometres still count towards its distance.
        </div>
      )}
      {data && data.items.length > 0 && (
        <>
          <JobsTable jobs={data.items} onOpen={(id) => navigate(`/jobs/${id}`)} />
          <Pager
            page={page}
            pageSize={PAGE_SIZE}
            total={data.total}
            loading={loading}
            onPage={setPage}
          />
        </>
      )}
    </div>
  );
}

const KIND_LABELS: Record<SessionKind, string> = {
  JOB: "Delivery",
  FREE_ROAM: "Free roam",
};

const KIND_CLASSES: Record<SessionKind, string> = {
  JOB: "border-transparent bg-sky-100 text-sky-800 dark:bg-sky-950 dark:text-sky-300",
  FREE_ROAM: "border-transparent bg-violet-100 text-violet-800 dark:bg-violet-950 dark:text-violet-300",
};

const SESSION_STATUS_LABELS: Record<SessionStatus, string> = {
  COMPLETED: "Completed",
  CANCELLED: "Cancelled",
};

function sessionSeconds(s: DrivingSession): number {
  const ms = new Date(s.endedAt).getTime() - new Date(s.startedAt).getTime();
  return Number.isFinite(ms) ? Math.max(0, Math.round(ms / 1000)) : 0;
}

function TruckSessions({ truck, captured }: { truck: Truck; captured: number }) {
  const [page, setPage] = useState(0);
  const [data, setData] = useState<SessionsPage | null>(null);
  const [settled, setSettled] = useState<Settled>(null);
  const requestKey = `${truck.id}:${page}:${captured}`;

  useEffect(() => {
    let cancelled = false;
    api
      .listSessions({
        truckMake: truck.make,
        truckModel: truck.model,
        limit: PAGE_SIZE,
        offset: page * PAGE_SIZE,
      })
      .then((p) => {
        if (cancelled) return;
        setData(p);
        setSettled({ key: requestKey, error: null });
      })
      .catch((e: unknown) => {
        if (!cancelled) setSettled({ key: requestKey, error: String(e) });
      });
    return () => {
      cancelled = true;
    };
  }, [truck.id, truck.make, truck.model, page, captured, requestKey]);

  const loading = settled?.key !== requestKey;
  const error = loading ? null : (settled?.error ?? null);

  return (
    <div className="space-y-3" data-testid="truck-sessions">
      {error && <p className="text-destructive text-sm">Error: {error}</p>}
      {!data && loading && <p className="text-muted-foreground text-sm">Loading…</p>}
      {data && data.items.length === 0 && (
        <div className="text-muted-foreground rounded-md border border-dashed p-10 text-center text-sm">
          No recorded driving sessions on this truck. Jobs logged by hand have no session.
        </div>
      )}
      {data && data.items.length > 0 && (
        <>
          <div className="overflow-hidden rounded-md border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Ended</TableHead>
                  <TableHead>Kind</TableHead>
                  <TableHead className="text-right">Distance</TableHead>
                  <TableHead className="text-right">Duration</TableHead>
                  <TableHead>Status</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.items.map((s) => (
                  <TableRow key={s.id}>
                    <TableCell className="whitespace-nowrap">
                      <div>{formatDate(s.endedAt)}</div>
                      <div className="text-muted-foreground text-xs">
                        {formatRelativeDate(s.endedAt)}
                      </div>
                    </TableCell>
                    <TableCell>
                      <Badge className={KIND_CLASSES[s.kind]} data-testid="session-kind">
                        {KIND_LABELS[s.kind]}
                      </Badge>
                    </TableCell>
                    <TableCell className="whitespace-nowrap text-right tabular-nums">
                      {formatKm(s.distanceKm)}
                    </TableCell>
                    <TableCell className="whitespace-nowrap text-right tabular-nums">
                      {formatDuration(sessionSeconds(s))}
                    </TableCell>
                    <TableCell>
                      <Badge
                        className={cn(
                          "border-transparent",
                          s.status === "CANCELLED"
                            ? "bg-muted text-muted-foreground"
                            : "bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300",
                        )}
                      >
                        {SESSION_STATUS_LABELS[s.status]}
                      </Badge>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
          <Pager
            page={page}
            pageSize={PAGE_SIZE}
            total={data.total}
            loading={loading}
            onPage={setPage}
          />
        </>
      )}
    </div>
  );
}
