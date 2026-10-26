import { describe, expect, it } from "vitest";

import type { UpdateStatus } from "@/types";
import { downloadPercent, updateSummary } from "./updates";

function status(overrides: Partial<UpdateStatus>): UpdateStatus {
  return {
    currentVersion: "0.5.0",
    state: "idle",
    latest: null,
    received: 0,
    total: 0,
    message: null,
    ...overrides,
  };
}

const latest = { version: "0.6.0", pageUrl: "https://example.com/v0.6.0", canInstall: true };

describe("downloadPercent", () => {
  it("rounds down and caps at 100", () => {
    expect(downloadPercent({ received: 0, total: 200 })).toBe(0);
    expect(downloadPercent({ received: 199, total: 200 })).toBe(99);
    expect(downloadPercent({ received: 200, total: 200 })).toBe(100);
    expect(downloadPercent({ received: 300, total: 200 })).toBe(100);
  });

  it("is null when the size is unknown", () => {
    expect(downloadPercent({ received: 50, total: 0 })).toBeNull();
  });
});

describe("updateSummary", () => {
  it("describes each state", () => {
    expect(updateSummary(status({ state: "upToDate" }))).toBe("You have the latest version.");
    expect(updateSummary(status({ state: "available", latest }))).toBe("v0.6.0 is available.");
    expect(
      updateSummary(status({ state: "downloading", latest, received: 50, total: 200 })),
    ).toBe("Downloading v0.6.0 (25%)");
    expect(updateSummary(status({ state: "downloading", latest }))).toBe("Downloading v0.6.0…");
    expect(updateSummary(status({ state: "installing", latest }))).toBe(
      "Installing v0.6.0. ProjectTrucks will restart.",
    );
  });

  it("shows the backend's message for errors and disabled checks", () => {
    expect(updateSummary(status({ state: "error", message: "offline" }))).toBe("offline");
    expect(
      updateSummary(status({ state: "disabled", message: "Update checks are off in test profiles." })),
    ).toBe("Update checks are off in test profiles.");
  });
});
