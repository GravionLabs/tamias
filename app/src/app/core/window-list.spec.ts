// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { TestBed, getTestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { WindowReport } from "./report";
import { WindowList } from "./window-list";

beforeAll(() => {
  getTestBed().initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
beforeEach(() => {
  // 7 November 2026: 7 of 30 days elapsed, so the month projection is fixed.
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date(2026, 10, 7, 12, 0));
});
afterEach(() => {
  TestBed.resetTestingModule();
  vi.useRealTimers();
});

function render(windows: WindowReport[]): HTMLElement {
  TestBed.overrideComponent(WindowList, {
    set: {
      templateUrl: undefined,
      styleUrl: undefined,
      template: readFileSync("src/app/core/window-list.html", "utf8"),
    },
  });
  const fixture = TestBed.createComponent(WindowList);
  fixture.componentRef.setInput("windows", windows);
  fixture.detectChanges();
  return fixture.nativeElement as HTMLElement;
}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim();

// Today 4% of 10% (ratio 0.4); month usage 80 at a daily budget of 10 projects to 343 of 300.
const SEVEN_DAY: WindowReport = {
  window: "seven_day",
  binding: true,
  unit: "percent",
  used: 54,
  limit: 100,
  resets_at: "2026-11-07T12:00:00Z",
  colour: "yellow",
  today_usage: 4,
  budget: 10,
  ratio: 0.4,
  month_usage: 80,
};

const MONTHLY: WindowReport = {
  window: "monthly",
  binding: true,
  unit: "requests",
  used: 26,
  limit: 300,
  resets_at: "2026-12-01T00:00:00Z",
  colour: "green",
};

describe("WindowList", () => {
  it("shows a short used and resets line under the window percentage of a percent window", () => {
    const el = render([SEVEN_DAY]);
    const block = el.querySelector(".window")!;
    expect(text(block.querySelector("[data-testid=percent]"))).toBe("54%");
    expect(text(block.querySelector("[data-testid=used]"))).toBe("54% used");
    expect(text(block.querySelector("[data-testid=resets]"))).toMatch(/^resets /);
    expect(text(block.querySelector(".detail"))).toMatch(/^54% used · resets /);
  });

  it("shows today against today's budget under the Today percentage", () => {
    const el = render([SEVEN_DAY]);
    expect(text(el.querySelector("[data-testid=today]"))).toBe("40%");
    expect(text(el.querySelector("[data-testid=today-detail]"))).toBe("4% of 10% today");
  });

  it("shows the projected month against the monthly quota under the Month (projected) percentage", () => {
    const el = render([SEVEN_DAY]);
    expect(text(el.querySelector("[data-testid=projection]"))).toBe("114%");
    expect(text(el.querySelector("[data-testid=projection-detail]"))).toBe(
      "≈ 343% of 300% by month end",
    );
  });

  it("never shows `of 100%` for a percent window", () => {
    const el = render([SEVEN_DAY]);
    expect(text(el)).not.toContain("of 100%");
  });

  it("shows a requests window as `26 of 300 used · resets`, without Today or Month lines", () => {
    const el = render([MONTHLY]);
    const block = el.querySelector(".window")!;
    expect(text(block.querySelector(".detail"))).toMatch(/^26 of 300 used · resets /);
    expect(block.querySelector("[data-testid=today-detail]")).toBeNull();
    expect(block.querySelector("[data-testid=projection-detail]")).toBeNull();
  });
});
