// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { TestBed, getTestBed } from "@angular/core/testing";
import { provideRouter } from "@angular/router";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { Ipc, type Unlisten } from "../core/ipc";
import { STORE_CHANGED } from "../core/store-events";
import type { MonthReport, ProviderMonth } from "../core/report";
import { Calendar, FIRST_DAY, NOW } from "./calendar";
import type { FirstDay } from "./grid";

beforeAll(() => {
  getTestBed().initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => TestBed.resetTestingModule());

/** An `Ipc` whose `month` answers `answer(year, month)`; `emit` fires the events the view listens to. */
class StubIpc extends Ipc {
  requests: [number, number][] = [];
  private readonly handlers = new Map<string, Set<() => void>>();

  constructor(public answer: (year: number, month: number) => unknown) {
    super();
  }

  override async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    if (command !== "month") throw new Error(`unexpected command ${command}`);
    const { year, month } = args as { year: number; month: number };
    this.requests.push([year, month]);
    return this.answer(year, month) as T;
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

const CLAUDE: ProviderMonth = {
  provider: "claude",
  status: "ok",
  unit: "percent",
  days: [
    { date: "2026-11-01", usage: 2, budget: 10, ratio: 0.2, colour: "green" },
    { date: "2026-11-02", usage: 9, budget: 10, ratio: 0.9, colour: "yellow" },
    { date: "2026-11-03", usage: 15, budget: 10, ratio: 1.5, colour: "red" },
  ],
};

const COPILOT_UNAVAILABLE: ProviderMonth = {
  provider: "copilot",
  status: "unavailable",
  unit: "requests",
  days: [],
};

function month(year: number, m: number, ...providers: ProviderMonth[]): MonthReport {
  return { year, month: m, generated_at: "2026-11-03T10:05:00Z", providers };
}

const NOV_3 = new Date(2026, 10, 3, 10, 5);

async function mount(
  answer: (year: number, month: number) => unknown,
  options: { firstDay?: FirstDay; now?: Date } = {},
) {
  const ipc = new StubIpc(answer);
  TestBed.configureTestingModule({
    providers: [
      { provide: Ipc, useValue: ipc },
      { provide: FIRST_DAY, useValue: options.firstDay ?? 1 },
      { provide: NOW, useValue: () => options.now ?? NOV_3 },
      provideRouter([]),
    ],
  });
  TestBed.overrideComponent(Calendar, {
    set: {
      templateUrl: undefined,
      styleUrl: undefined,
      template: readFileSync("src/app/calendar/calendar.html", "utf8"),
    },
  });
  const fixture = TestBed.createComponent(Calendar);
  /** Waits for the pending loads and renders. */
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
  return { ipc, el, settle, click };
}

function cells(el: HTMLElement): HTMLElement[] {
  return [...el.querySelectorAll<HTMLElement>("[data-testid=cell]")];
}

function cell(el: HTMLElement, date: string): HTMLElement {
  const found = el.querySelector<HTMLElement>(`[data-date="${date}"]`);
  if (!found) throw new Error(`no cell for ${date}`);
  return found;
}

function chip(el: HTMLElement, date: string, provider: string): HTMLElement {
  return cell(el, date).querySelector<HTMLElement>(`[data-provider=${provider}]`)!;
}

describe("Calendar", () => {
  it("renders 30 cells for November 2026, starting in the Sunday column of a Monday-first week", async () => {
    const { el } = await mount((y, m) => month(y, m, CLAUDE, COPILOT_UNAVAILABLE));
    expect(cells(el)).toHaveLength(30);
    // 2026-11-01 is a Sunday: six blanks before it with weeks starting on Monday.
    expect(el.querySelectorAll(".cell.blank")).toHaveLength(6);
    expect(el.querySelectorAll("[role=columnheader]")).toHaveLength(7);
    expect(cell(el, "2026-11-01").querySelector(".day")?.textContent).toBe("1");
  });

  it("starts the week on the given first day", async () => {
    const { el } = await mount((y, m) => month(y, m, CLAUDE), { firstDay: 7 });
    expect(el.querySelectorAll(".cell.blank")).toHaveLength(0);
  });

  it("renders 31 and 28 cells for other months", async () => {
    const dec = await mount((y, m) => month(y, m, CLAUDE), { now: new Date(2026, 11, 5) });
    expect(cells(dec.el)).toHaveLength(31);
    // 2026-12-01 is a Tuesday: one blank on a Monday-first week.
    expect(dec.el.querySelectorAll(".cell.blank")).toHaveLength(1);
    TestBed.resetTestingModule();
    const feb = await mount((y, m) => month(y, m, CLAUDE), { now: new Date(2027, 1, 5) });
    expect(cells(feb.el)).toHaveLength(28);
    expect(feb.el.querySelectorAll(".cell.blank")).toHaveLength(0);
  });

  it("colours the chips from the data and shows both providers side by side", async () => {
    const { el } = await mount((y, m) => month(y, m, CLAUDE, COPILOT_UNAVAILABLE));
    expect(chip(el, "2026-11-01", "claude").dataset["colour"]).toBe("green");
    expect(chip(el, "2026-11-02", "claude").dataset["colour"]).toBe("yellow");
    expect(chip(el, "2026-11-03", "claude").dataset["colour"]).toBe("red");
    expect(chip(el, "2026-11-03", "claude").textContent).toBe("C");
    expect(chip(el, "2026-11-03", "copilot").textContent).toBe("CP");
  });

  it("keeps future days and days without data neutral", async () => {
    const { el } = await mount((y, m) =>
      month(y, m, {
        ...CLAUDE,
        // A day after today in the data must still not be coloured.
        days: [...CLAUDE.days, { date: "2026-11-05", usage: 1, budget: 10, colour: "green" }],
      }),
    );
    expect(chip(el, "2026-11-05", "claude").dataset["colour"]).toBe("neutral");
    expect(chip(el, "2026-11-20", "claude").dataset["colour"]).toBe("neutral");
    expect(chip(el, "2026-11-04", "claude").dataset["colour"]).toBe("neutral");
  });

  it("shows Copilot as unavailable and neutral", async () => {
    const { el } = await mount((y, m) => month(y, m, CLAUDE, COPILOT_UNAVAILABLE));
    const copilot = chip(el, "2026-11-01", "copilot");
    expect(copilot.dataset["colour"]).toBe("neutral");
    expect(copilot.title).toBe("Copilot: unavailable");
  });

  it("shows usage over budget in the unit as the tooltip", async () => {
    const { el } = await mount((y, m) => month(y, m, CLAUDE));
    expect(chip(el, "2026-11-02", "claude").title).toBe("Claude: 9% of 10%");
  });

  it("greys today and shows the stale hint when a provider is stale", async () => {
    const stale: ProviderMonth = {
      ...CLAUDE,
      status: "stale",
      stale_since: "2026-11-03T09:00:00Z",
    };
    const { el } = await mount((y, m) => month(y, m, stale, COPILOT_UNAVAILABLE));
    expect(chip(el, "2026-11-03", "claude").dataset["colour"]).toBe("grey");
    expect(cell(el, "2026-11-03").querySelector("[data-testid=stale]")?.textContent).toMatch(
      /^stale since (\d{4}-\d{2}-\d{2} )?\d{2}:\d{2}$/,
    );
    // Earlier days keep their colour, and only today has the hint.
    expect(chip(el, "2026-11-02", "claude").dataset["colour"]).toBe("yellow");
    expect(el.querySelectorAll("[data-testid=stale]")).toHaveLength(1);
  });

  it("shows no stale hint for an ok provider", async () => {
    const { el } = await mount((y, m) => month(y, m, CLAUDE));
    expect(el.querySelector("[data-testid=stale]")).toBeNull();
  });

  it("navigates to the previous and next month and back to today", async () => {
    const { el, ipc, click } = await mount((y, m) => month(y, m, CLAUDE));
    expect(ipc.requests).toEqual([[2026, 11]]);

    await click("prev");
    expect(ipc.requests.at(-1)).toEqual([2026, 10]);
    expect(cells(el)).toHaveLength(31);
    // Nothing of November's data leaks into October.
    expect(chip(el, "2026-10-01", "claude").dataset["colour"]).toBe("neutral");

    await click("next");
    await click("next");
    expect(ipc.requests.at(-1)).toEqual([2026, 12]);
    expect(cells(el)).toHaveLength(31);

    await click("today");
    expect(ipc.requests.at(-1)).toEqual([2026, 11]);
    expect(cells(el)).toHaveLength(30);
  });

  it("crosses the year boundary", async () => {
    const { ipc, click } = await mount((y, m) => month(y, m, CLAUDE), {
      now: new Date(2026, 0, 10),
    });
    await click("prev");
    expect(ipc.requests.at(-1)).toEqual([2025, 12]);
  });

  it("reloads on a store-changed event", async () => {
    const { el, ipc, settle } = await mount((y, m) => month(y, m, CLAUDE));
    expect(ipc.requests).toHaveLength(1);
    ipc.answer = (y, m) =>
      month(y, m, {
        ...CLAUDE,
        days: [{ ...CLAUDE.days[0], date: "2026-11-03", colour: "green" }],
      });
    ipc.emit(STORE_CHANGED);
    await settle();
    expect(ipc.requests).toHaveLength(2);
    expect(chip(el, "2026-11-03", "claude").dataset["colour"]).toBe("green");
  });

  it("shows the error when the backend fails", async () => {
    const { el } = await mount(() => {
      throw new Error("database is locked");
    });
    expect(el.querySelector("[data-testid=error]")?.textContent).toContain("database is locked");
    expect(el.querySelector("[data-testid=grid]")).toBeNull();
  });

  it("keeps the loading state on a busy store", async () => {
    const { el } = await mount(() => {
      throw { message: "busy", busy: true };
    });
    expect(el.querySelector("[data-testid=error]")).toBeNull();
    expect(el.querySelector("[role=status]")?.textContent).toContain("Reading the month");
  });
});
