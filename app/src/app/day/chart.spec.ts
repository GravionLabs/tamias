import { describe, expect, it } from "vitest";
import type { ProviderDay } from "../core/report";
import { buildChart, dayBounds, emptyReason, niceMax } from "./chart";

/** A local time on 2026-11-03, as the backend would send it (UTC), whatever the machine's zone. */
function at(hour: number, minute = 0): string {
  return new Date(2026, 10, 3, hour, minute).toISOString();
}

const DATE = "2026-11-03";

describe("buildChart size", () => {
  const pts = [{ at: at(12), usage: 5 }];

  it("lays the plot out in the given size, keeping the margins fixed", () => {
    const chart = buildChart(DATE, pts, 10, "percent", undefined, { width: 960, height: 400 });
    expect(chart.right).toBe(948);
    expect(chart.bottom).toBe(372);
    expect(chart.top).toBe(12);
    expect(chart.left).toBe(48);
    expect(chart.path).toBe("M498 192");
  });

  it("falls back to the default size without one", () => {
    expect(buildChart(DATE, pts, 10, "percent").right).toBe(468);
  });

  it("never gets a plot smaller than the minimum size", () => {
    const chart = buildChart(DATE, pts, 10, "percent", undefined, { width: 10, height: 10 });
    expect(chart.right).toBeGreaterThan(chart.left);
    expect(chart.bottom).toBeGreaterThan(chart.top);
  });
});

describe("buildChart", () => {
  const points = [
    { at: at(0), usage: 0 },
    { at: at(6), usage: 2 },
    { at: at(12), usage: 6 },
  ];

  it("maps points to x over the local day and y over the unit", () => {
    const chart = buildChart(DATE, points, 10, "percent");
    // Plot: x 48..468 for 00:00..24:00, y 232 (0) .. 12 (10, the axis top).
    expect(chart.path).toBe("M48 232 H153 V188 H258 V100");
    expect(chart.max).toBe(10);
  });

  it("draws the budget line at the budget", () => {
    expect(buildChart(DATE, points, 10, "percent").budgetY).toBe(12);
    // Budget 5 against a top of 10 (the usage reaches 6): halfway up the plot.
    expect(buildChart(DATE, points, 5, "percent").budgetY).toBe(122);
  });

  it("scales the y axis to the budget or the usage, whichever is higher", () => {
    expect(
      buildChart(
        DATE,
        [
          { at: at(0), usage: 0 },
          { at: at(1), usage: 15 },
        ],
        10,
        "percent",
      ).max,
    ).toBe(20);
    expect(buildChart(DATE, points, 4, "percent").budgetY).toBeCloseTo(232 - (4 / 10) * 220);
  });

  it("runs the line on to the present when given", () => {
    expect(buildChart(DATE, points, 10, "percent", new Date(2026, 10, 3, 15).getTime()).path).toBe(
      "M48 232 H153 V188 H258 V100 H310.5",
    );
  });

  it("does not run on when the present is not after the last point", () => {
    expect(buildChart(DATE, points, 10, "percent", new Date(2026, 10, 3, 12).getTime()).path).toBe(
      "M48 232 H153 V188 H258 V100",
    );
  });

  it("labels the axes in the unit: 00:00-24:00 and the amounts", () => {
    const chart = buildChart(DATE, points, 10, "percent");
    expect(chart.xTicks.map((t) => t.label)).toEqual(["00:00", "06:00", "12:00", "18:00", "24:00"]);
    expect(chart.xTicks.map((t) => t.at)).toEqual([48, 153, 258, 363, 468]);
    expect(chart.yTicks.map((t) => t.label)).toEqual(["0%", "2.5%", "5%", "7.5%", "10%"]);
    const requests = buildChart(DATE, points, 8, "requests");
    expect(requests.yTicks.map((t) => t.label)).toEqual(["0", "2.5", "5", "7.5", "10"]);
  });

  it("has an empty line without points", () => {
    expect(buildChart(DATE, [], 10, "percent", Date.now()).path).toBe("");
  });
});

describe("dayBounds", () => {
  it("spans the local day, midnight to the next midnight", () => {
    const { start, end } = dayBounds("2026-12-31");
    expect(new Date(start).getFullYear()).toBe(2026);
    expect(new Date(end).getFullYear()).toBe(2027);
    expect(new Date(end).getDate()).toBe(1);
    expect(new Date(end).getHours()).toBe(0);
  });
});

describe("niceMax", () => {
  it("rounds up to 1, 2, 2.5, 5 or 10 times a power of ten", () => {
    expect(niceMax(0)).toBe(1);
    expect(niceMax(-3)).toBe(1);
    expect(niceMax(8.04)).toBe(10);
    expect(niceMax(10)).toBe(10);
    expect(niceMax(11.1)).toBe(20);
    expect(niceMax(21)).toBe(25);
    expect(niceMax(3)).toBe(5);
  });
});

describe("emptyReason", () => {
  const base: ProviderDay = { provider: "claude", status: "ok", unit: "percent", points: [] };
  const now = new Date(2026, 10, 3, 10, 5);

  it("says an unavailable provider is not set up", () => {
    expect(emptyReason({ ...base, status: "unavailable" }, now)).toBe(
      "Not set up or switched off.",
    );
  });

  it("says a day without points has no data", () => {
    expect(emptyReason(base, now)).toBe("No data for this day.");
  });

  it("adds the staleness to the reason", () => {
    expect(emptyReason({ ...base, status: "stale" }, now)).toBe(
      "No data for this day. The provider is stale, never collected.",
    );
  });

  it("has no reason with a curve", () => {
    expect(emptyReason({ ...base, points: [{ at: at(1), usage: 0 }] }, now)).toBeNull();
  });
});

describe("buildChart with five-hour points", () => {
  const points = [{ at: at(12), usage: 5 }];

  it("scales both curves to percent, the budget at 100 and the axis above a higher curve", () => {
    const chart = buildChart(DATE, points, 10, "requests", undefined, undefined, [
      { at: at(12), used: 150, limit: 100 },
    ]);
    expect(chart.dual).toBe(true);
    expect(chart.max).toBe(200);
    // y: 232 - 220 * percent / 200; day 50 % -> 177, window 150 % -> 67, budget 100 % -> 122.
    expect(chart.path).toBe("M258 177");
    expect(chart.fiveHourPath).toBe("M258 67");
    expect(chart.budgetY).toBe(122);
    expect(chart.yTicks.at(-1)?.label).toBe("200%");
  });

  it("has no window path without points, and stays single without the field", () => {
    expect(
      buildChart(DATE, points, 10, "percent", undefined, undefined, []).fiveHourPath,
    ).toBeNull();
    const single = buildChart(DATE, points, 10, "percent");
    expect(single.dual).toBe(false);
    expect(single.fiveHourPath).toBeNull();
  });
});
