// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { TestBed, getTestBed } from "@angular/core/testing";
import { ActivatedRoute, convertToParamMap, provideRouter } from "@angular/router";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { of } from "rxjs";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { NOW } from "../calendar/calendar";
import { Ipc, type Unlisten } from "../core/ipc";
import { STORE_CHANGED } from "../core/store-events";
import type { DayReport, ProviderDay } from "../core/report";
import { Day } from "./day";

beforeAll(() => {
  getTestBed().initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => TestBed.resetTestingModule());

class StubIpc extends Ipc {
  requests: string[] = [];
  private readonly handlers = new Map<string, Set<() => void>>();

  constructor(public answer: (date: string) => unknown) {
    super();
  }

  override async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    if (command !== "day") throw new Error(`unexpected command ${command}`);
    const { date } = args as { date: string };
    this.requests.push(date);
    return this.answer(date) as T;
  }

  override async listen(event: string, handler: () => void): Promise<Unlisten> {
    const handlers = this.handlers.get(event) ?? new Set();
    this.handlers.set(event, handlers.add(handler));
    return () => handlers.delete(handler);
  }

  emit(event: string): void {
    for (const handler of this.handlers.get(event) ?? []) handler();
  }
}

const at = (hour: number) => new Date(2026, 10, 3, hour).toISOString();

const CLAUDE: ProviderDay = {
  provider: "claude",
  status: "ok",
  unit: "percent",
  points: [
    { at: at(0), usage: 0 },
    { at: at(6), usage: 2 },
    { at: at(12), usage: 6 },
  ],
  usage: 6,
  budget: 10,
  colour: "green",
  five_hour: { used: 30, limit: 100, colour: "green" },
};

const COPILOT_OK: ProviderDay = {
  provider: "copilot",
  status: "ok",
  unit: "requests",
  points: [{ at: at(6), usage: 5 }],
  usage: 5,
  budget: 10,
  colour: "green",
};

const COPILOT_UNAVAILABLE: ProviderDay = {
  provider: "copilot",
  status: "unavailable",
  unit: "requests",
  points: [],
};

function day(date: string, ...providers: ProviderDay[]): DayReport {
  return { date, generated_at: at(12), providers };
}

const NOV_3 = new Date(2026, 10, 3, 12);

async function mount(answer: (date: string) => unknown, date = "2026-11-03") {
  const ipc = new StubIpc(answer);
  TestBed.configureTestingModule({
    providers: [
      provideRouter([]),
      { provide: Ipc, useValue: ipc },
      { provide: NOW, useValue: () => NOV_3 },
      { provide: ActivatedRoute, useValue: { paramMap: of(convertToParamMap({ date })) } },
    ],
  });
  TestBed.overrideComponent(Day, {
    set: {
      templateUrl: undefined,
      styleUrl: undefined,
      template: readFileSync("src/app/day/day.html", "utf8"),
    },
  });
  const fixture = TestBed.createComponent(Day);
  const settle = async () => {
    await fixture.whenStable();
    fixture.detectChanges();
  };
  await settle();
  return { ipc, el: fixture.nativeElement as HTMLElement, settle };
}

function panel(el: HTMLElement, provider: string): HTMLElement {
  const found = el.querySelector<HTMLElement>(`[data-testid=panel][data-provider=${provider}]`);
  if (!found) throw new Error(`no panel for ${provider}`);
  return found;
}

describe("Day", () => {
  it("asks for the date of the route", async () => {
    const { ipc } = await mount((d) => day(d, CLAUDE, COPILOT_UNAVAILABLE));
    expect(ipc.requests).toEqual(["2026-11-03"]);
  });

  it("draws the usage step line and the budget line at the mapped coordinates", async () => {
    const { el } = await mount((d) => day(d, CLAUDE, COPILOT_UNAVAILABLE));
    const claude = panel(el, "claude");
    // 00:00 -> 48, 06:00 -> 153, 12:00 -> 258; y = 232 - 22 * usage; the line runs on to now (12:00).
    expect(claude.querySelector("[data-testid=usage-line]")?.getAttribute("d")).toBe(
      "M48 232 H153 V188 H258 V100",
    );
    const budget = claude.querySelector("[data-testid=budget-line]");
    expect(budget?.getAttribute("y1")).toBe("12");
    expect(budget?.getAttribute("x1")).toBe("48");
    expect(budget?.getAttribute("x2")).toBe("468");
    expect(claude.querySelector("[data-testid=summary]")?.textContent).toContain("6% of 10%");
    expect(claude.querySelector("[data-testid=five-hour]")).toBeNull();
    expect(claude.querySelector("[data-testid=empty]")).toBeNull();
  });

  it("colours a single curve like the day", async () => {
    const red: ProviderDay = { ...COPILOT_OK, colour: "red" };
    const { el } = await mount((d) => day(d, CLAUDE, red));
    expect(
      panel(el, "copilot").querySelector("[data-testid=usage-line]")?.getAttribute("class"),
    ).toContain("colour-red");
    expect(panel(el, "copilot").querySelector("[data-testid=five-hour-line]")).toBeNull();
  });

  describe("two curves", () => {
    const WITH_WINDOW: ProviderDay = {
      ...CLAUDE,
      five_hour_points: [
        { at: at(0), used: 0, limit: 100 },
        { at: at(6), used: 50, limit: 100 },
        { at: at(7), used: 0, limit: 100 },
      ],
    };

    it("scales the day to the budget and the window to its limit on one percent axis", async () => {
      const { el } = await mount((d) => day(d, WITH_WINDOW, COPILOT_UNAVAILABLE));
      const claude = panel(el, "claude");
      // Day: 0, 2, 6 of 10 -> 0, 20, 60 %; axis top 100 %: y = 232 - 2.2 * percent.
      expect(claude.querySelector("[data-testid=usage-line]")?.getAttribute("d")).toBe(
        "M48 232 H153 V188 H258 V100",
      );
      // Window: 0, 50, 0 % at 00:00, 06:00, 07:00; the drop is a reset.
      expect(claude.querySelector("[data-testid=five-hour-line]")?.getAttribute("d")).toBe(
        "M48 232 H153 V122 H170.5 V232 H258",
      );
      // Budget at 100 %, the axis top.
      expect(claude.querySelector("[data-testid=budget-line]")?.getAttribute("y1")).toBe("12");
      expect(claude.querySelectorAll("[data-testid=chart]").length).toBe(1);
    });

    it("names the curves in the legend with their value and drops the old text line", async () => {
      const { el } = await mount((d) => day(d, WITH_WINDOW, COPILOT_UNAVAILABLE));
      const claude = panel(el, "claude");
      expect(claude.querySelector("[data-testid=legend-day]")?.textContent).toBe(
        "Day vs budget 60%",
      );
      expect(claude.querySelector("[data-testid=legend-five-hour]")?.textContent).toBe(
        "5-hour window 30%",
      );
      expect(claude.querySelector("[data-testid=five-hour]")).toBeNull();
      expect(claude.querySelector("[data-testid=summary]")?.textContent).toContain("6% of 10%");
    });

    it("gives the curves different theme colours", async () => {
      const { el } = await mount((d) => day(d, WITH_WINDOW, COPILOT_UNAVAILABLE));
      const claude = panel(el, "claude");
      const dayClass = claude.querySelector("[data-testid=usage-line]")?.getAttribute("class");
      const windowClass = claude
        .querySelector("[data-testid=five-hour-line]")
        ?.getAttribute("class");
      expect(dayClass).toContain("curve-day");
      expect(windowClass).toContain("curve-five-hour");
      const css = readFileSync("src/app/day/day.css", "utf8");
      expect(css).toMatch(/path\.curve-day\s*{\s*stroke: var\(--h-blue-500\);/);
      expect(css).toMatch(/path\.curve-five-hour\s*{\s*stroke: var\(--h-orange-500\);/);
    });

    it("draws only the day curve against the budget on a day without five-hour points", async () => {
      const none: ProviderDay = { ...CLAUDE, five_hour_points: [] };
      const { el } = await mount((d) => day(d, none, COPILOT_UNAVAILABLE));
      const claude = panel(el, "claude");
      expect(claude.querySelector("[data-testid=five-hour-line]")).toBeNull();
      expect(claude.querySelector("[data-testid=legend-five-hour]")).toBeNull();
      expect(claude.querySelector("[data-testid=legend-day]")?.textContent).toBe(
        "Day vs budget 60%",
      );
      expect(claude.querySelector("[data-testid=usage-line]")?.getAttribute("d")).toBe(
        "M48 232 H153 V188 H258 V100",
      );
    });
  });

  it("shows both providers side by side, the unavailable one as an empty state with the reason", async () => {
    const { el } = await mount((d) => day(d, CLAUDE, COPILOT_UNAVAILABLE));
    expect(el.querySelectorAll("[data-testid=panel]")).toHaveLength(2);
    const copilot = panel(el, "copilot");
    expect(copilot.querySelector("[data-testid=chart]")).toBeNull();
    expect(copilot.querySelector("[data-testid=empty]")?.textContent).toBe(
      "Not set up or switched off.",
    );
  });

  it("shows the empty state for a day without data", async () => {
    const none: ProviderDay = { ...CLAUDE, points: [], usage: undefined, budget: undefined };
    const { el } = await mount((d) => day(d, none, COPILOT_UNAVAILABLE));
    const claude = panel(el, "claude");
    expect(claude.querySelector("[data-testid=chart]")).toBeNull();
    expect(claude.querySelector("[data-testid=empty]")?.textContent).toBe("No data for this day.");
  });

  it("names the staleness in the empty state of a stale provider", async () => {
    const stale: ProviderDay = {
      ...CLAUDE,
      status: "stale",
      points: [],
      usage: undefined,
      budget: undefined,
    };
    const { el } = await mount((d) => day(d, stale, COPILOT_UNAVAILABLE));
    expect(panel(el, "claude").querySelector("[data-testid=empty]")?.textContent).toBe(
      "No data for this day. The provider is stale, never collected.",
    );
  });

  it("keeps the curve of a stale provider and shows since when it is stale", async () => {
    const stale: ProviderDay = { ...CLAUDE, status: "stale", stale_since: at(9), colour: "grey" };
    const { el } = await mount((d) => day(d, stale, COPILOT_UNAVAILABLE));
    const claude = panel(el, "claude");
    expect(claude.querySelector("[data-testid=chart]")).not.toBeNull();
    expect(claude.querySelector("[data-testid=stale]")?.textContent).toBe("stale since 09:00");
  });

  it("links back to the month of the day", async () => {
    const { el } = await mount((d) => day(d, CLAUDE, COPILOT_UNAVAILABLE));
    const back = el.querySelector<HTMLAnchorElement>("[data-testid=back]")!;
    expect(back.getAttribute("href")).toBe("/?month=2026-11");
  });

  it("shows an error, and a busy store keeps what is shown", async () => {
    let fail: unknown = { message: "database is corrupt", busy: false };
    const { el, ipc, settle } = await mount((d) => {
      if (fail) throw fail;
      return day(d, CLAUDE);
    });
    expect(el.querySelector("[data-testid=error]")?.textContent).toContain("database is corrupt");
    fail = null;
    ipc.emit(STORE_CHANGED);
    await settle();
    expect(el.querySelector("[data-testid=error]")).toBeNull();
    expect(el.querySelectorAll("[data-testid=chart]")).toHaveLength(1);
    fail = { message: "locked", busy: true };
    ipc.emit(STORE_CHANGED);
    await settle();
    expect(el.querySelector("[data-testid=error]")).toBeNull();
    expect(el.querySelectorAll("[data-testid=chart]")).toHaveLength(1);
  });

  it("reloads on every store-changed event", async () => {
    const { ipc, settle } = await mount((d) => day(d, CLAUDE));
    ipc.emit(STORE_CHANGED);
    await settle();
    expect(ipc.requests).toHaveLength(2);
  });

  it("is not bound by a fixed maximum width", () => {
    const css = readFileSync("src/app/day/day.css", "utf8");
    expect(css).not.toMatch(/max-width\s*:\s*\d+px/);
  });

  it("draws the chart in the measured size of its box, so text is not stretched", async () => {
    const observers: { cb: ResizeObserverCallback; el: Element }[] = [];
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(private cb: ResizeObserverCallback) {}
        observe(el: Element) {
          observers.push({ cb: this.cb, el });
        }
        disconnect() {}
        unobserve() {}
      },
    );
    try {
      const { el, settle } = await mount((d) => day(d, CLAUDE));
      const resize = async (width: number, height: number) => {
        for (const o of observers) {
          o.cb(
            [{ target: o.el, contentRect: { width, height } } as unknown as ResizeObserverEntry],
            {} as ResizeObserver,
          );
        }
        await settle();
      };
      const svg = () => el.querySelector("[data-testid=chart]")!;
      await resize(960, 400);
      expect(svg().getAttribute("viewBox")).toBe("0 0 960 400");
      await resize(500, 260);
      expect(svg().getAttribute("viewBox")).toBe("0 0 500 260");
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
