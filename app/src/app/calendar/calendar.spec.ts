// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { TestBed, getTestBed } from "@angular/core/testing";
import { ActivatedRoute, Router, convertToParamMap, provideRouter } from "@angular/router";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { Ipc, type ServiceStatus, type Unlisten } from "../core/ipc";
import { STORE_CHANGED } from "../core/store-events";
import type { MonthReport, ProviderMonth } from "../core/report";
import { STATUS_RETRY_DELAY, STATUS_TIMEOUT } from "../core/collection";
import { Calendar, FIRST_DAY, NOW } from "./calendar";
import type { FirstDay } from "./grid";

beforeAll(() => {
  getTestBed().initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => TestBed.resetTestingModule());

const INSTALLED: ServiceStatus = { state: "installed", message: null, exe: "/usr/bin/tamias" };
const NOT_INSTALLED: ServiceStatus = { state: "not_installed", message: null, exe: null };

/**
 * An `Ipc` whose `month` answers `answer(year, month)`; `emit` fires the events the view listens to.
 * `service` is what `tamias service status` reports, `install` what `service_install` does.
 */
class StubIpc extends Ipc {
  requests: [number, number][] = [];
  serviceCalls: string[] = [];
  service: ServiceStatus = INSTALLED;
  /** Answers of the next status calls, before `service`. */
  serviceQueue: ServiceStatus[] = [];
  /** The status call never answers. */
  hang = false;
  install: () => void = () => {
    this.service = INSTALLED;
  };
  private readonly handlers = new Map<string, Set<() => void>>();

  constructor(public answer: (year: number, month: number) => unknown) {
    super();
  }

  override async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    if (command === "service_status" || command === "service_install") {
      this.serviceCalls.push(command);
      if (command === "service_install") this.install();
      if (command === "service_status" && this.hang) return new Promise<T>(() => undefined);
      if (command === "service_status") return (this.serviceQueue.shift() ?? this.service) as T;
      return "installed" as T;
    }
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
  options: {
    firstDay?: FirstDay;
    now?: Date;
    month?: string;
    service?: ServiceStatus;
    prepare?: Partial<Pick<StubIpc, "serviceQueue" | "hang">>;
  } = {},
) {
  const ipc = new StubIpc(answer);
  if (options.service) ipc.service = options.service;
  Object.assign(ipc, options.prepare);
  TestBed.configureTestingModule({
    providers: [
      { provide: Ipc, useValue: ipc },
      { provide: STATUS_RETRY_DELAY, useValue: 0 },
      { provide: STATUS_TIMEOUT, useValue: 50 },
      { provide: FIRST_DAY, useValue: options.firstDay ?? 1 },
      { provide: NOW, useValue: () => options.now ?? NOV_3 },
      provideRouter([]),
      ...(options.month === undefined
        ? []
        : [
            {
              provide: ActivatedRoute,
              useValue: {
                snapshot: { queryParamMap: convertToParamMap({ month: options.month }) },
              },
            },
          ]),
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
    expect(chip(el, "2026-11-03", "claude").textContent).toBe("C 150 %");
    expect(chip(el, "2026-11-03", "copilot").textContent).toBe("CP");
  });

  it("shows the usage ratio in the chip and just the letters without a ratio", async () => {
    const { el } = await mount((y, m) =>
      month(y, m, {
        ...CLAUDE,
        days: [
          { date: "2026-11-01", usage: 2, budget: 10, ratio: 0.2, colour: "green" },
          { date: "2026-11-02", usage: 9, budget: 10, ratio: 0.644, colour: "yellow" },
          { date: "2026-11-03", usage: 1, budget: 10, colour: "green" },
        ],
      }),
    );
    expect(chip(el, "2026-11-01", "claude").textContent).toBe("C 20 %");
    expect(chip(el, "2026-11-02", "claude").textContent).toBe("C 64 %");
    expect(chip(el, "2026-11-03", "claude").textContent).toBe("C");
  });

  it("marks today, future days and weekends by class and aria-current", async () => {
    const { el } = await mount((y, m) => month(y, m, CLAUDE));
    expect(cell(el, "2026-11-03").classList.contains("today")).toBe(true);
    expect(cell(el, "2026-11-03").getAttribute("aria-current")).toBe("date");
    expect(cell(el, "2026-11-04").classList.contains("future")).toBe(true);
    expect(cell(el, "2026-11-01").classList.contains("weekend")).toBe(true); // a Sunday
    expect(cell(el, "2026-11-02").classList.contains("weekend")).toBe(false);
  });

  it("carries both weekend and future classes on a future weekend day", async () => {
    const { el } = await mount((y, m) => month(y, m, CLAUDE));
    for (const date of ["2026-11-07", "2026-11-08"]) {
      expect(cell(el, date).classList.contains("weekend")).toBe(true);
      expect(cell(el, date).classList.contains("future")).toBe(true);
    }
    expect(cell(el, "2026-11-09").classList.contains("weekend")).toBe(false);
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

  it("names the icon-only month buttons for assistive technology", async () => {
    const { el } = await mount((y, m) => month(y, m, CLAUDE));
    const prev = el.querySelector("[data-testid=prev]")!;
    const next = el.querySelector("[data-testid=next]")!;
    expect(prev.getAttribute("aria-label")).toBe("Previous month");
    expect(prev.getAttribute("title")).toBe("Previous month");
    expect(next.getAttribute("aria-label")).toBe("Next month");
    expect(next.getAttribute("title")).toBe("Next month");
    expect(el.querySelector<HTMLButtonElement>("[data-testid=today]")!.disabled).toBe(true);
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

  it("links the gear in the header to the settings", async () => {
    const { el } = await mount((y, m) => month(y, m, CLAUDE));
    const gear = el.querySelector<HTMLAnchorElement>("[data-testid=gear]")!;
    expect(gear.getAttribute("href")).toBe("/settings");
  });

  it("links the number of every day up to today to its day view, and no later day", async () => {
    const { el } = await mount((y, m) => month(y, m, CLAUDE));
    const link = cell(el, "2026-11-03").querySelector<HTMLAnchorElement>("[data-testid=open-day]");
    expect(link?.getAttribute("href")).toBe("/day/2026-11-03?month=2026-11");
    expect(cell(el, "2026-11-01").querySelector("[data-testid=open-day]")).not.toBeNull();
    expect(cell(el, "2026-11-04").querySelector("[data-testid=open-day]")).toBeNull();
  });

  it("opens the day view when a chip of the card is clicked", async () => {
    const { el } = await mount((y, m) => month(y, m, CLAUDE));
    const router = TestBed.inject(Router);
    const navigate = vi.spyOn(router, "navigate").mockResolvedValue(true);
    chip(el, "2026-11-03", "claude").click();
    expect(navigate).toHaveBeenCalledWith(["/day", "2026-11-03"], {
      queryParams: { month: "2026-11" },
    });
    navigate.mockClear();
    chip(el, "2026-11-04", "claude")?.click();
    expect(navigate).not.toHaveBeenCalled();
  });

  it("opens on the month of the query, e.g. when coming back from a day", async () => {
    const { el, ipc } = await mount((y, m) => month(y, m, CLAUDE), { month: "2026-10" });
    expect(ipc.requests).toEqual([[2026, 10]]);
    expect(cells(el)).toHaveLength(31);
  });

  it("ignores an invalid month in the query", async () => {
    const { ipc } = await mount((y, m) => month(y, m, CLAUDE), { month: "2026-13" });
    expect(ipc.requests).toEqual([[2026, 11]]);
  });

  it("keeps the loading state on a busy store", async () => {
    const { el } = await mount(() => {
      throw { message: "busy", busy: true };
    });
    expect(el.querySelector("[data-testid=error]")).toBeNull();
    expect(el.querySelector("[role=status]")?.textContent).toContain("Reading the month");
  });
});

describe("Calendar: first-start prompt", () => {
  const CLAUDE_UNAVAILABLE: ProviderMonth = { ...CLAUDE, status: "unavailable", days: [] };
  /** No database: every provider is unavailable. */
  const empty = (y: number, m: number) => month(y, m, CLAUDE_UNAVAILABLE, COPILOT_UNAVAILABLE);
  const prompt = (el: HTMLElement) => el.querySelector("[data-testid=setup-prompt]");

  it("asks to set up collection without data and without a collector", async () => {
    const { el } = await mount(empty, { service: NOT_INSTALLED });
    expect(prompt(el)?.textContent).toContain("Collection is not set up — set up now?");
    expect(el.querySelector<HTMLButtonElement>("[data-testid=setup]")!.disabled).toBe(false);
  });

  it("does not ask when the collector is installed, e.g. a CLI from the archives", async () => {
    const { el, ipc } = await mount(empty, { service: INSTALLED });
    expect(ipc.serviceCalls).toEqual(["service_status"]);
    expect(prompt(el)).toBeNull();
  });

  it("does not ask, nor run the CLI, when there is data", async () => {
    const { el, ipc } = await mount((y, m) => month(y, m, CLAUDE, COPILOT_UNAVAILABLE), {
      service: NOT_INSTALLED,
    });
    expect(ipc.serviceCalls).toEqual([]);
    expect(prompt(el)).toBeNull();
  });

  it("asks when a provider is stale without ever having succeeded", async () => {
    const neverOk: ProviderMonth = { ...CLAUDE, status: "stale", days: [] };
    const { el } = await mount((y, m) => month(y, m, neverOk), { service: NOT_INSTALLED });
    expect(prompt(el)).not.toBeNull();
  });

  it("does not ask when a stale provider had data before", async () => {
    const stale: ProviderMonth = {
      ...CLAUDE,
      status: "stale",
      stale_since: "2026-11-03T09:00:00Z",
    };
    const { el } = await mount((y, m) => month(y, m, stale), { service: NOT_INSTALLED });
    expect(prompt(el)).toBeNull();
  });

  it("shows the error, not an empty calendar, when the status cannot be read", async () => {
    const { el } = await mount(empty, {
      service: { state: "error", message: "did not answer within 10 s", exe: null },
    });
    expect(prompt(el)).toBeNull();
    expect(el.querySelector("[data-testid=status-error]")?.textContent).toContain(
      "Cannot read the collection status: did not answer within 10 s",
    );
  });

  it("tries a failed first status once more before it shows the error", async () => {
    const { el, ipc, settle } = await mount(empty, {
      service: { state: "error", message: "did not answer within 10 s", exe: null },
    });
    await new Promise((resolve) => setTimeout(resolve, 20));
    await settle();
    expect(ipc.serviceCalls).toEqual(["service_status", "service_status"]);
    expect(el.querySelector("[data-testid=status-error]")).not.toBeNull();
  });

  it("asks to set up when the automatic retry of the status succeeds", async () => {
    const ipc = { serviceQueue: [{ state: "error", message: "cold", exe: null } as ServiceStatus] };
    const {
      el,
      ipc: stub,
      settle,
    } = await mount(empty, {
      service: NOT_INSTALLED,
      prepare: ipc,
    });
    await new Promise((resolve) => setTimeout(resolve, 20));
    await settle();
    expect(stub.serviceCalls).toEqual(["service_status", "service_status"]);
    expect(el.querySelector("[data-testid=status-error]")).toBeNull();
    expect(prompt(el)).not.toBeNull();
  });

  it("shows the error, not an empty calendar, when the status never answers", async () => {
    const { el, ipc, settle } = await mount(empty, { prepare: { hang: true } });
    expect(el.querySelector("[data-testid=status-pending]")).not.toBeNull();
    await new Promise((resolve) => setTimeout(resolve, 200));
    await settle();
    expect(ipc.serviceCalls).toEqual(["service_status", "service_status"]);
    expect(el.querySelector("[data-testid=status-pending]")).toBeNull();
    expect(el.querySelector("[data-testid=status-error]")?.textContent).toContain(
      "no answer within 0.05 s",
    );
  });

  it("asks to set up for a report whose providers are ok but have no days", async () => {
    const hollow: ProviderMonth = { ...CLAUDE, days: [] };
    const { el } = await mount((y, m) => month(y, m, hollow), { service: NOT_INSTALLED });
    expect(prompt(el)).not.toBeNull();
  });

  it("asks to set up for a report without providers", async () => {
    const { el } = await mount((y, m) => month(y, m), { service: NOT_INSTALLED });
    expect(prompt(el)).not.toBeNull();
  });

  it("neither asks nor shows an error for an unusable report with a collector", async () => {
    const hollow: ProviderMonth = { ...CLAUDE, days: [] };
    const { el } = await mount((y, m) => month(y, m, hollow), { service: INSTALLED });
    expect(prompt(el)).toBeNull();
    expect(el.querySelector("[data-testid=status-error]")).toBeNull();
    expect(el.querySelector("[data-testid=status-pending]")).toBeNull();
  });

  it("reads the status again on Retry and then asks to set up", async () => {
    const { el, ipc, click } = await mount(empty, {
      service: { state: "error", message: "did not answer within 10 s", exe: null },
    });
    ipc.service = NOT_INSTALLED;
    await click("status-retry");
    expect(ipc.serviceCalls).toEqual(["service_status", "service_status", "service_status"]);
    expect(el.querySelector("[data-testid=status-error]")).toBeNull();
    expect(prompt(el)).not.toBeNull();
  });

  it("does not show a status error when there is data", async () => {
    const { el } = await mount((y, m) => month(y, m, CLAUDE, COPILOT_UNAVAILABLE), {
      service: { state: "error", message: "boom", exe: null },
    });
    expect(el.querySelector("[data-testid=status-error]")).toBeNull();
  });

  it("sets up collection and then shows that it is set up", async () => {
    const { el, ipc, click, settle } = await mount(empty, { service: NOT_INSTALLED });
    await click("setup");
    await settle();
    expect(ipc.serviceCalls).toEqual(["service_status", "service_install", "service_status"]);
    expect(prompt(el)).toBeNull();
    expect(el.querySelector("[data-testid=setup-done]")?.textContent).toContain(
      "Collection is set up",
    );
  });

  it("keeps the prompt with the error when the setup fails, and reads the status again", async () => {
    const { el, ipc, click, settle } = await mount(empty, { service: NOT_INSTALLED });
    ipc.install = () => {
      throw new Error("no systemd user manager");
    };
    await click("setup");
    await settle();
    expect(ipc.serviceCalls).toEqual(["service_status", "service_install", "service_status"]);
    expect(el.querySelector("[data-testid=setup-error]")?.textContent).toContain(
      "Cannot set up collection: no systemd user manager",
    );
    expect(el.querySelector("[data-testid=setup-done]")).toBeNull();
  });

  it("hides the prompt on Not now", async () => {
    const { el, click } = await mount(empty, { service: NOT_INSTALLED });
    await click("setup-dismiss");
    expect(prompt(el)).toBeNull();
  });
});

describe("Calendar month projection", () => {
  const lines = (el: HTMLElement) =>
    [...el.querySelectorAll("[data-testid=month-line]")].map((e) => ({
      text: e.textContent?.trim(),
      colour: e.getAttribute("data-colour"),
    }));

  it("projects the current month: 26 in 3 days against a daily 10 is 260 of 300", async () => {
    const { el } = await mount((y, m) => month(y, m, CLAUDE, COPILOT_UNAVAILABLE));
    expect(lines(el)).toEqual([{ text: "Claude: projected 87 %", colour: "yellow" }]);
  });

  it("colours the chip by the projection ratio, not by the latest day", async () => {
    const heavy: ProviderMonth = {
      ...CLAUDE,
      days: [
        { date: "2026-11-01", usage: 20, budget: 10, ratio: 2, colour: "red" },
        { date: "2026-11-03", usage: 1, budget: 10, ratio: 0.1, colour: "green" },
      ],
    };
    // 21 in 3 days against a daily 10: 210 of 300 = 70 %, green although day 1 was red.
    const { el } = await mount((y, m) => month(y, m, heavy));
    expect(lines(el)).toEqual([{ text: "Claude: projected 70 %", colour: "green" }]);
    TestBed.resetTestingModule();
    const over: ProviderMonth = { ...heavy, days: [{ ...heavy.days[0], usage: 45 }] };
    // 45 in 3 days: 450 of 300 = 150 %.
    const high = await mount((y, m) => month(y, m, over));
    expect(lines(high.el)).toEqual([{ text: "Claude: projected 150 %", colour: "red" }]);
  });

  it("shows the final usage of a past month, without projection", async () => {
    const october: ProviderMonth = {
      ...CLAUDE,
      days: [{ date: "2026-10-31", usage: 62, budget: 10, ratio: 6.2, colour: "red" }],
    };
    const { el } = await mount((y, m) => month(y, m, october), { month: "2026-10" });
    expect(lines(el)).toEqual([{ text: "Claude: month total 20 %", colour: "green" }]);
  });

  it("shows nothing for a future month, without data or with a budget of 0", async () => {
    const future = await mount((y, m) => month(y, m, CLAUDE), { month: "2026-12" });
    expect(lines(future.el)).toEqual([]);
    TestBed.resetTestingModule();
    const none = await mount((y, m) => month(y, m, { ...CLAUDE, days: [] }));
    expect(lines(none.el)).toEqual([]);
    TestBed.resetTestingModule();
    const zero: ProviderMonth = {
      ...CLAUDE,
      days: [{ date: "2026-11-01", usage: 2, budget: 0, colour: "green" }],
    };
    const noBudget = await mount((y, m) => month(y, m, zero));
    expect(lines(noBudget.el)).toEqual([]);
  });
});

describe("Calendar month chart", () => {
  const charts = (el: HTMLElement) => [...el.querySelectorAll("[data-testid=month-chart]")];

  it("shows a chart per provider with data, with usage, budget and projection lines", async () => {
    const { el } = await mount((y, m) => month(y, m, CLAUDE, COPILOT_UNAVAILABLE));
    const found = charts(el);
    expect(found.map((c) => c.getAttribute("data-provider"))).toEqual(["claude"]);
    expect(found[0]!.querySelector("[data-testid=budget-line]")).not.toBeNull();
    expect(found[0]!.querySelector("[data-testid=usage-line]")!.getAttribute("class")).toContain(
      "colour-yellow",
    );
    expect(found[0]!.querySelector("[data-testid=projection-line]")).not.toBeNull();
  });

  it("shows no projection line for a past month and no chart for a future one", async () => {
    const october: ProviderMonth = {
      ...CLAUDE,
      days: [{ date: "2026-10-31", usage: 62, budget: 10, ratio: 6.2, colour: "red" }],
    };
    const past = await mount((y, m) => month(y, m, october), { month: "2026-10" });
    expect(charts(past.el)).toHaveLength(1);
    expect(past.el.querySelector("[data-testid=projection-line]")).toBeNull();
    TestBed.resetTestingModule();
    const future = await mount((y, m) => month(y, m, CLAUDE), { month: "2026-12" });
    expect(charts(future.el)).toHaveLength(1);
  });
});
