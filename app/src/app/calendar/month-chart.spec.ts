import { describe, expect, it } from "vitest";
import type { DayEntry, ProviderMonth } from "../core/report";
import { buildMonthChart, cumulativeUsage } from "./month-chart";

function entry(day: number, usage: number, budget = 10): DayEntry {
  const date = `2026-11-${String(day).padStart(2, "0")}`;
  return { date, usage, budget, ratio: usage / budget, colour: "green" };
}

function provider(days: DayEntry[], extra: Partial<ProviderMonth> = {}): ProviderMonth {
  return { provider: "claude", status: "ok", unit: "percent", days, ...extra };
}

describe("cumulativeUsage", () => {
  it("sums the days up to the elapsed one", () => {
    const p = provider([entry(1, 2), entry(2, 9), entry(3, 15)]);
    expect(cumulativeUsage(2026, 11, 5, p, 3)).toEqual([2, 11, 26, null, null]);
  });

  it("leaves a null for a day without data and keeps summing after it", () => {
    const p = provider([entry(1, 2), entry(3, 15)]);
    expect(cumulativeUsage(2026, 11, 4, p, 4)).toEqual([2, null, 17, null]);
  });

  it("has no point after the elapsed day, even when the report has data there", () => {
    const p = provider([entry(1, 2), entry(4, 5)]);
    expect(cumulativeUsage(2026, 11, 4, p, 2)).toEqual([2, null, null, null]);
  });
});

describe("buildMonthChart", () => {
  const p = provider([entry(1, 2), entry(2, 9), entry(3, 15)]);

  it("draws the cumulative usage, the cumulative budget and the projection to the month end", () => {
    const chart = buildMonthChart(2026, 11, 30, p, true, 3)!;
    expect(chart.max).toBe(500);
    expect(chart.usagePath).toBe("M70 191.28 L92 188.04 L114 182.64");
    expect(chart.budgetPath).toBe("M48 192 L708 84");
    // 26 in 3 days over 30 days = 260, below the monthly budget of 300 but above 80 %.
    expect(chart.projectionPath).toBe("M114 182.64 L708 98.4");
    expect(chart.colour).toBe("yellow");
  });

  it("breaks the usage line at a day without data", () => {
    const gap = provider([entry(1, 2), entry(3, 15)]);
    const chart = buildMonthChart(2026, 11, 30, gap, true, 3)!;
    expect(chart.usagePath).toBe("M70 191.28 h0 M114 185.88 h0");
  });

  it("has no usage point for the future days and starts the projection at the last data point", () => {
    const chart = buildMonthChart(2026, 11, 30, p, true, 10)!;
    expect(chart.usagePath).toBe("M70 191.28 L92 188.04 L114 182.64");
    expect(chart.projectionPath?.startsWith("M114 182.64 L708 ")).toBe(true);
  });

  it("colours by the projection ratio: green below 80 %, red above 100 %", () => {
    const low = buildMonthChart(2026, 11, 30, provider([entry(1, 2)]), true, 3)!;
    expect(low.colour).toBe("green");
    const high = buildMonthChart(2026, 11, 30, provider([entry(1, 45)]), true, 3)!;
    expect(high.colour).toBe("red");
  });

  it("colours a stale provider grey", () => {
    const stale = provider([{ ...entry(3, 15), colour: "grey" }]);
    expect(buildMonthChart(2026, 11, 30, stale, true, 3)!.colour).toBe("grey");
  });

  it("draws a past month in full, without a projection line", () => {
    const chart = buildMonthChart(
      2026,
      10,
      31,
      provider([{ ...entry(31, 62), date: "2026-10-31" }]),
      false,
      31,
    )!;
    expect(chart.projectionPath).toBeNull();
    expect(chart.usagePath).toMatch(/^M708 /);
  });

  it("has no chart without data, without a budget or for an unavailable provider", () => {
    expect(buildMonthChart(2026, 11, 30, provider([]), true, 3)).toBeNull();
    expect(buildMonthChart(2026, 11, 30, provider([entry(1, 2, 0)]), true, 3)).toBeNull();
    expect(
      buildMonthChart(2026, 11, 30, provider([entry(1, 2)], { status: "unavailable" }), true, 3),
    ).toBeNull();
  });
});
