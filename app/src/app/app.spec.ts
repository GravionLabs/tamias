// @vitest-environment jsdom
import { TestBed, getTestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { Router } from "@angular/router";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { App, SHOW_CALENDAR } from "./app";
import { Ipc, type Unlisten } from "./core/ipc";

beforeAll(() => {
  getTestBed().initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => TestBed.resetTestingModule());

class EventIpc extends Ipc {
  handlers = new Map<string, () => void>();
  readonly unlisten = vi.fn<Unlisten>();

  override invoke<T>(): Promise<T> {
    throw new Error("no commands");
  }

  override async listen(event: string, handler: () => void): Promise<Unlisten> {
    this.handlers.set(event, handler);
    return this.unlisten;
  }
}

const flush = () => new Promise((resolve) => setTimeout(resolve));

async function shell(url: string, hash = "#/") {
  window.location.hash = hash;
  const ipc = new EventIpc();
  const navigate = vi.fn().mockResolvedValue(true);
  TestBed.configureTestingModule({
    providers: [
      { provide: Ipc, useValue: ipc },
      { provide: Router, useValue: { url, navigateByUrl: navigate } },
    ],
  });
  const fixture = TestBed.createComponent(App);
  await flush();
  return { ipc, navigate, fixture };
}

describe("App", () => {
  it("routes to the calendar on the show-calendar event", async () => {
    const { ipc, navigate } = await shell("/day/2026-01-02");
    ipc.handlers.get(SHOW_CALENDAR)?.();
    expect(navigate).toHaveBeenCalledWith("/");
  });

  it("keeps the tray card on its own route", async () => {
    const { ipc, navigate } = await shell("/tray", "#/tray");
    ipc.handlers.get(SHOW_CALENDAR)?.();
    expect(navigate).not.toHaveBeenCalled();
  });

  it("shows the status bar in the main window", async () => {
    const { fixture } = await shell("/settings", "#/settings");
    expect(fixture.nativeElement.querySelector('[data-testid="status-bar"]')).not.toBeNull();
  });

  it("shows no status bar in the tray popup", async () => {
    const { fixture } = await shell("/tray", "#/tray");
    expect(fixture.nativeElement.querySelector('[data-testid="status-bar"]')).toBeNull();
  });

  it("stops listening when destroyed", async () => {
    const { ipc, fixture } = await shell("/");
    fixture.destroy();
    expect(ipc.unlisten).toHaveBeenCalledOnce();
  });
});
