import { describe, expect, it } from "vitest";

import {
  formatCurrency,
  formatDate,
  formatDuration,
  formatKm,
  formatRelativeDate,
} from "./format";

describe("formatKm", () => {
  it("groups thousands and keeps one decimal", () => {
    expect(formatKm(1234.56)).toBe("1,234.6 km");
  });

  it("drops a trailing .0", () => {
    expect(formatKm(812)).toBe("812 km");
    expect(formatKm(0)).toBe("0 km");
  });
});

describe("formatCurrency", () => {
  it("rounds to whole euros with the sign after the amount", () => {
    expect(formatCurrency(4250)).toBe("4,250 €");
    expect(formatCurrency(1999.6)).toBe("2,000 €");
  });

  it("keeps the minus sign for losses", () => {
    expect(formatCurrency(-300)).toBe("-300 €");
  });
});

describe("formatDuration", () => {
  it("shows hours and minutes", () => {
    expect(formatDuration(4 * 3600 + 30 * 60)).toBe("4h 30m");
  });

  it("drops zero parts", () => {
    expect(formatDuration(45 * 60)).toBe("45m");
    expect(formatDuration(2 * 3600)).toBe("2h");
  });

  it("rounds to the nearest minute, carrying into hours", () => {
    expect(formatDuration(89)).toBe("1m");
    expect(formatDuration(3599)).toBe("1h");
  });

  it("treats zero and negative durations as 0m", () => {
    expect(formatDuration(0)).toBe("0m");
    expect(formatDuration(-60)).toBe("0m");
  });
});

describe("formatDate", () => {
  it("formats an ISO timestamp as a short date", () => {
    expect(formatDate("2026-05-16T12:00:00Z")).toBe("16 May 2026");
  });

  it("returns unparseable input unchanged", () => {
    expect(formatDate("not a date")).toBe("not a date");
  });
});

describe("formatRelativeDate", () => {
  const now = new Date("2026-05-16T12:00:00Z");
  const ago = (seconds: number) =>
    new Date(now.getTime() - seconds * 1000).toISOString();
  const ahead = (seconds: number) =>
    new Date(now.getTime() + seconds * 1000).toISOString();

  it("says 'just now' under a minute either way", () => {
    expect(formatRelativeDate(ago(30), now)).toBe("just now");
    expect(formatRelativeDate(ahead(59), now)).toBe("just now");
  });

  it("uses minutes, then hours", () => {
    expect(formatRelativeDate(ago(5 * 60), now)).toBe("5 minutes ago");
    expect(formatRelativeDate(ahead(3 * 3600), now)).toBe("in 3 hours");
  });

  it("uses yesterday / tomorrow and days within a week", () => {
    expect(formatRelativeDate(ago(86400), now)).toBe("yesterday");
    expect(formatRelativeDate(ahead(86400), now)).toBe("tomorrow");
    expect(formatRelativeDate(ago(3 * 86400), now)).toBe("3 days ago");
  });

  it("uses weeks up to about a month", () => {
    expect(formatRelativeDate(ahead(14 * 86400), now)).toBe("in 2 weeks");
  });

  it("falls back to an absolute date after 30 days", () => {
    expect(formatRelativeDate("2026-03-01T12:00:00Z", now)).toBe("1 Mar 2026");
  });

  it("returns unparseable input unchanged", () => {
    expect(formatRelativeDate("soon", now)).toBe("soon");
  });
});
