// @vitest-environment jsdom
// The live job card ticks every second while driving; the rest of the Dashboard (stat cards,
// both recharts charts) must not re-render on those ticks (#5670).
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { CurrentJob, DashboardStats, PeriodStats } from "@/types";

import { Dashboard } from "./Dashboard";

const h = vi.hoisted(() => ({
  listeners: new Map<string, (e: { payload: unknown }) => void>(),
  chartRenders: 0,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, cb: (e: { payload: unknown }) => void) => {
    h.listeners.set(name, cb);
    return Promise.resolve(() => h.listeners.delete(name));
  },
}));

vi.mock("@/components/charts", () => ({
  DistanceChart: () => {
    h.chartRenders += 1;
    return <div data-testid="distance-chart" />;
  },
  OnTimeDonut: () => <div data-testid="on-time-donut" />,
}));

const period: PeriodStats = {
  totalKm: 1200,
  deliveryKm: 1000,
  freeRoamKm: 200,
  deliveries: 4,
  income: 40000,
  onTime: 3,
  late: 1,
  costs: 120,
};

const stats: DashboardStats = {
  lifetime: period,
  thisMonth: period,
  lastMonth: { ...period, totalKm: 800 },
  last30Days: [],
  recentJobs: [],
};

vi.mock("@/lib/api", () => ({
  api: {
    getDashboardStats: () => Promise.resolve(stats),
    getTelemetryStatus: () => Promise.resolve("delivery"),
    getCurrentJob: () => Promise.resolve(null),
  },
}));

declare global {
  var IS_REACT_ACT_ENVIRONMENT: boolean | undefined;
}

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  h.listeners.clear();
  h.chartRenders = 0;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
});

function tick(drivenKm: number): CurrentJob {
  return {
    cargoName: "Apples",
    cargoMassKg: 12000,
    sourceCity: "Berlin",
    sourceCompany: "Tradeaux",
    destinationCity: "Prague",
    destinationCompany: "Fle",
    income: 9000,
    deadline: null,
    gameTime: null,
    minutesToDeadline: null,
    drivenKm,
    distanceLeftKm: 300 - drivenKm,
    routeMinutesLeft: 200,
    progressPct: (drivenKm / 300) * 100,
    parked: false,
  };
}

test("telemetry:job ticks update the job card without re-rendering the charts", async () => {
  await act(async () => {
    root.render(
      <MemoryRouter>
        <Dashboard />
      </MemoryRouter>,
    );
  });
  expect(container.querySelector('[data-testid="distance-chart"]')).not.toBeNull();
  const before = h.chartRenders;
  const emit = h.listeners.get("telemetry:job");
  expect(emit).toBeDefined();

  for (let km = 1; km <= 10; km++) {
    await act(async () => emit!({ payload: tick(km) }));
  }

  expect(container.querySelector('[data-testid="job-in-progress"]')?.textContent).toContain(
    "Prague",
  );
  expect(h.chartRenders - before).toBe(0);
});
