import { ArrowDown, ArrowUp, Minus } from "lucide-react";

import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";

export interface StatDelta {
  /** Change against the comparison period; the sign picks the arrow. */
  value: number;
  /** Already formatted, e.g. "+120 km" or "-3". */
  label: string;
  /** What it is compared with, e.g. "vs last month". */
  against: string;
}

export function StatCard({
  label,
  value,
  sub,
  delta,
  hero = false,
  className,
}: {
  label: string;
  value: string;
  /** Smaller line under the value. */
  sub?: string;
  delta?: StatDelta;
  /** The headline card: bigger number, spans two columns. */
  hero?: boolean;
  className?: string;
}) {
  return (
    <Card
      size="sm"
      className={cn(hero && "md:col-span-2", className)}
      data-testid="stat-card"
      data-label={label}
    >
      <CardContent className="space-y-1">
        <p className="text-muted-foreground text-xs font-medium uppercase tracking-wide">
          {label}
        </p>
        <p
          className={cn(
            "font-semibold tabular-nums tracking-tight",
            hero ? "text-4xl" : "text-2xl",
          )}
          data-testid="stat-value"
        >
          {value}
        </p>
        {sub && (
          <p className="text-muted-foreground text-xs tabular-nums" data-testid="stat-sub">
            {sub}
          </p>
        )}
        {delta && <DeltaLine delta={delta} />}
      </CardContent>
    </Card>
  );
}

function DeltaLine({ delta }: { delta: StatDelta }) {
  const Icon = delta.value > 0 ? ArrowUp : delta.value < 0 ? ArrowDown : Minus;
  const tone =
    delta.value > 0
      ? "text-emerald-600 dark:text-emerald-400"
      : delta.value < 0
        ? "text-amber-700 dark:text-amber-400"
        : "text-muted-foreground";
  return (
    <p
      className={cn("flex items-center gap-1 text-xs tabular-nums", tone)}
      data-testid="stat-delta"
    >
      <Icon className="h-3 w-3" aria-hidden="true" />
      <span>{delta.label}</span>
      <span className="text-muted-foreground">{delta.against}</span>
    </p>
  );
}
