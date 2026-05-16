// Domain formatters. All use a fixed `en-GB` locale for now — when the
// Settings page wires up the imperial/metric toggle in M2, this is where we
// switch on it.

const LOCALE = "en-GB";

const kmFormatter = new Intl.NumberFormat(LOCALE, {
  maximumFractionDigits: 1,
});

const currencyFormatter = new Intl.NumberFormat(LOCALE, {
  maximumFractionDigits: 0,
});

const dateFormatter = new Intl.DateTimeFormat(LOCALE, {
  year: "numeric",
  month: "short",
  day: "numeric",
});

const relativeFormatter = new Intl.RelativeTimeFormat(LOCALE, { numeric: "auto" });

/** "1,234.5 km" — drops trailing .0 thanks to maximumFractionDigits. */
export function formatKm(km: number): string {
  return `${kmFormatter.format(km)} km`;
}

/** "4,250 €" — ETS2's in-game currency is displayed with the euro sign. */
export function formatCurrency(amount: number): string {
  return `${currencyFormatter.format(amount)} €`;
}

/** "4h 30m" — drops zero parts ("45m", "2h"). */
export function formatDuration(seconds: number): string {
  if (seconds <= 0) return "0m";
  const totalMinutes = Math.round(seconds / 60);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours === 0) return `${minutes}m`;
  if (minutes === 0) return `${hours}h`;
  return `${hours}h ${minutes}m`;
}

/**
 * Human relative date for ISO 8601 timestamps:
 *   - "just now" for <60s
 *   - "5 minutes ago" / "in 3 hours" for the same day
 *   - "yesterday" / "tomorrow"
 *   - "3 days ago" / "in 2 weeks" up to ~30 days
 *   - Falls back to a short absolute date ("16 May 2026") for older / further entries.
 */
export function formatRelativeDate(iso: string, now: Date = new Date()): string {
  const then = new Date(iso);
  if (Number.isNaN(then.getTime())) return iso;

  const diffMs = then.getTime() - now.getTime();
  const diffSec = Math.round(diffMs / 1000);
  const absSec = Math.abs(diffSec);

  if (absSec < 60) return "just now";

  if (absSec < 60 * 60) {
    return relativeFormatter.format(Math.round(diffSec / 60), "minute");
  }
  if (absSec < 60 * 60 * 24) {
    return relativeFormatter.format(Math.round(diffSec / 3600), "hour");
  }
  if (absSec < 60 * 60 * 24 * 7) {
    return relativeFormatter.format(Math.round(diffSec / 86400), "day");
  }
  if (absSec < 60 * 60 * 24 * 30) {
    return relativeFormatter.format(Math.round(diffSec / (86400 * 7)), "week");
  }
  return dateFormatter.format(then);
}

/** "16 May 2026" — absolute, locale-formatted, ISO 8601 → readable. */
export function formatDate(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return dateFormatter.format(date);
}
