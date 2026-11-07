import { describe, expect, it } from "vitest";
import {
  amount,
  localTime,
  monthProjection,
  projectionColour,
  ratioColour,
  windowProjection,
  providerLabel,
  staleText,
  todayPercent,
  projectionShortText,
  todayShortText,
  todayText,
  usedPercent,
  usedText,
  windowLabel,
  type WindowReport,
} from "./report";

describe("amount", () => {
  it("shows percent with a % sign and whole numbers without decimals", () => {
    expect(amount(54, "percent")).toBe("54%");
    expect(amount(0, "percent")).toBe("0%");
  });

  it("shows one decimal for fractions", () => {
    expect(amount(4.25, "percent")).toBe("4.3%");
    expect(amount(6.5, "requests")).toBe("6.5");
  });

  it("shows requests without a unit sign, also above the limit", () => {
    expect(amount(26, "requests")).toBe("26");
    expect(amount(312, "requests")).toBe("312");
  });
});

describe("localTime", () => {
  // Instants built from local date parts, so the expectations hold in every time zone.
  const now = new Date(2026, 10, 3, 12, 0);

  it("shows HH:MM on the local day of now", () => {
    expect(localTime(new Date(2026, 10, 3, 9, 5).toISOString(), now)).toBe("09:05");
    expect(localTime(new Date(2026, 10, 3, 0, 0).toISOString(), now)).toBe("00:00");
    expect(localTime(new Date(2026, 10, 3, 23, 59).toISOString(), now)).toBe("23:59");
  });

  it("adds the date on another day", () => {
    expect(localTime(new Date(2026, 10, 2, 23, 59).toISOString(), now)).toBe("2026-11-02 23:59");
    expect(localTime(new Date(2026, 11, 1, 0, 0).toISOString(), now)).toBe("2026-12-01 00:00");
  });
});

describe("staleText", () => {
  const now = new Date(2026, 10, 3, 12, 0);

  it("names the time of the last success", () => {
    expect(staleText(new Date(2026, 10, 3, 9, 0).toISOString(), now)).toBe("stale since 09:00");
  });

  it("says when there never was a success", () => {
    expect(staleText(undefined, now)).toBe("stale, never collected");
  });
});

describe("labels", () => {
  it("names the providers", () => {
    expect(providerLabel("claude")).toBe("Claude");
    expect(providerLabel("copilot")).toBe("Copilot");
  });

  it("names the windows", () => {
    expect(windowLabel("seven_day")).toBe("7-day window");
    expect(windowLabel("five_hour")).toBe("5-hour window");
    expect(windowLabel("monthly")).toBe("Monthly quota");
  });
});

const SEVEN_DAY: WindowReport = {
  window: "seven_day",
  binding: true,
  unit: "percent",
  used: 54,
  limit: 100,
  resets_at: "2026-11-07T12:00:00Z",
  colour: "green",
  today_usage: 4,
  budget: 10,
  ratio: 0.4,
};

describe("usedText", () => {
  it("shows percent windows as the percentage used", () => {
    expect(usedText(SEVEN_DAY)).toBe("54% used");
  });

  it("shows request windows against their limit", () => {
    expect(
      usedText({ ...SEVEN_DAY, window: "monthly", unit: "requests", used: 26, limit: 300 }),
    ).toBe("26 of 300 used");
  });
});

describe("usedPercent", () => {
  it("rounds the share of the limit", () => {
    expect(usedPercent({ ...SEVEN_DAY, unit: "requests", used: 489, limit: 2380 })).toBe(21);
    expect(usedPercent(SEVEN_DAY)).toBe(54);
  });

  it("is 0 for a limit of 0", () => {
    expect(usedPercent({ ...SEVEN_DAY, limit: 0 })).toBe(0);
  });
});

describe("todayShortText", () => {
  it("shows today's usage against today's budget, in the unit of the window", () => {
    expect(todayShortText(SEVEN_DAY)).toBe("4% of 10% today");
    expect(
      todayShortText({ ...SEVEN_DAY, unit: "requests", today_usage: 12, budget: 40, ratio: 0.3 }),
    ).toBe("12 of 40 today");
  });

  it("is null without a budget", () => {
    expect(todayShortText({ ...SEVEN_DAY, budget: undefined, today_usage: undefined })).toBeNull();
  });
});

describe("projectionShortText", () => {
  it("shows the projected month against the monthly quota, rounded", () => {
    expect(
      projectionShortText(
        { projected: 239.6, budget: 300, percent: 80, colour: "green" },
        "requests",
      ),
    ).toBe("≈ 240 of 300 by month end");
    expect(
      projectionShortText(
        { projected: 342.857, budget: 300, percent: 114, colour: "red" },
        "percent",
      ),
    ).toBe("≈ 343% of 300% by month end");
  });
});

describe("todayPercent", () => {
  it("is the ratio of today's usage to the budget in percent", () => {
    expect(todayPercent(SEVEN_DAY)).toBe(40);
    expect(todayPercent({ ...SEVEN_DAY, unit: "requests", ratio: 0.2134 })).toBe(21);
  });

  it("shows the real percent above the budget", () => {
    expect(todayPercent({ ...SEVEN_DAY, ratio: 1.8 })).toBe(180);
  });

  it("is null without a budget or with a budget of 0", () => {
    expect(
      todayPercent({ ...SEVEN_DAY, today_usage: undefined, budget: undefined, ratio: undefined }),
    ).toBeNull();
    expect(todayPercent({ ...SEVEN_DAY, budget: 0, ratio: undefined })).toBeNull();
  });
});

describe("todayText", () => {
  it("shows today's usage against today's budget", () => {
    expect(todayText(SEVEN_DAY)).toBe("today 4% of 10%");
  });

  it("is null without a budget", () => {
    const fiveHour: WindowReport = {
      window: "five_hour",
      binding: false,
      unit: "percent",
      used: 12,
      limit: 100,
      resets_at: "2026-11-03T13:30:00Z",
      colour: "green",
    };
    expect(todayText(fiveHour)).toBeNull();
  });

  it("shows a budget of 0", () => {
    expect(todayText({ ...SEVEN_DAY, today_usage: 0, budget: 0, ratio: undefined })).toBe(
      "today 0% of 0%",
    );
  });
});

describe("monthProjection", () => {
  it("extrapolates mid-month: 15 of 30 days used 30 against a daily budget of 4", () => {
    expect(monthProjection(30, 4, 15, 30)).toEqual({
      projected: 60,
      budget: 120,
      percent: 50,
      colour: "green",
    });
  });

  it("on day 1 takes the usage of the day times the days of the month", () => {
    expect(monthProjection(2, 10, 1, 30)).toEqual({
      projected: 60,
      budget: 300,
      percent: 20,
      colour: "green",
    });
  });

  it("is 0 without usage, not NaN", () => {
    expect(monthProjection(0, 10, 5, 30)).toEqual({
      projected: 0,
      budget: 300,
      percent: 0,
      colour: "green",
    });
  });

  it("at month end is the usage itself", () => {
    expect(monthProjection(150, 10, 30, 30)?.projected).toBe(150);
  });

  it("can exceed 100 percent", () => {
    expect(monthProjection(30, 10, 2, 30)?.percent).toBe(150);
  });

  it("is null for informational windows (no month usage or no budget) and a budget of 0", () => {
    expect(monthProjection(undefined, 10, 5, 30)).toBeNull();
    expect(monthProjection(5, undefined, 5, 30)).toBeNull();
    expect(monthProjection(5, 0, 5, 30)).toBeNull();
    expect(monthProjection(5, 10, 0, 30)).toBeNull();
  });
});

describe("ratioColour", () => {
  it("is green below 0.8, yellow from 0.8 up to and including 1.0, red above", () => {
    expect(ratioColour(0)).toBe("green");
    expect(ratioColour(0.79)).toBe("green");
    expect(ratioColour(0.8)).toBe("yellow");
    expect(ratioColour(1.0)).toBe("yellow");
    expect(ratioColour(1.01)).toBe("red");
    expect(ratioColour(1.5)).toBe("red");
  });

  it("colours a 150 % projection red, 90 % yellow, 50 % green", () => {
    expect(monthProjection(30, 10, 2, 30)?.colour).toBe("red");
    expect(monthProjection(27, 10, 3, 30)?.colour).toBe("yellow");
    expect(monthProjection(15, 10, 3, 30)?.colour).toBe("green");
  });

  it("projectionColour turns a stale provider grey", () => {
    const p = monthProjection(30, 10, 2, 30)!;
    expect(projectionColour(p, "grey")).toBe("grey");
    expect(projectionColour(p, "green")).toBe("red");
  });
});

describe("windowProjection", () => {
  const w = {
    window: "seven_day",
    binding: true,
    unit: "percent",
    used: 54,
    limit: 100,
    resets_at: "2026-11-07T12:00:00Z",
    colour: "green",
    budget: 10,
    month_usage: 30,
  } satisfies WindowReport;

  it("uses the local day of the month and its length", () => {
    // 3 November 2026: 30 / 3 * 30 = 300 of 300.
    expect(windowProjection(w, new Date(2026, 10, 3, 10, 0))?.percent).toBe(100);
  });

  it("is null for a window without month usage", () => {
    expect(windowProjection({ ...w, month_usage: undefined }, new Date(2026, 10, 3))).toBeNull();
  });
});
