import { afterEach, describe, expect, it } from "vitest";
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

  describe("localeFirstDay", () => {
    const real = Intl.Locale;
    const setLocale = (value: unknown) =>
      Object.defineProperty(Intl, "Locale", { value, configurable: true, writable: true });
    const stub = (info: object) => {
      class Fake {
        constructor(public tag: string) {
          if (tag.includes("!")) throw new RangeError("invalid");
        }
      }
      Object.assign(Fake.prototype, info);
      setLocale(Fake);
    };
    afterEach(() => {
      setLocale(real);
    });

    it("maps getWeekInfo()", () => {
      stub({ getWeekInfo: () => ({ firstDay: 7 }) });
      expect(localeFirstDay("en-US")).toBe(7);
    });

    it("maps the weekInfo property", () => {
      stub({ weekInfo: { firstDay: 6 } });
      expect(localeFirstDay("xx")).toBe(6);
    });

    it("falls back to Monday without week info or with an out-of-range day", () => {
      stub({});
      expect(localeFirstDay("de-DE")).toBe(1);
      stub({ getWeekInfo: () => ({ firstDay: 9 }) });
      expect(localeFirstDay("de-DE")).toBe(1);
    });

    it("falls back to Monday for an invalid locale", () => {
      stub({ getWeekInfo: () => ({ firstDay: 7 }) });
      expect(localeFirstDay("not a locale!!")).toBe(1);
    });
  });
});
