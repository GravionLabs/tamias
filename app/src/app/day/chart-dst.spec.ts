import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { buildChart, dayBounds } from "./chart";

describe("buildChart x ticks on a DST change (Europe/Berlin)", () => {
  const original = process.env["TZ"];
  beforeAll(() => {
    process.env["TZ"] = "Europe/Berlin";
  });
  afterAll(() => {
    if (original === undefined) delete process.env["TZ"];
    else process.env["TZ"] = original;
  });

  // Plot x 48..468 (420 wide); the labels sit at the real local hours.
  it.each([
    // 23 hour day: 06:00 is 5 h in, 12:00 11 h, 18:00 17 h.
    ["2026-03-29", 23, [48, 48 + (420 * 5) / 23, 48 + (420 * 11) / 23, 48 + (420 * 17) / 23, 468]],
    // 25 hour day: 06:00 is 7 h in, 12:00 13 h, 18:00 19 h.
    ["2026-10-25", 25, [48, 48 + (420 * 7) / 25, 48 + (420 * 13) / 25, 48 + (420 * 19) / 25, 468]],
  ])("%s (%i h)", (date, hours, expected) => {
    const { start, end } = dayBounds(date);
    expect((end - start) / 3_600_000).toBe(hours);
    const chart = buildChart(date, [], 10, "percent");
    expect(chart.xTicks.map((t) => t.label)).toEqual(["00:00", "06:00", "12:00", "18:00", "24:00"]);
    chart.xTicks.forEach((t, i) => expect(t.at).toBeCloseTo(expected[i]!, 1));
  });
});
