import { describe, expect, it } from "vitest";
import { daysInMonth, isoDate, leadingBlanks, localeFirstDay, shiftMonth } from "./grid";

describe("grid", () => {
  it("counts the days of a month, leap years included", () => {
    expect(daysInMonth(2026, 11)).toBe(30);
    expect(daysInMonth(2026, 12)).toBe(31);
    expect(daysInMonth(2027, 2)).toBe(28);
    expect(daysInMonth(2028, 2)).toBe(29);
  });

  it("places the 1st by the first day of the week", () => {
    // 2026-11-01 is a Sunday.
    expect(leadingBlanks(2026, 11, 1)).toBe(6);
    expect(leadingBlanks(2026, 11, 7)).toBe(0);
    expect(leadingBlanks(2026, 11, 6)).toBe(1);
    // 2027-02-01 is a Monday.
    expect(leadingBlanks(2027, 2, 1)).toBe(0);
    expect(leadingBlanks(2027, 2, 7)).toBe(1);
  });

  it("formats dates and shifts months over the year boundary", () => {
    expect(isoDate(2026, 3, 5)).toBe("2026-03-05");
    expect(shiftMonth(2026, 12, 1)).toEqual({ year: 2027, month: 1 });
    expect(shiftMonth(2026, 1, -1)).toEqual({ year: 2025, month: 12 });
    expect(shiftMonth(2026, 6, 1)).toEqual({ year: 2026, month: 7 });
  });

  it("knows the first day of the week of a locale", () => {
    expect(localeFirstDay("de-DE")).toBe(1);
    expect(localeFirstDay("en-US")).toBe(7);
    expect(localeFirstDay("not a locale!!")).toBe(1);
  });
});
