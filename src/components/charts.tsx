// Dashboard charts (recharts). Colours come from the theme's --chart-* tokens so
// they follow light/dark mode; recharts puts them straight into SVG attributes.

import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Legend,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

import { formatKm } from "@/lib/format";
import type { DistanceDay } from "@/types";

const DELIVERY_COLOR = "var(--chart-4)";
const FREE_ROAM_COLOR = "var(--chart-1)";
const ON_TIME_COLOR = "var(--chart-4)";
const LATE_COLOR = "var(--chart-1)";

const dayLabel = new Intl.DateTimeFormat("en-GB", { day: "numeric", month: "short" });

/** "2026-05-15" → "15 May". Parsed as a local date so the label matches the bucket. */
function formatDay(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  return dayLabel.format(new Date(y, m - 1, d));
}

/** Stacked delivery + free-roam km per day (bars: the days are discrete, and most are empty). */
export function DistanceChart({ days }: { days: DistanceDay[] }) {
  return (
    <div className="h-64 w-full" data-testid="distance-chart">
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={days} margin={{ top: 8, right: 8, left: 0, bottom: 0 }} barCategoryGap={2}>
          <CartesianGrid vertical={false} strokeDasharray="3 3" stroke="var(--border)" />
          <XAxis
            dataKey="day"
            tickFormatter={formatDay}
            tickLine={false}
            axisLine={false}
            minTickGap={24}
            tick={{ fontSize: 11, fill: "var(--muted-foreground)" }}
          />
          <YAxis
            tickFormatter={(v: number) => formatKm(v)}
            tickLine={false}
            axisLine={false}
            width={64}
            tick={{ fontSize: 11, fill: "var(--muted-foreground)" }}
          />
          <Tooltip
            labelFormatter={(label) => formatDay(String(label))}
            formatter={(value, name) => [formatKm(Number(value)), String(name)]}
            contentStyle={{
              background: "var(--popover)",
              color: "var(--popover-foreground)",
              border: "1px solid var(--border)",
              borderRadius: 8,
              fontSize: 12,
            }}
          />
          <Legend iconType="square" iconSize={10} wrapperStyle={{ fontSize: 12 }} />
          <Bar
            dataKey="deliveryKm"
            name="Delivery"
            stackId="km"
            fill={DELIVERY_COLOR}
            isAnimationActive={false}
          />
          <Bar
            dataKey="freeRoamKm"
            name="Free roam"
            stackId="km"
            fill={FREE_ROAM_COLOR}
            radius={[2, 2, 0, 0]}
            isAnimationActive={false}
          />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

/** On-time vs late deliveries as a donut, with the on-time share in the middle. */
export function OnTimeDonut({ onTime, late }: { onTime: number; late: number }) {
  const total = onTime + late;
  const pct = total === 0 ? null : Math.round((onTime / total) * 100);
  const data =
    total === 0
      ? [{ name: "No deliveries", value: 1, color: "var(--muted)" }]
      : [
          { name: "On time", value: onTime, color: ON_TIME_COLOR },
          { name: "Late", value: late, color: LATE_COLOR },
        ];
  return (
    <div className="relative h-64 w-full" data-testid="on-time-donut">
      <ResponsiveContainer width="100%" height="100%">
        <PieChart>
          <Pie
            data={data}
            dataKey="value"
            nameKey="name"
            innerRadius="62%"
            outerRadius="85%"
            stroke="var(--card)"
            isAnimationActive={false}
          >
            {data.map((d) => (
              <Cell key={d.name} fill={d.color} />
            ))}
          </Pie>
          {total > 0 && (
            <Tooltip
              formatter={(value, name) => [String(value), String(name)]}
              contentStyle={{
                background: "var(--popover)",
                color: "var(--popover-foreground)",
                border: "1px solid var(--border)",
                borderRadius: 8,
                fontSize: 12,
              }}
            />
          )}
        </PieChart>
      </ResponsiveContainer>
      <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center">
        <span className="text-3xl font-semibold tabular-nums" data-testid="on-time-pct">
          {pct === null ? "–" : `${pct}%`}
        </span>
        <span className="text-muted-foreground text-xs">on time</span>
      </div>
    </div>
  );
}
