import { describe, expect, it } from "vitest";

import { hubPill, syncSummary } from "./hub";

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

describe("hubPill", () => {
  const base = {
    hubUrl: "https://hub",
    linked: true,
    account: {
      name: "Erin",
      username: "erin",
      profileUrl: "https://hub/drivers/erin",
    },
    linking: null,
    pending: 0,
    failed: 0,
    synced: 3,
    history: 0,
    lastSyncAt: null,
    lastError: null,
  };

  it("shows nothing unless linked", () => {
    expect(hubPill(null)).toBeNull();
    expect(hubPill({ ...base, linked: false, account: null })).toBeNull();
    expect(hubPill({ ...base, hubUrl: null })).toBeNull();
  });

  it("names the account, then the queue, then problems", () => {
    expect(hubPill(base)).toMatchObject({ state: "linked", label: "@erin" });
    expect(hubPill({ ...base, account: { ...base.account, username: null } })?.label).toBe("Erin");
    expect(hubPill({ ...base, pending: 2 })).toMatchObject({
      state: "syncing",
      label: "Syncing 2",
    });
    expect(hubPill({ ...base, failed: 1 })).toMatchObject({
      state: "attention",
      label: "1 refused",
    });
    expect(
      hubPill({
        ...base,
        pending: 2,
        lastError: "Couldn't sync: couldn't reach the Hub: timeout. Retrying automatically.",
      }),
    ).toMatchObject({ state: "offline", label: "Hub offline" });
  });
});
