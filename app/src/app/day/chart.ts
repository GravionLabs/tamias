import { amount, staleText, type CurvePoint, type ProviderDay, type Unit } from "../core/report";

/** Default size of the SVG (its `viewBox`) until the box is measured; the plot lies inside the margins. */
export const WIDTH = 480;
export const HEIGHT = 260;
/** Smallest size the plot is laid out in, so a tiny box never gives a negative plot. */
const MIN_WIDTH = 160;
const MIN_HEIGHT = 100;
const LEFT = 48;
const RIGHT = 12;
const TOP = 12;
const BOTTOM = 28;

export interface Tick {
  /** Coordinate on the axis. */
  at: number;
  label: string;
}

/** The chart of one provider, in SVG coordinates. */
export interface Chart {
  /** The usage step line: along to the next snapshot, then up. */
  path: string;
  /** y of the budget line, which spans `left` to `right`. */
  budgetY: number;
  left: number;
  right: number;
  top: number;
  bottom: number;
  /** Top of the y axis, in the unit. */
  max: number;
  xTicks: Tick[];
  yTicks: Tick[];
}

/** First instant of the local day `date` (`YYYY-MM-DD`) and the first of the next one. */
export function dayBounds(date: string): { start: number; end: number } {
  const [year, month, day] = date.split("-").map(Number) as [number, number, number];
  return {
    start: new Date(year, month - 1, day).getTime(),
    end: new Date(year, month - 1, day + 1).getTime(),
  };
}

/** A round top for the y axis that holds `value`: 1, 2, 2.5, 5 or 10 times a power of ten. */
export function niceMax(value: number): number {
  if (!(value > 0)) return 1;
  const power = 10 ** Math.floor(Math.log10(value));
  const step = [1, 2, 2.5, 5, 10].find((s) => s * power >= value) ?? 10;
  return step * power;
}

const round = (n: number) => Math.round(n * 100) / 100;

/**
 * Maps the curve and the budget of the day `date` to SVG coordinates. The x axis spans the local
 * day, midnight to midnight (23 or 25 hours on a DST change); the y axis starts at 0. The line
 * runs on to `until` (now, for today) when that is after the last point.
 */
export function buildChart(
  date: string,
  points: CurvePoint[],
  budget: number,
  unit: Unit,
  until?: number,
  size: { width: number; height: number } = { width: WIDTH, height: HEIGHT },
): Chart {
  const { start, end } = dayBounds(date);
  const right = Math.max(size.width, MIN_WIDTH) - RIGHT;
  const bottom = Math.max(size.height, MIN_HEIGHT) - BOTTOM;
  const top = TOP;
  const high = points.reduce((m, p) => Math.max(m, p.usage), 0);
  const max = niceMax(Math.max(budget, high));
  const x = (t: number) =>
    LEFT + ((Math.min(Math.max(t, start), end) - start) / (end - start)) * (right - LEFT);
  const y = (v: number) => bottom - (v / max) * (bottom - top);

  let path = "";
  points.forEach((p, i) => {
    const px = round(x(new Date(p.at).getTime()));
    path += i === 0 ? `M${px} ${round(y(p.usage))}` : ` H${px} V${round(y(p.usage))}`;
  });
  const lastAt = points.length ? new Date(points[points.length - 1]!.at).getTime() : start;
  if (points.length > 0 && until !== undefined && until > lastAt) {
    path += ` H${round(x(until))}`;
  }

  // Ticks sit at the real instants of local 00:00, 06:00, 12:00 and 18:00, the last one at the end
  // of the day (so on a 23 or 25 hour day the spacing is uneven but each label is right).
  const [year, month, day] = date.split("-").map(Number) as [number, number, number];
  const xTicks = [0, 6, 12, 18, 24].map((h) => ({
    at: round(x(h === 24 ? end : new Date(year, month - 1, day, h).getTime())),
    label: `${String(h).padStart(2, "0")}:00`,
  }));
  const yTicks = [0, 0.25, 0.5, 0.75, 1].map((f) => ({
    at: round(y(f * max)),
    label: amount(f * max, unit),
  }));
  return { path, budgetY: round(y(budget)), left: LEFT, right, top, bottom, max, xTicks, yTicks };
}

/** Why a provider has no curve for the day, `null` when it has one. */
export function emptyReason(p: ProviderDay, now: Date): string | null {
  if (p.status === "unavailable") return "Not set up or switched off.";
  if (p.points.length > 0) return null;
  const stale = p.status === "stale" ? ` The provider is ${staleText(p.stale_since, now)}.` : "";
  return `No data for this day.${stale}`;
}
