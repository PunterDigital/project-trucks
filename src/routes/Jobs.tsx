import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import type { DateRange } from "react-day-picker";

import { DateRangePicker } from "@/components/DateRangePicker";
import { JobStatusBadge } from "@/components/JobStatusBadge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
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
import {
  formatCurrency,
  formatDate,
  formatDuration,
  formatKm,
} from "@/lib/format";
import { useDebounce } from "@/lib/useDebounce";
import type { JobFilter, JobsPage, JobStatus } from "@/types";

const PAGE_SIZE = 25;

type StatusFilterValue = JobStatus | "ALL";
const STATUS_OPTIONS: { value: StatusFilterValue; label: string }[] = [
  { value: "ALL", label: "All statuses" },
  { value: "DELIVERED", label: "On time" },
  { value: "LATE", label: "Late" },
  { value: "CANCELLED", label: "Cancelled" },
];

function startOfDayIso(d: Date): string {
  const x = new Date(d);
  x.setHours(0, 0, 0, 0);
  return x.toISOString();
}

function endOfDayExclusiveIso(d: Date): string {
  const x = new Date(d);
  x.setHours(0, 0, 0, 0);
  x.setDate(x.getDate() + 1);
  return x.toISOString();
}

export function Jobs() {
  const navigate = useNavigate();

  const [searchInput, setSearchInput] = useState("");
  const search = useDebounce(searchInput, 300);
  const [status, setStatus] = useState<StatusFilterValue>("ALL");
  const [range, setRange] = useState<DateRange | undefined>(undefined);
  const [page, setPage] = useState(0);

  const [data, setData] = useState<JobsPage | null>(null);
  // The request the last response (or error) belongs to. While it differs
  // from the current request, the list is loading.
  const [settled, setSettled] = useState<{
    key: string;
    error: string | null;
  } | null>(null);

  const noFilters = !search.trim() && status === "ALL" && !range?.from;

  // Filter changes reset to the first page. Done during render (not in an
  // effect) so the old page is never fetched with the new filters.
  const [pageFilters, setPageFilters] = useState({ search, status, range });
  if (
    pageFilters.search !== search ||
    pageFilters.status !== status ||
    pageFilters.range !== range
  ) {
    setPageFilters({ search, status, range });
    setPage(0);
  }

  const filter = useMemo(() => {
    const f: JobFilter = {
      limit: PAGE_SIZE,
      offset: page * PAGE_SIZE,
    };
    if (search.trim()) f.search = search.trim();
    if (status !== "ALL") f.status = status;
    if (range?.from) f.from = startOfDayIso(range.from);
    if (range?.to) f.to = endOfDayExclusiveIso(range.to);
    return f;
  }, [search, status, range, page]);
  const requestKey = JSON.stringify(filter);
  const captured = useCaptureRefresh();

  useEffect(() => {
    let cancelled = false;
    api
      .listJobs(filter)
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
  }, [filter, requestKey, captured]);

  const loading = settled?.key !== requestKey;
  const error = loading ? null : (settled?.error ?? null);

  const empty = !loading && data && data.items.length === 0;
  const showFirstRunCta = empty && noFilters && data.total === 0;
  const showFilteredEmpty = empty && !showFirstRunCta;
  const pageCount = data ? Math.max(1, Math.ceil(data.total / PAGE_SIZE)) : 0;

  const clearFilters = () => {
    setSearchInput("");
    setStatus("ALL");
    setRange(undefined);
  };

  return (
    <div className="space-y-4">
      <div className="flex items-start justify-between gap-4">
        <div className="space-y-1">
          <h2 className="text-2xl font-semibold tracking-tight">Jobs</h2>
          <p className="text-muted-foreground text-sm">
            {data
              ? `${data.total} ${data.total === 1 ? "delivery" : "deliveries"} logged`
              : loading
                ? "Loading…"
                : ""}
          </p>
        </div>
        <Button asChild>
          <Link to="/jobs/new">Log a job</Link>
        </Button>
      </div>

      {/* Filter bar */}
      <div className="flex flex-wrap items-center gap-2">
        <Input
          placeholder="Search city, company, cargo…"
          value={searchInput}
          onChange={(e) => setSearchInput(e.target.value)}
          className="max-w-xs"
        />
        <Select
          value={status}
          onValueChange={(v) => setStatus(v as StatusFilterValue)}
        >
          <SelectTrigger className="w-[180px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {STATUS_OPTIONS.map((o) => (
              <SelectItem key={o.value} value={o.value}>
                {o.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <DateRangePicker value={range} onChange={setRange} />
        {!noFilters && (
          <Button variant="ghost" onClick={clearFilters}>
            Clear filters
          </Button>
        )}
      </div>

      {error && <p className="text-sm text-destructive">Error: {error}</p>}

      {/* States */}
      {showFirstRunCta && (
        <div className="space-y-3 rounded-md border border-dashed p-10 text-center">
          <h3 className="text-lg font-medium">No jobs yet</h3>
          <p className="text-muted-foreground text-sm">
            Auto-capture will fill this in when M2 ships. For now, log one by
            hand.
          </p>
          <Button asChild>
            <Link to="/jobs/new">Log your first job manually</Link>
          </Button>
        </div>
      )}

      {showFilteredEmpty && (
        <div className="text-muted-foreground rounded-md border border-dashed p-10 text-center text-sm">
          No jobs match these filters.
        </div>
      )}

      {data && data.items.length > 0 && (
        <>
          <div className="overflow-hidden rounded-md border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Completed</TableHead>
                  <TableHead>Route</TableHead>
                  <TableHead>Cargo</TableHead>
                  <TableHead className="text-right">Distance</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead className="text-right">Income</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.items.map((job) => (
                  <TableRow
                    key={job.id}
                    className="hover:bg-accent/50 cursor-pointer"
                    onClick={() => navigate(`/jobs/${job.id}`)}
                  >
                    <TableCell className="whitespace-nowrap">
                      <div>{formatDate(job.completedAt)}</div>
                      <div className="text-muted-foreground text-xs">
                        {formatDuration(job.durationSec)}
                      </div>
                    </TableCell>
                    <TableCell>
                      <div className="font-medium">
                        {job.sourceCity} → {job.destCity}
                      </div>
                      <div className="text-muted-foreground text-xs">
                        {job.sourceCompany} → {job.destCompany}
                      </div>
                    </TableCell>
                    <TableCell>{job.cargoName}</TableCell>
                    <TableCell className="whitespace-nowrap text-right">
                      {formatKm(job.distanceKm)}
                    </TableCell>
                    <TableCell>
                      <JobStatusBadge status={job.status} />
                    </TableCell>
                    <TableCell className="whitespace-nowrap text-right">
                      {formatCurrency(job.declaredIncome)}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>

          <div className="text-muted-foreground flex items-center justify-between text-sm">
            <div>
              Showing {page * PAGE_SIZE + 1}–
              {Math.min((page + 1) * PAGE_SIZE, data.total)} of {data.total}
            </div>
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                disabled={page === 0 || loading}
                onClick={() => setPage((p) => Math.max(0, p - 1))}
              >
                Previous
              </Button>
              <div className="flex items-center px-2 text-sm">
                Page {page + 1} of {pageCount}
              </div>
              <Button
                variant="outline"
                size="sm"
                disabled={page + 1 >= pageCount || loading}
                onClick={() => setPage((p) => p + 1)}
              >
                Next
              </Button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
