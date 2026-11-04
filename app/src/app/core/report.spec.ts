import { describe, expect, it } from "vitest";
import {
  amount,
  localTime,
  providerLabel,
  staleText,
  todayText,
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
