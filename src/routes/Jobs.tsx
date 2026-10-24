import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import type { DateRange } from "react-day-picker";

import { DateRangePicker } from "@/components/DateRangePicker";
import { JobsTable, Pager } from "@/components/JobsTable";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

import { api } from "@/lib/api";
import { useCaptureRefresh } from "@/lib/useTelemetryEvents";
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
