import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Truck as TruckIcon } from "lucide-react";

import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

import { api } from "@/lib/api";
import { formatKm, formatRelativeDate } from "@/lib/format";
import type { Truck } from "@/types";

export function Trucks() {
  const navigate = useNavigate();
  const [trucks, setTrucks] = useState<Truck[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .listTrucks()
      .then((t) => {
        if (!cancelled) setTrucks(t);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (error) {
    return (
      <div className="space-y-4">
        <PageHeader count={null} />
        <p className="text-destructive text-sm">Error: {error}</p>
      </div>
    );
  }

  if (!trucks) {
    return (
      <div className="space-y-4">
        <PageHeader count={null} />
        <p className="text-muted-foreground text-sm">Loading…</p>
      </div>
    );
  }

  if (trucks.length === 0) {
    return (
      <div className="space-y-4">
        <PageHeader count={0} />
        <div className="rounded-md border border-dashed p-10 text-center">
          <TruckIcon className="text-muted-foreground mx-auto mb-3 h-8 w-8" />
          <h3 className="text-lg font-medium">No trucks yet</h3>
          <p className="text-muted-foreground mt-1 text-sm">
            Log a job (or capture telemetry in M2) and the truck will appear
            here.
          </p>
        </div>
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
      <p className="text-muted-foreground text-sm">
        {count == null
          ? "Loading…"
          : count === 1
            ? "1 truck driven"
            : `${count} trucks driven`}
      </p>
    </div>
  );
}
