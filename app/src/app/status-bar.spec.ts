// @vitest-environment jsdom
import { TestBed, getTestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { Ipc } from "./core/ipc";
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

const flush = () => new Promise((resolve) => setTimeout(resolve));

async function bar(version: string | null) {
  TestBed.configureTestingModule({ providers: [{ provide: Ipc, useValue: new BarIpc(version) }] });
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
});
