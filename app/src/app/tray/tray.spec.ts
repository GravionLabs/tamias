// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { TestBed, getTestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { Ipc, type Unlisten } from "../core/ipc";
import { STORE_CHANGED } from "../core/store-events";
import type { ProviderReport, Report, WindowReport } from "../core/report";
import { barWidth, todayBarWidth, projectionBarWidth, WindowList } from "../core/window-list";
import { Tray, providerNote, shownProviders } from "./tray";

beforeAll(() => {
  getTestBed().initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => {
  TestBed.resetTestingModule();
  vi.useRealTimers();
});

/** An `Ipc` whose `today` answers `answer`; records the popup commands; `emit` fires events. */
class StubIpc extends Ipc {
  todayCalls = 0;
  commands: string[] = [];
  private readonly handlers = new Map<string, Set<() => void>>();

  constructor(public answer: () => unknown) {
    super();
  }

  args: unknown[] = [];

  override async invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    if (command === "today") {
      this.todayCalls++;
      return this.answer() as T;
    }
    this.commands.push(command);
    if (args) this.args.push(args);
    return undefined as T;
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

const report = (...providers: ProviderReport[]): Report => ({
  schema_version: 1,
  generated_at: "2026-11-03T10:05:00Z",
  providers,
});

const CLAUDE: ProviderReport = {
  provider: "claude",
  status: "ok",
  windows: [
    {
      window: "seven_day",
      binding: true,
      unit: "percent",
      used: 54,
      limit: 100,
      resets_at: "2026-11-07T12:00:00Z",
      colour: "yellow",
    },
    {
      window: "five_hour",
      binding: false,
      unit: "percent",
      used: 12,
      limit: 100,
      resets_at: "2026-11-03T13:30:00Z",
      colour: "green",
    },
  ],
};

const COPILOT: ProviderReport = {
  provider: "copilot",
  status: "ok",
  windows: [
    {
      window: "monthly",
      binding: true,
      unit: "requests",
      used: 489,
      limit: 2380,
      resets_at: "2026-12-01T00:00:00Z",
      colour: "red",
    },
  ],
};

async function mount(answer: () => unknown) {
  const ipc = new StubIpc(answer);
  TestBed.configureTestingModule({ providers: [{ provide: Ipc, useValue: ipc }] });
  TestBed.overrideComponent(Tray, {
    set: {
      templateUrl: undefined,
      styleUrl: undefined,
      template: readFileSync("src/app/tray/tray.html", "utf8"),
    },
  });
  TestBed.overrideComponent(WindowList, {
    set: {
      templateUrl: undefined,
      styleUrl: undefined,
      template: readFileSync("src/app/core/window-list.html", "utf8"),
    },
  });
  const fixture = TestBed.createComponent(Tray);
  const settle = async () => {
    await fixture.whenStable();
    fixture.detectChanges();
  };
  await settle();
  return { ipc, el: fixture.nativeElement as HTMLElement, settle };
}

const text = (el: Element | null) => el?.textContent?.trim();

describe("Tray", () => {
  it("shows a block per window with label, percentage, bar and numbers", async () => {
    const { el } = await mount(() => report(CLAUDE, COPILOT));
    expect(text(el.querySelector("[data-provider=claude] h2"))).toBe("Claude Usage");
    const sevenDay = el.querySelector("[data-window=seven_day]")!;
    expect(text(sevenDay.querySelector(".label"))).toBe("7-day window");
    expect(text(sevenDay.querySelector("[data-testid=percent]"))).toBe("54%");
    expect(sevenDay.querySelector(".bar")?.classList).toContain("colour-yellow");
    expect(sevenDay.querySelector<HTMLElement>(".bar")?.style.width).toBe("54%");
    expect(text(sevenDay.querySelector("[data-testid=used]"))).toBe("54% used");
    expect(text(sevenDay.querySelector("[data-testid=resets]"))).toMatch(/^resets /);
    const monthly = el.querySelector("[data-window=monthly]")!;
    expect(text(monthly.querySelector("[data-testid=percent]"))).toBe("21%");
    expect(monthly.querySelector(".bar")?.classList).toContain("colour-red");
    expect(text(monthly.querySelector("[data-testid=used]"))).toBe("489 of 2380 used");
    expect(el.querySelectorAll(".window").length).toBe(3);
  });

  it("shows today against the budget with its own bar, only for windows with a budget", async () => {
    const claude: ProviderReport = {
      ...CLAUDE,
      windows: [
        { ...CLAUDE.windows[0], today_usage: 4, budget: 10, ratio: 0.4 },
        CLAUDE.windows[1],
      ],
    };
    const copilot: ProviderReport = {
      ...COPILOT,
      windows: [{ ...COPILOT.windows[0], today_usage: 90, budget: 50, ratio: 1.8 }],
    };
    const { el } = await mount(() => report(claude, copilot));
    const sevenDay = el.querySelector("[data-window=seven_day]")!;
    expect(text(sevenDay.querySelector("[data-testid=today]"))).toBe("40%");
    expect(sevenDay.querySelector<HTMLElement>("[data-testid=today-bar]")?.style.width).toBe("40%");
    expect(sevenDay.querySelector("[data-testid=today-bar]")?.classList).toContain("colour-yellow");
    expect(text(sevenDay.querySelectorAll(".row .label")[1])).toBe("Today");
    const monthly = el.querySelector("[data-window=monthly]")!;
    expect(text(monthly.querySelector("[data-testid=today]"))).toBe("180%");
    expect(monthly.querySelector<HTMLElement>("[data-testid=today-bar]")?.style.width).toBe("100%");
    expect(el.querySelector("[data-window=five_hour] [data-testid=today]")).toBeNull();
  });

  it("shows the stale reason and the grey bar of a stale provider", async () => {
    const stale: ProviderReport = {
      ...COPILOT,
      status: "stale",
      stale_since: "2026-11-03T09:00:00Z",
      windows: [{ ...COPILOT.windows[0], colour: "grey" }],
    };
    const { el } = await mount(() => report(stale));
    expect(text(el.querySelector("[data-testid=note]"))).toMatch(/^stale since /);
    expect(el.querySelector(".bar")?.classList).toContain("colour-grey");
  });

  it("shows a hint for a provider without data", async () => {
    const { el } = await mount(() => report({ provider: "claude", status: "ok", windows: [] }));
    expect(text(el.querySelector("[data-testid=note]"))).toBe("No data yet.");
  });

  it("hides unavailable providers while another is set up", async () => {
    const off: ProviderReport = { provider: "copilot", status: "unavailable", windows: [] };
    const { el } = await mount(() => report(CLAUDE, off));
    expect(el.querySelector("[data-provider=copilot]")).toBeNull();
  });

  it("says so when no provider is set up", async () => {
    const off: ProviderReport = { provider: "claude", status: "unavailable", windows: [] };
    const { el } = await mount(() => report(off));
    expect(text(el.querySelector("[data-testid=note]"))).toBe(
      "Not set up on this machine, or disabled.",
    );
  });

  it("shows the error when the backend fails", async () => {
    const { el } = await mount(() => {
      throw new Error("database is locked");
    });
    expect(text(el.querySelector("[data-testid=error]"))).toContain("database is locked");
  });

  it("shows the loading state until the backend answers", async () => {
    const { el } = await mount(() => new Promise(() => {}));
    expect(text(el.querySelector("[role=status]"))).toBe("Reading usage…");
  });

  it("reloads on a store-changed event", async () => {
    const { ipc, el, settle } = await mount(() => report(CLAUDE));
    expect(ipc.todayCalls).toBe(1);
    ipc.answer = () => report({ ...CLAUDE, windows: [{ ...CLAUDE.windows[0], used: 90 }] });
    ipc.emit(STORE_CHANGED);
    await settle();
    expect(ipc.todayCalls).toBe(2);
    expect(text(el.querySelector("[data-testid=percent]"))).toBe("90%");
  });

  it("opens the main window and hides on Escape", async () => {
    const { ipc, el } = await mount(() => report(CLAUDE));
    el.querySelector<HTMLButtonElement>("button.open")!.click();
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(ipc.commands).toEqual(["tray_popup_open_main", "tray_popup_hide"]);
  });
});

describe("Tray hover", () => {
  it("reports the pointer entering and leaving the card", async () => {
    const { ipc, el } = await mount(() => report(CLAUDE));
    el.dispatchEvent(new MouseEvent("mouseenter"));
    el.dispatchEvent(new MouseEvent("mouseleave"));
    expect(ipc.commands).toEqual(["tray_popup_hover", "tray_popup_hover"]);
    expect(ipc.args).toEqual([{ over: true }, { over: false }]);
  });
});

describe("tray helpers", () => {
  const w = (over: Partial<WindowReport>): WindowReport => ({ ...CLAUDE.windows[0], ...over });

  it("caps the bar at the full track but not below zero", () => {
    expect(barWidth(w({ used: 130 }))).toBe(100);
    expect(barWidth(w({ used: 0 }))).toBe(0);
    expect(barWidth(w({ limit: 0 }))).toBe(0);
  });

  it("caps the today bar at the full track and is empty without a budget", () => {
    expect(todayBarWidth(w({ budget: 10, ratio: 1.8 }))).toBe(100);
    expect(todayBarWidth(w({ budget: 10, ratio: 0.21 }))).toBe(21);
    expect(todayBarWidth(w({ budget: 0, ratio: undefined }))).toBe(0);
    expect(todayBarWidth(w({}))).toBe(0);
  });

  it("explains stale providers and has no note for healthy ones", () => {
    expect(providerNote({ ...CLAUDE, status: "stale" })).toBe("stale, never collected");
    expect(providerNote(CLAUDE)).toBeNull();
  });

  it("falls back to all providers when none is set up", () => {
    const off: ProviderReport = { provider: "claude", status: "unavailable", windows: [] };
    expect(shownProviders(report(off))).toHaveLength(1);
  });
});

describe("Tray month projection", () => {
  const claude = (extra: Partial<WindowReport>): ProviderReport => ({
    ...CLAUDE,
    windows: [{ ...CLAUDE.windows[0], ...extra }],
  });

  it("shows the projected month in the budget colour, like the Today row", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2026, 10, 3, 10, 0));
    // 3 days used 15 of a daily 10: 15 / 3 * 30 = 150 of 300 = 50 %.
    const { el } = await mount(() =>
      report(claude({ today_usage: 4, budget: 10, ratio: 0.4, month_usage: 15 })),
    );
    const row = el.querySelector("[data-window=seven_day]")!;
    expect(row.textContent).toContain("Month (projected)");
    expect(text(row.querySelector("[data-testid=projection]"))).toBe("50%");
    const bar = row.querySelector<HTMLElement>("[data-testid=projection-bar]")!;
    expect(bar.classList).toContain("colour-green");
    expect(bar.style.width).toBe("50%");
    expect(row.querySelectorAll(".today-row").length).toBe(2);
    expect(row.querySelectorAll(".today-track").length).toBe(2);
  });

  it("colours the projection by its own ratio, not by the window colour", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2026, 10, 3, 10, 0));
    const bar = async (month_usage: number, colour: WindowReport["colour"]) => {
      const { el } = await mount(() =>
        report(claude({ today_usage: 1, budget: 10, ratio: 0.1, colour, month_usage })),
      );
      const found = el.querySelector("[data-testid=projection-bar]")!;
      const result = [...found.classList].find((c) => c.startsWith("colour-"));
      TestBed.resetTestingModule();
      return result;
    };
    expect(await bar(45, "green")).toBe("colour-red"); // 150 %
    expect(await bar(27, "green")).toBe("colour-yellow"); // 90 %
    expect(await bar(24, "red")).toBe("colour-yellow"); // 80 %, boundary
    expect(await bar(23, "red")).toBe("colour-green"); // 76.7 %
    expect(await bar(45, "grey")).toBe("colour-grey"); // stale
  });

  it("has no projection row without a budget or without month usage", async () => {
    const { el } = await mount(() => report(claude({ month_usage: 15 }), COPILOT));
    expect(el.querySelector("[data-testid=projection]")).toBeNull();
  });

  it("caps the bar at the full track", () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    const w = { ...CLAUDE.windows[0], budget: 1, month_usage: 30 };
    expect(projectionBarWidth(w, new Date(2026, 10, 1))).toBe(100);
  });
});

describe("Tray popup height", () => {
  afterEach(() => vi.restoreAllMocks());

  it("reports the height of the loaded card without a resize notification", async () => {
    // A hidden popup webview draws no frames, so its ResizeObserver stays silent (jsdom has none):
    // the height has to be reported after the report has rendered, not only on a resize.
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (
      this: HTMLElement,
    ) {
      const height = 40 + 100 * this.querySelectorAll(".window").length;
      return { height } as DOMRect;
    });
    const { ipc } = await mount(() => report(CLAUDE, COPILOT));
    const heights = ipc.args.map((a) => (a as { height: number }).height);
    expect(heights.at(-1)).toBe(340);
  });
});
