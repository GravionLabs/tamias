// @vitest-environment jsdom
import { TestBed, getTestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { AUTO_UPDATE_KEY } from "./core/auto-update";
import { Ipc } from "./core/ipc";
import { AvailableUpdate, Updater } from "./core/updater";
import { StatusBar } from "./status-bar";

beforeAll(() => {
  getTestBed().initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => {
  TestBed.resetTestingModule();
  localStorage.clear();
});

class BarIpc extends Ipc {
  constructor(private readonly version: string | null) {
    super();
  }
  override async invoke<T>(): Promise<T> {
    throw new Error("unused");
  }
  override async listen(): Promise<() => void> {
    return () => undefined;
  }
  override async appVersion(): Promise<string> {
    if (this.version === null) throw new Error("not in Tauri");
    return this.version;
  }
  override async setZoom(): Promise<void> {}
}

class FakeUpdater extends Updater {
  checks = 0;
  installed: string[] = [];
  constructor(
    private result: "none" | "offline" | { version: string; installError?: string } = "none",
  ) {
    super();
  }
  override async check(): Promise<AvailableUpdate | null> {
    this.checks++;
    if (this.result === "offline") throw new Error("could not reach the update server");
    if (this.result === "none") return null;
    const { version, installError } = this.result;
    return {
      version,
      install: async () => {
        if (installError) throw new Error(installError);
        this.installed.push(version);
      },
    };
  }
}

const flush = () => new Promise((resolve) => setTimeout(resolve));

async function bar(version: string | null, updater: Updater = new FakeUpdater()) {
  TestBed.configureTestingModule({
    providers: [
      { provide: Ipc, useValue: new BarIpc(version) },
      { provide: Updater, useValue: updater },
    ],
  });
  const fixture = TestBed.createComponent(StatusBar);
  await flush();
  fixture.detectChanges();
  const el: HTMLElement = fixture.nativeElement;
  const q = (id: string) => el.querySelector<HTMLElement>(`[data-testid="${id}"]`)!;
  return { fixture, q };
}

describe("StatusBar", () => {
  it("shows the installed version", async () => {
    const { q } = await bar("0.6.0");
    expect(q("version").textContent).toBe("v0.6.0");
  });

  it("shows no version outside Tauri", async () => {
    const { q } = await bar(null);
    expect(q("version").textContent).toBe("");
  });

  it("zooms with the buttons and resets by clicking the percentage", async () => {
    const { fixture, q } = await bar("0.6.0");
    expect(q("zoom-reset").textContent?.trim()).toBe("100%");
    q("zoom-in").click();
    q("zoom-in").click();
    await flush();
    fixture.detectChanges();
    expect(q("zoom-reset").textContent?.trim()).toBe("120%");
    q("zoom-reset").click();
    await flush();
    fixture.detectChanges();
    expect(q("zoom-reset").textContent?.trim()).toBe("100%");
  });

  it("disables the buttons at the limits", async () => {
    localStorage.setItem("tamias.zoom", "200");
    const { q } = await bar("0.6.0");
    expect((q("zoom-in") as HTMLButtonElement).disabled).toBe(true);
    expect((q("zoom-out") as HTMLButtonElement).disabled).toBe(false);
  });

  describe("updates", () => {
    const text = (q: (id: string) => HTMLElement | null, id: string) => q(id)?.textContent?.trim();
    const click = async (fixture: { detectChanges(): void }, el: HTMLElement | null) => {
      el!.click();
      await flush();
      fixture.detectChanges();
    };

    it("checks at start and says the app is up to date", async () => {
      const updater = new FakeUpdater("none");
      const { q } = await bar("0.6.0", updater);
      expect(updater.checks).toBe(1);
      expect(text(q, "update-status")).toBe("Up to date");
      expect(q("update-check")).not.toBeNull();
    });

    it("offers the new version and installs it on click", async () => {
      const updater = new FakeUpdater({ version: "0.7.0" });
      const { fixture, q } = await bar("0.6.0", updater);
      expect(text(q, "update-install")).toBe("Update to v0.7.0");
      expect(updater.installed).toEqual([]);
      await click(fixture, q("update-install"));
      expect(updater.installed).toEqual(["0.7.0"]);
      expect(text(q, "update-status")).toBe("Installing...");
    });

    it("checks again with the button", async () => {
      const updater = new FakeUpdater("none");
      const { fixture, q } = await bar("0.6.0", updater);
      await click(fixture, q("update-check"));
      expect(updater.checks).toBe(2);
    });

    it("shows an offline failure and keeps the bar working", async () => {
      const { fixture, q } = await bar("0.6.0", new FakeUpdater("offline"));
      expect(text(q, "update-error")).toBe(
        "Update check failed: could not reach the update server",
      );
      expect(text(q, "version")).toBe("v0.6.0");
      await click(fixture, q("zoom-in"));
      expect(text(q, "zoom-reset")).toBe("110%");
    });

    it("shows a failed install, for example a bad signature, and offers a new check", async () => {
      const updater = new FakeUpdater({
        version: "0.7.0",
        installError: "signature verification failed",
      });
      const { fixture, q } = await bar("0.6.0", updater);
      await click(fixture, q("update-install"));
      expect(text(q, "update-error")).toBe("Update failed: signature verification failed");
      expect(q("update-check")).not.toBeNull();
    });

    it("does not check at start with the switch off, and remembers the switch", async () => {
      localStorage.setItem(AUTO_UPDATE_KEY, "off");
      const updater = new FakeUpdater("none");
      const { fixture, q } = await bar("0.6.0", updater);
      expect(updater.checks).toBe(0);
      expect((q("update-auto") as HTMLInputElement).checked).toBe(false);
      (q("update-auto") as HTMLInputElement).click();
      await flush();
      fixture.detectChanges();
      expect(localStorage.getItem(AUTO_UPDATE_KEY)).toBe("on");
    });

    it("has the switch on by default", async () => {
      const { q } = await bar("0.6.0");
      expect((q("update-auto") as HTMLInputElement).checked).toBe(true);
    });
  });
});
