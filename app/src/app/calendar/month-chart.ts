import {
  amount,
  monthProjection,
  projectionColour,
  type Colour,
  type ProviderMonth,
} from "../core/report";
import { niceMax } from "../day/chart";
import { isoDate } from "./grid";

/** Size of the SVG (its `viewBox`); the plot lies inside the margins. */
export const WIDTH = 720;
export const HEIGHT = 220;
const LEFT = 48;
const RIGHT = 12;
const TOP = 12;
const BOTTOM = 28;

export interface MonthTick {
  at: number;
  label: string;
}

/** The month chart of one provider, in SVG coordinates. */
export interface MonthChart {
  /** The cumulative usage; one `M... L...` run per stretch of days with data, so a day without data leaves a gap. */
  usagePath: string;
  /** The cumulative budget, a straight line from day 0 to the month end. */
  budgetPath: string;
  /** Dashed line from the last usage point to the projected month end; `null` without a projection. */
  projectionPath: string | null;
  /** The colour of the usage line and the projection: the projection colour rule. */
  colour: Colour;
  left: number;
  right: number;
  top: number;
  bottom: number;
  /** Top of the y axis, in the unit. */
  max: number;
  xTicks: MonthTick[];
  yTicks: MonthTick[];
}

const round = (n: number) => Math.round(n * 100) / 100;

/**
 * Cumulative usage per day of the month: `null` for a day without data and for every day after
 * `elapsed`. A day without data adds nothing to the sum.
 */
export function cumulativeUsage(
  year: number,
  month: number,
  days: number,
  p: ProviderMonth,
  elapsed: number,
): (number | null)[] {
  const byDate = new Map(p.days.map((d) => [d.date, d.usage]));
  let sum = 0;
  return Array.from({ length: days }, (_, i) => {
    if (i + 1 > elapsed) return null;
    const usage = byDate.get(isoDate(year, month, i + 1));
    if (usage === undefined) return null;
    sum += usage;
    return sum;
  });
}

/**
 * The month chart of `p`: cumulative usage per day against the cumulative budget, and for the
 * current month (`current`, `elapsed` = today's day number) the projection as a dashed line to the
 * month end. `null` for an unavailable provider, without data or without a budget.
 */
export function buildMonthChart(
  year: number,
  month: number,
  days: number,
  p: ProviderMonth,
  current: boolean,
  elapsed: number,
): MonthChart | null {
  const last = p.days[p.days.length - 1];
  if (p.status === "unavailable" || !last || !(last.budget > 0)) return null;
  const upTo = current ? elapsed : days;
  const cumulative = cumulativeUsage(year, month, days, p, upTo);
  const total = cumulative.reduce<number>((m, v) => (v === null ? m : Math.max(m, v)), 0);
  const usage = p.days.reduce(
    (sum, d) => sum + (d.date <= isoDate(year, month, upTo) ? d.usage : 0),
    0,
  );
  const projection = monthProjection(usage, last.budget, upTo, days);
  const monthBudget = last.budget * days;
  const colour = projection ? projectionColour(projection, last.colour) : last.colour;

  const right = WIDTH - RIGHT;
  const bottom = HEIGHT - BOTTOM;
  const max = niceMax(
    Math.max(monthBudget, total, current && projection ? projection.projected : 0),
  );
  const x = (day: number) => LEFT + (day / days) * (right - LEFT);
  const y = (v: number) => bottom - (v / max) * (bottom - TOP);

  let usagePath = "";
  let lastIndex = -1;
  cumulative.forEach((v, i) => {
    if (v === null) return;
    const point = `${round(x(i + 1))} ${round(y(v))}`;
    const joins = i > 0 && cumulative[i - 1] !== null;
    usagePath += joins ? ` L${point}` : `${usagePath === "" ? "" : " "}M${point}`;
    // A lone day is a zero-length segment, so the round line cap still draws a dot.
    if (cumulative[i + 1] == null && !joins) usagePath += " h0";
    lastIndex = i;
  });
  const projectionPath =
    current && projection && lastIndex >= 0 && lastIndex + 1 < days
      ? `M${round(x(lastIndex + 1))} ${round(y(cumulative[lastIndex]!))} L${round(x(days))} ${round(y(projection.projected))}`
      : null;

  const xTicks = [1, 8, 15, 22, days]
    .filter((d, i, all) => d <= days && all.indexOf(d) === i && (d === days || days - d >= 3))
    .map((d) => ({ at: round(x(d)), label: String(d) }));
  const yTicks = [0, 0.5, 1].map((f) => ({
    at: round(y(f * max)),
    label: amount(f * max, p.unit),
  }));
  return {
    usagePath,
    budgetPath: `M${round(x(0))} ${round(y(0))} L${round(x(days))} ${round(y(monthBudget))}`,
    projectionPath,
    colour,
    left: LEFT,
    right,
    top: TOP,
    bottom,
    max,
    xTicks,
    yTicks,
  };
}
