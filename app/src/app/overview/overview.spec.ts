// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { TestBed, getTestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { Ipc } from "../core/ipc";
import type { ProviderReport, Report } from "../core/report";
import { Overview } from "./overview";

beforeAll(() => {
  getTestBed().initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => TestBed.resetTestingModule());

/** An `Ipc` whose `today` answers `answer` (a thrown error rejects it). */
class StubIpc extends Ipc {
  constructor(private readonly answer: () => unknown) {
    super();
  }

  override async invoke<T>(command: string): Promise<T> {
    if (command !== "today") throw new Error(`unexpected command ${command}`);
    return this.answer() as T;
  }
}

function report(...providers: ProviderReport[]): Report {
  return { schema_version: 1, generated_at: "2026-11-03T10:05:00Z", providers };
}

const CLAUDE_OK: ProviderReport = {
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
      colour: "green",
      today_usage: 4,
      budget: 10,
      ratio: 0.4,
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

const COPILOT_STALE: ProviderReport = {
  provider: "copilot",
  status: "stale",
  stale_since: "2026-11-03T09:00:00Z",
  windows: [
    {
      window: "monthly",
      binding: true,
      unit: "requests",
      used: 26,
      limit: 300,
      resets_at: "2026-12-01T00:00:00Z",
      colour: "grey",
      today_usage: 6,
      budget: 10,
      ratio: 0.6,
    },
  ],
};

async function render(answer: () => unknown): Promise<HTMLElement> {
  TestBed.configureTestingModule({ providers: [{ provide: Ipc, useValue: new StubIpc(answer) }] });
  TestBed.overrideComponent(Overview, {
    set: {
      templateUrl: undefined,
      styleUrl: undefined,
      template: readFileSync("src/app/overview/overview.html", "utf8"),
    },
  });
  const fixture = TestBed.createComponent(Overview);
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture.nativeElement as HTMLElement;
}

function card(el: HTMLElement, provider: string): HTMLElement {
  const found = el.querySelector<HTMLElement>(`hx-card[data-provider=${provider}]`);
  if (!found) throw new Error(`no card for ${provider}`);
  return found;
}

describe("Overview", () => {
  it("shows an ok provider with its windows, used amount and today's budget", async () => {
    const el = await render(() => report(CLAUDE_OK));
    const claude = card(el, "claude");
    expect(claude.querySelector("h2")?.textContent).toBe("Claude");
    expect(claude.querySelector("hx-tag")?.textContent).toContain("ok");
    const sevenDay = claude.querySelector("[data-window=seven_day]")!;
    expect(sevenDay.textContent).toContain("7-day window");
    expect(sevenDay.textContent).toContain("54% used");
    expect(sevenDay.textContent).toContain("today 4% of 10%");
    expect(sevenDay.querySelector(".dot")?.classList).toContain("colour-green");
    const fiveHour = claude.querySelector("[data-window=five_hour]")!;
    expect(fiveHour.textContent).toContain("12% used");
    expect(fiveHour.textContent).not.toContain("today");
    expect(claude.querySelector("[data-testid=stale]")).toBeNull();
  });

  it("shows a stale provider with the time it is stale since", async () => {
    const el = await render(() => report(COPILOT_STALE));
    const copilot = card(el, "copilot");
    expect(copilot.querySelector("hx-tag")?.textContent).toContain("stale");
    // The time is local (format covered by report.spec.ts): `HH:MM`, or with the date on another day.
    expect(copilot.querySelector("[data-testid=stale]")?.textContent).toMatch(
      /^stale since (\d{4}-\d{2}-\d{2} )?\d{2}:\d{2}$/,
    );
    const monthly = copilot.querySelector("[data-window=monthly]")!;
    expect(monthly.textContent).toContain("26 of 300 used");
    expect(monthly.textContent).toContain("today 6 of 10");
    expect(monthly.querySelector(".dot")?.classList).toContain("colour-grey");
  });

  it("shows a stale provider that never collected", async () => {
    const el = await render(() => report({ provider: "claude", status: "stale", windows: [] }));
    expect(card(el, "claude").querySelector("[data-testid=stale]")?.textContent).toContain(
      "stale, never collected",
    );
  });

  it("shows an unavailable provider without windows", async () => {
    const el = await render(() =>
      report(CLAUDE_OK, { provider: "copilot", status: "unavailable", windows: [] }),
    );
    const copilot = card(el, "copilot");
    expect(copilot.querySelector("hx-tag")?.textContent).toContain("unavailable");
    expect(copilot.querySelector("[data-testid=unavailable]")).not.toBeNull();
    expect(copilot.querySelector(".windows")).toBeNull();
    expect(card(el, "claude").getAttribute("data-status")).toBe("ok");
  });

  it("shows the error when the backend fails", async () => {
    const el = await render(() => {
      throw new Error("database is locked");
    });
    expect(el.querySelector("[data-testid=error]")?.textContent).toContain("database is locked");
    expect(el.querySelector("hx-card")).toBeNull();
  });

  it("shows a string rejection of the backend as the error", async () => {
    const el = await render(() => Promise.reject("invalid config"));
    expect(el.querySelector("[data-testid=error]")?.textContent).toContain("invalid config");
  });

  it("shows the loading state until the backend answers", async () => {
    const el = await render(() => new Promise(() => {}));
    expect(el.querySelector("[role=status]")?.textContent).toContain("Reading today's usage");
  });
});
