import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import type { JobStatus } from "@/types";

const STATUS_LABELS: Record<JobStatus, string> = {
  DELIVERED: "On time",
  LATE: "Late",
  CANCELLED: "Cancelled",
};

const STATUS_CLASSES: Record<JobStatus, string> = {
  DELIVERED:
    "border-transparent bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300",
  LATE: "border-transparent bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300",
  CANCELLED:
    "border-transparent bg-muted text-muted-foreground",
};

export function JobStatusBadge({
  status,
  className,
}: {
  status: JobStatus;
  className?: string;
}) {
  return (
    <Badge className={cn(STATUS_CLASSES[status], className)}>
      {STATUS_LABELS[status]}
    </Badge>
  );
}
