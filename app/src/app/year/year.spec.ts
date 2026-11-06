// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { TestBed, getTestBed } from "@angular/core/testing";
import { ActivatedRoute, Router, convertToParamMap, provideRouter } from "@angular/router";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { BehaviorSubject } from "rxjs";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { NOW } from "../calendar/calendar";
import { Ipc, type Unlisten } from "../core/ipc";
import type { MonthEntry, ProviderYear, YearReport } from "../core/report";
import { Year } from "./year";

beforeAll(() => {
  getTestBed().initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => TestBed.resetTestingModule());

class StubIpc extends Ipc {
  requests: number[] = [];

  constructor(public answer: (year: number) => unknown) {
    super();
  }

  override async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    if (command !== "year") throw new Error(`unexpected command ${command}`);
    const { year } = args as { year: number };
    this.requests.push(year);
    return this.answer(year) as T;
  }

  override async listen(): Promise<Unlisten> {
    return () => undefined;
  }
}

const EMPTY: MonthEntry = { month: 0, has_data: false, usage: 0, budget: 0, colour: "grey" };

/** Twelve months, `data` replacing the empty ones by month number. */
function months(data: Record<number, Partial<MonthEntry>>): MonthEntry[] {
  return Array.from({ length: 12 }, (_, i) => ({ ...EMPTY, month: i + 1, ...data[i + 1] }));
}

const CLAUDE: ProviderYear = {
  provider: "claude",
  status: "ok",
  unit: "percent",
  months: months({
    1: { has_data: true, usage: 200, budget: 310, ratio: 0.65, colour: "green" },
    2: { has_data: true, usage: 280, budget: 280, ratio: 1, colour: "yellow" },
    3: { has_data: true, usage: 450, budget: 300, ratio: 1.5, colour: "red" },
  }),
};

const COPILOT_UNAVAILABLE: ProviderYear = {
  provider: "copilot",
  status: "unavailable",
  unit: "requests",
  months: [],
};

function year(y: number, ...providers: ProviderYear[]): YearReport {
  return { year: y, generated_at: "2026-11-03T10:05:00Z", providers };
}

const NOV_3 = new Date(2026, 10, 3, 10, 5);

async function mount(answer: (year: number) => unknown, route = "2026", now = NOV_3) {
  const ipc = new StubIpc(answer);
  const params = new BehaviorSubject(convertToParamMap({ year: route }));
  TestBed.configureTestingModule({
    providers: [
      { provide: Ipc, useValue: ipc },
      { provide: NOW, useValue: () => now },
      provideRouter([]),
      { provide: ActivatedRoute, useValue: { paramMap: params } },
    ],
  });
  TestBed.overrideComponent(Year, {
    set: {
      templateUrl: undefined,
      styleUrl: undefined,
      template: readFileSync("src/app/year/year.html", "utf8"),
    },
  });
  const router = TestBed.inject(Router);
  const navigate = vi.spyOn(router, "navigate").mockResolvedValue(true);
  const fixture = TestBed.createComponent(Year);
  const settle = async () => {
    await fixture.whenStable();
    fixture.detectChanges();
  };
  await settle();
  const el = fixture.nativeElement as HTMLElement;
  const click = async (testid: string) => {
    el.querySelector<HTMLElement>(`[data-testid=${testid}]`)!.click();
    await settle();
  };
  /** The route's year changes, as after a navigation. */
  const route$ = async (value: string) => {
    params.next(convertToParamMap({ year: value }));
    await settle();
  };
  return { ipc, el, click, navigate, route$ };
}

function bars(el: HTMLElement, provider = "claude"): HTMLElement[] {
  return [...el.querySelectorAll<HTMLElement>(`[data-provider=${provider}] [data-testid=bar]`)];
}

describe("Year", () => {
  it("shows the year and one bar per month for a provider with data", async () => {
    const { el, ipc } = await mount((y) => year(y, CLAUDE, COPILOT_UNAVAILABLE));
    expect(ipc.requests).toEqual([2026]);
    expect(el.querySelector("[data-testid=title]")?.textContent).toBe("2026");
    expect(bars(el)).toHaveLength(12);
    // The unavailable provider has no panel.
    expect(el.querySelectorAll("[data-testid=panel]")).toHaveLength(1);
  });

  it("colours the bars by ratio and sizes the fill against the budget", async () => {
    const { el } = await mount((y) => year(y, CLAUDE));
    const [jan, feb, mar] = bars(el);
    expect(jan.dataset["colour"]).toBe("green");
    expect(feb.dataset["colour"]).toBe("yellow");
    expect(mar.dataset["colour"]).toBe("red");
    expect(jan.querySelector<HTMLElement>(".fill")?.style.height).toMatch(/^43\.3/);
    expect(mar.querySelector<HTMLElement>(".fill")?.style.height).toBe("100%");
    expect(jan.querySelector(".percent")?.textContent).toBe("65 %");
  });

  it("leaves months without data and future months empty", async () => {
    const future = months({
      11: { has_data: true, usage: 5, budget: 5, ratio: 1, colour: "green" },
      12: { has_data: true, usage: 5, budget: 5, ratio: 1, colour: "green" },
    });
    const { el } = await mount((y) => year(y, { ...CLAUDE, months: future }));
    const all = bars(el);
    // January has no data, December is after the current month: both empty.
    expect(all[0].classList).toContain("empty");
    expect(all[0].querySelector(".fill")).toBeNull();
    expect(all[10].querySelector(".fill")).not.toBeNull();
    expect(all[11].classList).toContain("empty");
    expect(all[11].querySelector(".fill")).toBeNull();
    expect(all[11].querySelector(".percent")?.textContent).toBe("–");
  });

  it("shows every month of a future year empty", async () => {
    const { el } = await mount((y) => year(y, CLAUDE), "2027");
    expect(bars(el).every((b) => b.classList.contains("empty"))).toBe(true);
  });

  it("opens the month in the calendar from a bar", async () => {
    const { el } = await mount((y) => year(y, CLAUDE));
    const mar = bars(el)[2] as HTMLAnchorElement;
    expect(mar.tagName).toBe("A");
    expect(mar.getAttribute("href")).toBe("/?month=2026-03");
    // A month without data opens as well.
    expect(bars(el)[5].getAttribute("href")).toBe("/?month=2026-06");
  });

  it("navigates to the previous, next and current year", async () => {
    const { click, navigate, el, route$, ipc } = await mount((y) => year(y, CLAUDE), "2025");
    expect(el.querySelector<HTMLButtonElement>("[data-testid=current]")?.disabled).toBe(false);
    await click("prev");
    expect(navigate).toHaveBeenLastCalledWith(["/year", 2024]);
    await click("next");
    expect(navigate).toHaveBeenLastCalledWith(["/year", 2026]);
    await click("current");
    expect(navigate).toHaveBeenLastCalledWith(["/year", 2026]);
    await route$("2026");
    expect(ipc.requests).toEqual([2025, 2026]);
    expect(el.querySelector("[data-testid=title]")?.textContent).toBe("2026");
    expect(el.querySelector<HTMLButtonElement>("[data-testid=current]")?.disabled).toBe(true);
  });

  it("falls back to the current year for an invalid route year", async () => {
    const { el, ipc } = await mount((y) => year(y, CLAUDE), "abc");
    expect(ipc.requests).toEqual([2026]);
    expect(el.querySelector("[data-testid=title]")?.textContent).toBe("2026");
  });

  it("says so without a provider with data", async () => {
    const { el } = await mount((y) => year(y, COPILOT_UNAVAILABLE));
    expect(el.querySelector("[data-testid=no-data]")).not.toBeNull();
  });

  it("shows the error when the year cannot be read", async () => {
    const { el } = await mount(() => {
      throw new Error("boom");
    });
    expect(el.querySelector("[data-testid=error]")?.textContent).toContain("boom");
  });
});
