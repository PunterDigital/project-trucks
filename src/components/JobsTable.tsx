import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { JobStatusBadge } from "@/components/JobStatusBadge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

import { formatCurrency, formatDate, formatDuration, formatKm } from "@/lib/format";
import type { Job } from "@/types";

/** The jobs list shared by the Jobs page and the truck page. Rows open the job. */
export function JobsTable({
  jobs,
  onOpen,
  className,
  ...props
}: {
  jobs: Job[];
  onOpen: (id: string) => void;
} & React.ComponentProps<"div">) {
  return (
    <div className={className ?? "overflow-hidden rounded-md border"} {...props}>
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
          {jobs.map((job) => (
            <TableRow
              key={job.id}
              className="hover:bg-accent/50 cursor-pointer"
              onClick={() => onOpen(job.id)}
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
                <div className="flex flex-wrap items-center gap-1">
                  <JobStatusBadge status={job.status} />
                  {job.flagged && (
                    <Badge
                      data-testid="flagged-badge"
                      title={job.flagReason ?? "Flagged"}
                      className="border-transparent bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300"
                    >
                      Flagged
                    </Badge>
                  )}
                </div>
              </TableCell>
              <TableCell className="whitespace-nowrap text-right">
                {formatCurrency(job.declaredIncome)}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

/** "Showing 1–25 of 60" plus Previous/Next. `page` is zero-based. */
export function Pager({
  page,
  pageSize,
  total,
  loading,
  onPage,
}: {
  page: number;
  pageSize: number;
  total: number;
  loading: boolean;
  onPage: (page: number) => void;
}) {
  const pageCount = Math.max(1, Math.ceil(total / pageSize));
  return (
    <div className="text-muted-foreground flex items-center justify-between text-sm">
      <div>
        Showing {page * pageSize + 1}–{Math.min((page + 1) * pageSize, total)} of {total}
      </div>
      <div className="flex gap-2">
        <Button
          variant="outline"
          size="sm"
          disabled={page === 0 || loading}
          onClick={() => onPage(Math.max(0, page - 1))}
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
          onClick={() => onPage(page + 1)}
        >
          Next
        </Button>
      </div>
    </div>
  );
}
