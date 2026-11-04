/**
 * The document of `tamias today --json`, schema version 1 (`docs/schema/today-v1.schema.json`), as the
 * `today` command returns it. Within version 1 fields are only ever added as optional.
 */

/** The only `schema_version` this app reads. */
export const SCHEMA_VERSION = 1;

/** An RFC 3339 instant in UTC, e.g. `2026-11-03T10:05:00Z`. */
export type Timestamp = string;

export type ProviderName = "claude" | "copilot";

/** ok: collected recently; stale: the last collect failed or is too old; unavailable: disabled or not set up. */
export type Status = "ok" | "stale" | "unavailable";

/** Claude: `seven_day` (binding) and `five_hour`. Copilot: `monthly` (binding). */
export type WindowName = "five_hour" | "seven_day" | "monthly";

export type Unit = "percent" | "requests";

/** Relative to today's budget for binding windows; `grey` while the provider is stale. */
export type Colour = "green" | "yellow" | "red" | "grey";

export interface Report {
  schema_version: number;
  generated_at: Timestamp;
  providers: ProviderReport[];
}

export interface ProviderReport {
  provider: ProviderName;
  status: Status;
  /** Last successful collect of a stale provider; absent when it never succeeded. */
  stale_since?: Timestamp;
  /** Windows with data, the binding one first. */
  windows: WindowReport[];
}

export interface WindowReport {
  window: WindowName;
  binding: boolean;
  unit: Unit;
  /** High-water mark of the current window in `unit`. */
  used: number;
  /** Limit in `unit` (100 for percent). */
  limit: number;
  resets_at: Timestamp;
  colour: Colour;
  /** Usage of the local day (binding windows). */
  today_usage?: number;
  /** Budget of the local day (binding windows). */
  budget?: number;
  /** `today_usage / budget` (binding windows, absent when the budget is 0). */
  ratio?: number;
}

/** One local day of a binding window, an entry of `MonthReport` (`DayEntry` in `tamias_core::report`). */
export interface DayEntry {
  /** `YYYY-MM-DD`, the local day. */
  date: string;
  usage: number;
  budget: number;
  /** `usage / budget`, absent when the budget is 0. */
  ratio?: number;
  /** Never `grey`: a stale provider greys only today, in the view. */
  colour: Colour;
}

export interface ProviderMonth {
  provider: ProviderName;
  status: Status;
  /** Last successful collect of a stale provider; absent when it never succeeded. */
  stale_since?: Timestamp;
  /** Unit of the days' `usage` and `budget`. */
  unit: Unit;
  /** Days with data, oldest first; days without data are absent. */
  days: DayEntry[];
}

/** The answer of the `month` command (`MonthReport` in `app/src-tauri/src/month.rs`). */
export interface MonthReport {
  year: number;
  /** 1-12. */
  month: number;
  generated_at: Timestamp;
  providers: ProviderMonth[];
}

const PROVIDER_LABELS: Record<ProviderName, string> = {
  claude: "Claude",
  copilot: "Copilot",
};

const WINDOW_LABELS: Record<WindowName, string> = {
  five_hour: "5-hour window",
  seven_day: "7-day window",
  monthly: "Monthly quota",
};

/** Display name of a provider. */
export function providerLabel(provider: ProviderName): string {
  return PROVIDER_LABELS[provider] ?? provider;
}

/** Display name of a window. */
export function windowLabel(window: WindowName): string {
  return WINDOW_LABELS[window] ?? window;
}

/** `value` in `unit`: `54%` / `54.5%` for percent, `26` / `26.5` for requests. */
export function amount(value: number, unit: Unit): string {
  const text = Number.isInteger(value) ? String(value) : value.toFixed(1);
  return unit === "percent" ? `${text}%` : text;
}

/** The used amount of `w`: `54% used`, or `26 of 300 used` for requests (whose limit is not 100). */
export function usedText(w: WindowReport): string {
  const used = amount(w.used, w.unit);
  return w.unit === "percent" ? `${used} used` : `${used} of ${amount(w.limit, w.unit)} used`;
}

/** Today's usage against today's budget, `today 4% of 10%`; `null` without a budget (informational windows). */
export function todayText(w: WindowReport): string | null {
  if (w.today_usage === undefined || w.budget === undefined) return null;
  return `today ${amount(w.today_usage, w.unit)} of ${amount(w.budget, w.unit)}`;
}

/** Why a stale provider is stale: `stale since 09:00`, or `stale, never collected` without a success. */
export function staleText(since: Timestamp | undefined, now: Date = new Date()): string {
  return since ? `stale since ${localTime(since, now)}` : "stale, never collected";
}

/** `HH:MM` when `at` is on the local day of `now`, else `YYYY-MM-DD HH:MM`; both in local time. */
export function localTime(at: Timestamp, now: Date = new Date()): string {
  const date = new Date(at);
  const pad = (n: number) => String(n).padStart(2, "0");
  const time = `${pad(date.getHours())}:${pad(date.getMinutes())}`;
  const sameDay =
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate();
  return sameDay
    ? time
    : `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${time}`;
}
