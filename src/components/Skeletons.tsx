import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";

/**
 * Loading placeholders shaped like the content they stand in for. Every route shows one
 * of these while its first request is in flight; there is no "Loading…" text anywhere.
 * Each root carries `data-testid="skeleton"` and `aria-busy` so E2E can wait for it to go.
 */

const busy = { "data-testid": "skeleton", "aria-busy": true } as const;

/** A table: a header row and `rows` body rows of `cols` cells. */
export function TableSkeleton({
  rows = 5,
  cols = 5,
  className,
}: {
  rows?: number;
  cols?: number;
  className?: string;
}) {
  return (
    <div className={cn("overflow-hidden rounded-md border", className)} {...busy}>
      <div className="bg-muted/40 flex gap-4 border-b px-3 py-2.5">
        {Array.from({ length: cols }, (_, i) => (
          <Skeleton key={i} className="h-3 flex-1" />
        ))}
      </div>
      {Array.from({ length: rows }, (_, r) => (
        <div key={r} className="flex items-center gap-4 border-b px-3 py-3 last:border-b-0">
          {Array.from({ length: cols }, (_, c) => (
            <div key={c} className="flex-1 space-y-1.5">
              <Skeleton className="h-3.5 w-4/5" />
              {c % 2 === 0 && <Skeleton className="h-2.5 w-2/5" />}
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

/** One line of text (a page subtitle, a count). */
export function TextSkeleton({ className }: { className?: string }) {
  return <Skeleton className={cn("h-4 w-32", className)} {...busy} />;
}

/** A row of stat cards; the first spans two columns like the hero card. */
export function StatGridSkeleton({ cards = 4, hero = true }: { cards?: number; hero?: boolean }) {
  return (
    <div className="grid gap-3 md:grid-cols-3 lg:grid-cols-5" {...busy}>
      {Array.from({ length: cards }, (_, i) => (
        <Card key={i} size="sm" className={cn(hero && i === 0 && "md:col-span-2")}>
          <CardContent className="space-y-2">
            <Skeleton className="h-3 w-20" />
            <Skeleton className={cn("h-7 w-24", hero && i === 0 && "h-10 w-40")} />
            <Skeleton className="h-2.5 w-32" />
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

/** A chart card: title line and a chart-height block. */
export function ChartSkeleton({ className }: { className?: string }) {
  return (
    <Card size="sm" className={className} {...busy}>
      <CardHeader>
        <Skeleton className="h-4 w-40" />
      </CardHeader>
      <CardContent>
        <Skeleton className="h-64 w-full" />
      </CardContent>
    </Card>
  );
}

/** The dashboard and truck pages: stats, a chart and a table. */
export function StatsPageSkeleton({ title = true }: { title?: boolean }) {
  return (
    <div className="space-y-6" {...busy}>
      {title && (
        <div className="space-y-2">
          <Skeleton className="h-7 w-56" />
          <Skeleton className="h-4 w-32" />
        </div>
      )}
      <StatGridSkeleton />
      <ChartSkeleton />
      <TableSkeleton rows={4} cols={6} />
    </div>
  );
}

/** The job page: title, badges, four big numbers and the notes form. */
export function JobDetailSkeleton() {
  return (
    <div className="mx-auto max-w-3xl space-y-6" {...busy}>
      <div className="space-y-3">
        <Skeleton className="h-7 w-64" />
        <div className="flex gap-2">
          <Skeleton className="h-5 w-16 rounded-full" />
          <Skeleton className="h-5 w-24 rounded-full" />
        </div>
      </div>
      <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
        {Array.from({ length: 4 }, (_, i) => (
          <div key={i} className="space-y-2">
            <Skeleton className="h-3 w-16" />
            <Skeleton className="h-7 w-24" />
          </div>
        ))}
      </div>
      <Skeleton className="h-px w-full" />
      <div className="space-y-3">
        <Skeleton className="h-4 w-24" />
        <Skeleton className="h-24 w-full" />
      </div>
    </div>
  );
}

/** The settings page: a few labelled fields. */
export function SettingsSkeleton() {
  return (
    <div className="max-w-2xl space-y-8" {...busy}>
      <div className="space-y-2">
        <Skeleton className="h-7 w-32" />
        <Skeleton className="h-4 w-64" />
      </div>
      {Array.from({ length: 3 }, (_, i) => (
        <div key={i} className="space-y-3">
          <Skeleton className="h-5 w-40" />
          <Skeleton className="h-9 w-full" />
          <Skeleton className="h-3 w-72" />
        </div>
      ))}
    </div>
  );
}
