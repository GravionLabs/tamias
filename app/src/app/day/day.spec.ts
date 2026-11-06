// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { TestBed, getTestBed } from "@angular/core/testing";
import { ActivatedRoute, convertToParamMap, provideRouter } from "@angular/router";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { of } from "rxjs";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { NOW } from "../calendar/calendar";
import { Ipc, type Unlisten } from "../core/ipc";
import { STORE_CHANGED } from "../core/store-events";
import type { DayReport, ProviderDay, Report } from "../core/report";
import { WindowList } from "../core/window-list";
import { Day, dayBounds, emptyReason } from "./day";

beforeAll(() => {
  getTestBed().initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => TestBed.resetTestingModule());

class StubIpc extends Ipc {
  requests: string[] = [];
  private readonly handlers = new Map<string, Set<() => void>>();

  constructor(
    public answer: (date: string) => unknown,
    public todayAnswer: () => unknown = () => TODAY,
  ) {
    super();
  }

  override async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    if (command === "today") return this.todayAnswer() as T;
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

const text = (el: Element | null) => el?.textContent?.trim();

/** Today's report: Claude with a budgeted 7-day window, Copilot with a monthly one. */
const TODAY: Report = {
  generated_at: at(12),
  providers: [
    {
      provider: "claude",
      status: "ok",
      windows: [
        {
          window: "seven_day",
          binding: true,
          unit: "percent",
          used: 54,
          limit: 100,
          resets_at: "2026-11-10T00:00:00Z",
          colour: "yellow",
          today_usage: 4,
          budget: 10,
          ratio: 0.4,
          month_usage: 12,
        },
        {
          window: "five_hour",
          binding: false,
          unit: "percent",
          used: 30,
          limit: 100,
          resets_at: "2026-11-03T15:00:00Z",
          colour: "green",
        },
      ],
    },
    {
      provider: "copilot",
      status: "ok",
      windows: [
        {
          window: "monthly",
          binding: true,
          unit: "requests",
          used: 21,
          limit: 100,
          resets_at: "2026-12-01T00:00:00Z",
          colour: "red",
        },
      ],
    },
  ],
} as Report;

function day(date: string, ...providers: ProviderDay[]): DayReport {
  return { date, generated_at: at(12), providers };
}

const NOV_3 = new Date(2026, 10, 3, 12);

async function mount(
  answer: (date: string) => unknown,
  date = "2026-11-03",
  today: () => unknown = () => TODAY,
) {
  const ipc = new StubIpc(answer, today);
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
  TestBed.overrideComponent(WindowList, {
    set: {
      templateUrl: undefined,
      styleUrl: undefined,
      template: readFileSync("src/app/core/window-list.html", "utf8"),
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

  it("shows the summary of the day and no chart", async () => {
    const { el } = await mount((d) => day(d, CLAUDE, COPILOT_UNAVAILABLE));
    const claude = panel(el, "claude");
    expect(claude.querySelector("[data-testid=summary]")?.textContent).toContain("6% of 10%");
    expect(claude.querySelector("[data-testid=empty]")).toBeNull();
    expect(el.querySelector("svg")).toBeNull();
    expect(el.querySelector("[data-testid=chart]")).toBeNull();
  });

  it("shows the windows like the tray card, per provider", async () => {
    const { el } = await mount((d) => day(d, CLAUDE, COPILOT_OK));
    const claude = panel(el, "claude");
    const sevenDay = claude.querySelector("[data-window=seven_day]")!;
    expect(text(sevenDay.querySelector(".label"))).toBe("7-day window");
    expect(text(sevenDay.querySelector("[data-testid=percent]"))).toBe("54%");
    expect(sevenDay.querySelector<HTMLElement>(".bar")?.style.width).toBe("54%");
    expect(text(sevenDay.querySelector("[data-testid=today]"))).toBe("40%");
    expect(sevenDay.querySelector("[data-testid=projection]")).not.toBeNull();
    expect(text(sevenDay.querySelector("[data-testid=used]"))).toBe("54% of 100% used");
    expect(text(sevenDay.querySelector("[data-testid=resets]"))).toMatch(/^resets /);
    expect(claude.querySelector("[data-window=five_hour]")).not.toBeNull();
    const copilot = panel(el, "copilot");
    expect(text(copilot.querySelector("[data-window=monthly] [data-testid=percent]"))).toBe("21%");
    expect(claude.querySelector("[data-window=monthly]")).toBeNull();
  });

  it("still shows the day when today's report cannot be read", async () => {
    const { el } = await mount(
      (d) => day(d, CLAUDE),
      "2026-11-03",
      () => {
        throw { message: "boom", busy: false };
      },
    );
    const claude = panel(el, "claude");
    expect(claude.querySelector("[data-testid=summary]")).not.toBeNull();
    expect(claude.querySelector("[data-window]")).toBeNull();
    expect(el.querySelector("[data-testid=error]")).toBeNull();
  });

  it("shows both providers side by side, the unavailable one as an empty state with the reason", async () => {
    const noCopilot = {
      ...TODAY,
      providers: [TODAY.providers[0], { provider: "copilot", status: "unavailable", windows: [] }],
    };
    const { el } = await mount(
      (d) => day(d, CLAUDE, COPILOT_UNAVAILABLE),
      "2026-11-03",
      () => noCopilot,
    );
    expect(el.querySelectorAll("[data-testid=panel]")).toHaveLength(2);
    const copilot = panel(el, "copilot");
    expect(copilot.querySelector("[data-window]")).toBeNull();
    expect(copilot.querySelector("[data-testid=empty]")?.textContent).toBe(
      "Not set up or switched off.",
    );
  });

  it("shows the empty state for a day without data", async () => {
    const none: ProviderDay = { ...CLAUDE, points: [], usage: undefined, budget: undefined };
    const { el } = await mount((d) => day(d, none, COPILOT_UNAVAILABLE));
    const claude = panel(el, "claude");
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

  it("keeps the data of a stale provider and shows since when it is stale", async () => {
    const stale: ProviderDay = { ...CLAUDE, status: "stale", stale_since: at(9), colour: "grey" };
    const { el } = await mount((d) => day(d, stale, COPILOT_UNAVAILABLE));
    const claude = panel(el, "claude");
    expect(claude.querySelector("[data-testid=summary]")).not.toBeNull();
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
    expect(el.querySelectorAll("[data-testid=panel]")).toHaveLength(1);
    fail = { message: "locked", busy: true };
    ipc.emit(STORE_CHANGED);
    await settle();
    expect(el.querySelector("[data-testid=error]")).toBeNull();
    expect(el.querySelectorAll("[data-testid=panel]")).toHaveLength(1);
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

describe("emptyReason", () => {
  const base: ProviderDay = { provider: "claude", status: "ok", unit: "percent", points: [] };
  const now = new Date(2026, 10, 3, 10, 5);

  it("says an unavailable provider is not set up", () => {
    expect(emptyReason({ ...base, status: "unavailable" }, now)).toBe(
      "Not set up or switched off.",
    );
  });

  it("says a day without points has no data, and adds the staleness", () => {
    expect(emptyReason(base, now)).toBe("No data for this day.");
    expect(emptyReason({ ...base, status: "stale" }, now)).toBe(
      "No data for this day. The provider is stale, never collected.",
    );
  });

  it("has no reason with data", () => {
    expect(emptyReason({ ...base, points: [{ at: at(1), usage: 0 }] }, now)).toBeNull();
  });
});
