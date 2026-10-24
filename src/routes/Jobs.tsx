import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import type { DateRange } from "react-day-picker";
import { ClipboardList, SearchX } from "lucide-react";

import { DateRangePicker } from "@/components/DateRangePicker";
import { EmptyState } from "@/components/EmptyState";
import { JobsTable, Pager } from "@/components/JobsTable";
import { LoadError } from "@/components/LoadError";
import { TableSkeleton, TextSkeleton } from "@/components/Skeletons";
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
  const [attempt, setAttempt] = useState(0);
  const requestKey = `${JSON.stringify(filter)}#${attempt}`;
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
          {data ? (
            <p className="text-muted-foreground text-sm">
              {data.total} {data.total === 1 ? "delivery" : "deliveries"} logged
            </p>
          ) : loading ? (
            <TextSkeleton className="mt-1" />
          ) : null}
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

      {error && (
        <LoadError
          what="your jobs"
          message={error}
          onRetry={() => setAttempt((a) => a + 1)}
        />
      )}

      {/* States */}
      {!data && loading && <TableSkeleton rows={6} cols={6} />}

      {showFirstRunCta && (
        <EmptyState
          icon={ClipboardList}
          title="No jobs yet"
          description="With auto-capture on, every ETS2 delivery logs itself while you drive. You can also log one by hand."
        >
          <Button asChild>
            <Link to="/jobs/new">Log your first job manually</Link>
          </Button>
          <Button variant="outline" asChild>
            <Link to="/settings">Turn on auto-capture</Link>
          </Button>
        </EmptyState>
      )}

      {showFilteredEmpty && (
        <EmptyState
          icon={SearchX}
          title="No jobs match these filters"
          description="Try a different search, status or date range."
        >
          <Button variant="outline" onClick={clearFilters}>
            Clear filters
          </Button>
        </EmptyState>
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
