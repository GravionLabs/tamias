// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { TestBed, getTestBed } from "@angular/core/testing";
import { provideRouter } from "@angular/router";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { Ipc, type ServiceStatus, type Unlisten } from "../core/ipc";
import { Collection } from "../core/collection";
import { Settings } from "./settings";

beforeAll(() => {
  getTestBed().initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => TestBed.resetTestingModule());

const INSTALLED: ServiceStatus = {
  state: "installed",
  message: null,
  exe: "/Applications/tamias.app/Contents/MacOS/tamias",
};
const NOT_INSTALLED: ServiceStatus = { state: "not_installed", message: null, exe: null };

/**
 * An `Ipc` that plays the OS: `enabled` is the autostart entry, `fail` makes the next autostart
 * calls reject. `service` is what `tamias service status` reports; `install` and `uninstall` are
 * what the service commands do (a rejection fails the command).
 */
class StubIpc extends Ipc {
  calls: string[] = [];
  serviceCalls: string[] = [];
  fail: unknown = null;
  available = true;
  service: ServiceStatus = NOT_INSTALLED;
  install: () => Promise<void> = async () => {
    this.service = INSTALLED;
  };
  uninstall: () => Promise<void> = async () => {
    this.service = NOT_INSTALLED;
  };

  constructor(public enabled: boolean) {
    super();
  }

  override async invoke<T>(command: string): Promise<T> {
    if (command.startsWith("service_")) return this.serviceCommand(command) as T;
    this.calls.push(command);
    if (this.fail) throw this.fail;
    if (command === "autostart_available") return this.available as T;
    if (command === "plugin:autostart|is_enabled") return this.enabled as T;
    if (command === "autostart_enable") this.enabled = true;
    else if (command === "plugin:autostart|disable") this.enabled = false;
    else throw new Error(`unexpected command ${command}`);
    return undefined as T;
  }

  override async listen(): Promise<Unlisten> {
    return () => undefined;
  }

  serviceCommand = async (command: string): Promise<unknown> => {
    this.serviceCalls.push(command);
    if (command === "service_status") return this.service;
    if (command === "service_install") return this.install();
    if (command === "service_uninstall") return this.uninstall();
    throw new Error(`unexpected command ${command}`);
  };
}

/** A promise and its resolve function: a command that runs until the test lets it end. */
function deferred(): { promise: Promise<void>; resolve: () => void } {
  let resolve!: () => void;
  const promise = new Promise<void>((r) => (resolve = r));
  return { promise, resolve };
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
  const find = (testid: string) => el.querySelector<HTMLElement>(`[data-testid=${testid}]`);
  const text = (testid: string) => find(testid)?.textContent?.trim() ?? null;
  const press = async (testid: string) => {
    find(testid)!.click();
    await settle();
  };
  return {
    el,
    toggle,
    error,
    click,
    find,
    text,
    press,
    settle,
    detect: () => fixture.detectChanges(),
  };
}

describe("Settings", () => {
  it("shows the toggle off when the OS has no autostart entry", async () => {
    const { toggle, error } = await mount(new StubIpc(false));
    expect(toggle().checked).toBe(false);
    expect(toggle().disabled).toBe(false);
    expect(error()).toBeNull();
  });

  it("disables the toggle with a hint when autostart is not available (dev build)", async () => {
    const ipc = new StubIpc(false);
    ipc.available = false;
    const { toggle, text } = await mount(ipc);
    expect(toggle().disabled).toBe(true);
    expect(text("autostart-unavailable")).toBe(
      "Autostart is only available in an installed build.",
    );
  });

  it("shows no unavailable hint in an installed build", async () => {
    const { find } = await mount(new StubIpc(false));
    expect(find("autostart-unavailable")).toBeNull();
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
    expect(ipc.calls).toEqual([
      "autostart_available",
      "plugin:autostart|is_enabled",
      "autostart_enable",
    ]);
    expect(ipc.enabled).toBe(true);
    expect(toggle().checked).toBe(true);
    expect(error()).toBeNull();
  });

  it("disables autostart when the toggle is switched off", async () => {
    const ipc = new StubIpc(true);
    const { toggle, click } = await mount(ipc);
    await click();
    expect(ipc.calls).toEqual([
      "autostart_available",
      "plugin:autostart|is_enabled",
      "plugin:autostart|disable",
    ]);
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

describe("Settings: collection", () => {
  function stub(service: ServiceStatus): StubIpc {
    const ipc = new StubIpc(false);
    ipc.service = service;
    return ipc;
  }

  it("shows an installed collector with its exe and offers to remove it", async () => {
    const { text, find } = await mount(stub(INSTALLED));
    expect(find("collection")?.dataset["state"]).toBe("installed");
    expect(text("collection-state")).toBe("Collection is set up.");
    expect(text("collection-exe")).toBe("Runs /Applications/tamias.app/Contents/MacOS/tamias");
    expect(find("collection-remove")).not.toBeNull();
    expect(find("collection-setup")).toBeNull();
  });

  it("shows a missing collector and offers to set it up", async () => {
    const { text, find } = await mount(stub(NOT_INSTALLED));
    expect(find("collection")?.dataset["state"]).toBe("not_installed");
    expect(text("collection-state")).toBe("Collection is not set up.");
    expect(find("collection-setup")).not.toBeNull();
    expect(find("collection-remove")).toBeNull();
  });

  it("shows the note of a broken installation", async () => {
    const { text } = await mount(
      stub({
        state: "not_installed",
        message: "installed but not loaded; run `tamias service install` again",
        exe: null,
      }),
    );
    expect(text("collection-message")).toBe(
      "installed but not loaded; run `tamias service install` again",
    );
  });

  it("shows the error state with its message", async () => {
    const { text, find } = await mount(
      stub({
        state: "error",
        message: "`tamias service status` did not answer within 10 s",
        exe: null,
      }),
    );
    expect(find("collection")?.dataset["state"]).toBe("error");
    expect(text("collection-state")).toBe("Cannot read the collection status.");
    expect(text("collection-message")).toBe("`tamias service status` did not answer within 10 s");
  });

  it("shows a rejected status call as the error state", async () => {
    const ipc = stub(NOT_INSTALLED);
    ipc.serviceCommand = async () => {
      throw new Error("command service_status not found");
    };
    const { find, text } = await mount(ipc);
    expect(find("collection")?.dataset["state"]).toBe("error");
    expect(text("collection-message")).toBe("command service_status not found");
  });

  it("sets up collection and shows the status the CLI reports afterwards", async () => {
    const ipc = stub(NOT_INSTALLED);
    const { press, text, find } = await mount(ipc);
    await press("collection-setup");
    expect(ipc.serviceCalls).toEqual(["service_status", "service_install", "service_status"]);
    expect(text("collection-state")).toBe("Collection is set up.");
    expect(find("collection-error")).toBeNull();
  });

  it("removes collection and reads the status again", async () => {
    const ipc = stub(INSTALLED);
    const { press, text } = await mount(ipc);
    await press("collection-remove");
    expect(ipc.serviceCalls).toEqual(["service_status", "service_uninstall", "service_status"]);
    expect(text("collection-state")).toBe("Collection is not set up.");
  });

  it("shows a failed setup and the status read again from the CLI, not assumed", async () => {
    const ipc = stub(NOT_INSTALLED);
    ipc.install = async () => {
      // The CLI got half-way: what it reports afterwards is what the view shows.
      ipc.service = {
        state: "not_installed",
        message: "installed but not loaded; run `tamias service install` again",
        exe: null,
      };
      throw { message: "`tamias service install` failed (exit status: 1): launchctl", busy: false };
    };
    const { press, text, find } = await mount(ipc);
    await press("collection-setup");
    expect(ipc.serviceCalls).toEqual(["service_status", "service_install", "service_status"]);
    expect(text("collection-error")).toBe(
      "Cannot set up collection: `tamias service install` failed (exit status: 1): launchctl",
    );
    expect(text("collection-state")).toBe("Collection is not set up.");
    expect(text("collection-message")).toContain("installed but not loaded");
    expect(find("collection-setup")).not.toBeNull();
  });

  it("disables Set up while the setup runs and starts it only once", async () => {
    const ipc = stub(NOT_INSTALLED);
    const running = deferred();
    let installs = 0;
    ipc.install = async () => {
      installs += 1;
      await running.promise;
      ipc.service = INSTALLED;
    };
    const { find, settle, text, detect } = await mount(ipc);
    const button = () => find("collection-setup") as HTMLButtonElement;

    button().click();
    detect();
    expect(button().disabled).toBe(true);
    button().click();
    // Also a second start that bypasses the button (the calendar's prompt) does not run.
    await TestBed.inject(Collection).install();
    expect(installs).toBe(1);
    expect(ipc.serviceCalls).toEqual(["service_status", "service_install"]);

    running.resolve();
    // The install and the status read after it are plain promises, not tracked by whenStable.
    await new Promise((r) => setTimeout(r, 0));
    await settle();
    expect((find("collection-remove") as HTMLButtonElement).disabled).toBe(false);
    expect(installs).toBe(1);
    expect(text("collection-state")).toBe("Collection is set up.");
  });
});
