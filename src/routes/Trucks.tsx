import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { Truck as TruckIcon } from "lucide-react";

import { EmptyState } from "@/components/EmptyState";
import { LoadError } from "@/components/LoadError";
import { TableSkeleton, TextSkeleton } from "@/components/Skeletons";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

import { api } from "@/lib/api";
import { useCaptureRefresh } from "@/lib/useTelemetryEvents";
import { formatKm, formatRelativeDate } from "@/lib/format";
import type { Truck } from "@/types";

export function Trucks() {
  const navigate = useNavigate();
  const [trucks, setTrucks] = useState<Truck[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const captured = useCaptureRefresh();

  useEffect(() => {
    let cancelled = false;
    api
      .listTrucks()
      .then((t) => {
        if (cancelled) return;
        setTrucks(t);
        setError(null);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [captured, attempt]);

  if (error) {
    return (
      <div className="space-y-4">
        <PageHeader count={null} />
        <LoadError
          what="your trucks"
          message={error}
          onRetry={() => setAttempt((a) => a + 1)}
        />
      </div>
    );
  }

  if (!trucks) {
    return (
      <div className="space-y-4">
        <PageHeader count={null} />
        <TableSkeleton rows={4} cols={4} />
      </div>
    );
  }

  if (trucks.length === 0) {
    return (
      <div className="space-y-4">
        <PageHeader count={0} />
        <EmptyState
          icon={TruckIcon}
          title="No trucks yet"
          description="A truck appears here with its first job or recorded drive, and keeps a running total of its kilometres."
        >
          <Button asChild>
            <Link to="/jobs/new">Log a job</Link>
          </Button>
          <Button variant="outline" asChild>
            <Link to="/settings">Turn on auto-capture</Link>
          </Button>
        </EmptyState>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <PageHeader count={trucks.length} />
      <div className="overflow-hidden rounded-md border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Truck</TableHead>
              <TableHead>First seen</TableHead>
              <TableHead className="text-right">Trips</TableHead>
              <TableHead className="text-right">Distance</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {trucks.map((t) => (
              <TableRow
                key={t.id}
                className="hover:bg-accent/50 cursor-pointer"
                onClick={() => navigate(`/trucks/${t.id}`)}
              >
                <TableCell>
                  <div className="font-medium">{t.make}</div>
                  <div className="text-muted-foreground text-xs">
                    {t.model}
                  </div>
                </TableCell>
                <TableCell className="whitespace-nowrap text-sm">
                  {formatRelativeDate(t.firstSeenAt)}
                </TableCell>
                <TableCell className="whitespace-nowrap text-right tabular-nums">
                  {t.totalJobs}
                </TableCell>
                <TableCell className="whitespace-nowrap text-right">
                  <div className="tabular-nums">
                    {formatKm(t.totalDistanceKm)}
                  </div>
                  <div className="text-muted-foreground text-xs tabular-nums">
                    {formatKm(t.totalDeliveryDistance)} delivery ·{" "}
                    {formatKm(t.totalFreeRoamDistance)} free roam
                  </div>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
    </div>
  );
}

function PageHeader({ count }: { count: number | null }) {
  return (
    <div className="space-y-1">
      <h2 className="text-2xl font-semibold tracking-tight">Trucks</h2>
      {count == null ? (
        <TextSkeleton className="mt-1" />
      ) : (
        <p className="text-muted-foreground text-sm">
          {count === 1 ? "1 truck driven" : `${count} trucks driven`}
        </p>
      )}
    </div>
  );
}
