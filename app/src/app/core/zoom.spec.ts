// @vitest-environment jsdom
import { TestBed, getTestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { Ipc } from "./ipc";
import { ZOOM_KEY, Zoom, clampZoom } from "./zoom";

beforeAll(() => {
  getTestBed().initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});

class ZoomIpc extends Ipc {
  readonly factors: number[] = [];
  fail = false;

  override invoke<T>(): Promise<T> {
    throw new Error("unused");
  }
  override async listen(): Promise<() => void> {
    return () => undefined;
  }
  override async setZoom(factor: number): Promise<void> {
    if (this.fail) throw new Error("no webview");
    this.factors.push(factor);
  }
}

function setup() {
  const ipc = new ZoomIpc();
  TestBed.configureTestingModule({ providers: [{ provide: Ipc, useValue: ipc }] });
  return { ipc, zoom: TestBed.inject(Zoom) };
}

afterEach(() => {
  TestBed.resetTestingModule();
  localStorage.clear();
  vi.restoreAllMocks();
});

describe("clampZoom", () => {
  it("keeps the steps between 50 and 200", () => {
    expect([50, 60, 100, 190, 200].map(clampZoom)).toEqual([50, 60, 100, 190, 200]);
  });

  it("holds values outside the range at the limits", () => {
    expect(clampZoom(10)).toBe(50);
    expect(clampZoom(500)).toBe(200);
  });

  it("rounds to the next step and falls back to 100 for garbage", () => {
    expect(clampZoom(104)).toBe(100);
    expect(clampZoom(Number.NaN)).toBe(100);
  });
});

describe("Zoom", () => {
  it("steps by 10 % and applies the factor", async () => {
    const { zoom, ipc } = setup();
    await zoom.zoomIn();
    await zoom.zoomOut();
    await zoom.zoomOut();
    expect(zoom.percent()).toBe(90);
    expect(ipc.factors).toEqual([1.1, 1, 0.9]);
  });

  it("stops at 200 % and 50 %", async () => {
    const { zoom } = setup();
    for (let i = 0; i < 15; i++) await zoom.zoomIn();
    expect(zoom.percent()).toBe(200);
    for (let i = 0; i < 20; i++) await zoom.zoomOut();
    expect(zoom.percent()).toBe(50);
  });

  it("resets to 100 %", async () => {
    const { zoom, ipc } = setup();
    await zoom.zoomIn();
    await zoom.reset();
    expect(zoom.percent()).toBe(100);
    expect(ipc.factors.at(-1)).toBe(1);
  });

  it("remembers the zoom and restores it at the next start", async () => {
    await setup().zoom.zoomIn();
    expect(localStorage.getItem(ZOOM_KEY)).toBe("110");
    TestBed.resetTestingModule();
    const { zoom, ipc } = setup();
    await zoom.restore();
    expect(zoom.percent()).toBe(110);
    expect(ipc.factors).toEqual([1.1]);
  });

  it("clamps a stored value out of range", async () => {
    localStorage.setItem(ZOOM_KEY, "900");
    const { zoom } = setup();
    await zoom.restore();
    expect(zoom.percent()).toBe(200);
  });

  it("applies nothing at 100 % on start", async () => {
    const { zoom, ipc } = setup();
    await zoom.restore();
    expect(ipc.factors).toEqual([]);
  });

  it("keeps the percentage when the webview cannot zoom", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    const { zoom, ipc } = setup();
    ipc.fail = true;
    await zoom.zoomIn();
    expect(zoom.percent()).toBe(110);
  });
});
