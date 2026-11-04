// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { TestBed, getTestBed } from "@angular/core/testing";
import { provideRouter } from "@angular/router";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { Ipc, type Unlisten } from "../core/ipc";
import { Settings } from "./settings";

beforeAll(() => {
  getTestBed().initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => TestBed.resetTestingModule());

/** An `Ipc` that plays the OS: `enabled` is the autostart entry, `fail` makes the next calls reject. */
class StubIpc extends Ipc {
  calls: string[] = [];
  fail: unknown = null;

  constructor(public enabled: boolean) {
    super();
  }

  override async invoke<T>(command: string): Promise<T> {
    this.calls.push(command);
    if (this.fail) throw this.fail;
    if (command === "plugin:autostart|is_enabled") return this.enabled as T;
    if (command === "plugin:autostart|enable") this.enabled = true;
    else if (command === "plugin:autostart|disable") this.enabled = false;
    else throw new Error(`unexpected command ${command}`);
    return undefined as T;
  }

  override async listen(): Promise<Unlisten> {
    return () => undefined;
  }
}

async function mount(ipc: StubIpc) {
  TestBed.configureTestingModule({
    providers: [{ provide: Ipc, useValue: ipc }, provideRouter([])],
  });
  TestBed.overrideComponent(Settings, {
    set: {
      templateUrl: undefined,
      styleUrl: undefined,
      template: readFileSync("src/app/settings/settings.html", "utf8"),
    },
  });
  const fixture = TestBed.createComponent(Settings);
  const settle = async () => {
    await fixture.whenStable();
    fixture.detectChanges();
  };
  await settle();
  const el = fixture.nativeElement as HTMLElement;
  const toggle = () => el.querySelector<HTMLInputElement>("[data-testid=autostart]")!;
  const error = () => el.querySelector("[data-testid=error]")?.textContent?.trim() ?? null;
  const click = async () => {
    toggle().click();
    await settle();
  };
  return { el, toggle, error, click };
}

describe("Settings", () => {
  it("shows the toggle off when the OS has no autostart entry", async () => {
    const { toggle, error } = await mount(new StubIpc(false));
    expect(toggle().checked).toBe(false);
    expect(toggle().disabled).toBe(false);
    expect(error()).toBeNull();
  });

  it("shows the toggle on when the OS has the autostart entry", async () => {
    const { toggle } = await mount(new StubIpc(true));
    expect(toggle().checked).toBe(true);
  });

  it("says how to open the window of a start in the tray", async () => {
    const { el } = await mount(new StubIpc(true));
    const hint = el.querySelector("[data-testid=hint]")?.textContent ?? "";
    expect(hint).toContain("starts in the tray");
    expect(hint).toContain('"Open tamias" in the tray menu');
  });

  it("enables autostart when the toggle is switched on", async () => {
    const ipc = new StubIpc(false);
    const { toggle, error, click } = await mount(ipc);
    await click();
    expect(ipc.calls).toEqual(["plugin:autostart|is_enabled", "plugin:autostart|enable"]);
    expect(ipc.enabled).toBe(true);
    expect(toggle().checked).toBe(true);
    expect(error()).toBeNull();
  });

  it("disables autostart when the toggle is switched off", async () => {
    const ipc = new StubIpc(true);
    const { toggle, click } = await mount(ipc);
    await click();
    expect(ipc.calls).toEqual(["plugin:autostart|is_enabled", "plugin:autostart|disable"]);
    expect(ipc.enabled).toBe(false);
    expect(toggle().checked).toBe(false);
  });

  it("shows the error and reverts the toggle when enabling fails", async () => {
    const ipc = new StubIpc(false);
    const { toggle, error, click, el } = await mount(ipc);
    ipc.fail = "launch agent not writable";
    await click();
    expect(toggle().checked).toBe(false);
    expect(error()).toBe("Cannot enable autostart: launch agent not writable");
    expect(el.querySelector("[data-testid=autostart]")).not.toBeNull();
  });

  it("shows the error and reverts the toggle when disabling fails", async () => {
    const ipc = new StubIpc(true);
    const { toggle, error, click } = await mount(ipc);
    ipc.fail = new Error("registry key locked");
    await click();
    expect(toggle().checked).toBe(true);
    expect(error()).toBe("Cannot disable autostart: registry key locked");
  });

  it("keeps the toggle off and disabled, with an error, when the state cannot be read", async () => {
    const ipc = new StubIpc(true);
    ipc.fail = "no plugin";
    const { toggle, error } = await mount(ipc);
    expect(toggle().checked).toBe(false);
    expect(toggle().disabled).toBe(true);
    expect(error()).toBe("Cannot read the autostart setting: no plugin");
  });
});
