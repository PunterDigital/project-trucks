import { describe, expect, it } from "vitest";

import { syncSummary } from "./hub";

describe("syncSummary", () => {
  it("counts synced jobs", () => {
    expect(syncSummary({ synced: 0, pending: 0, failed: 0 })).toBe("0 jobs on the Hub");
    expect(syncSummary({ synced: 1, pending: 0, failed: 0 })).toBe("1 job on the Hub");
  });

  it("mentions waiting and refused jobs only when there are some", () => {
    expect(syncSummary({ synced: 12, pending: 3, failed: 1 })).toBe(
      "12 jobs on the Hub · 3 waiting to sync · 1 refused",
    );
  });
});
