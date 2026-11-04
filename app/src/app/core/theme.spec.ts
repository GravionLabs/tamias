// @vitest-environment jsdom
import { TestBed, getTestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { HxTheme } from "@gravionlabs/helix-ui";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { provideSystemTheme } from "./theme";

beforeAll(() => {
  getTestBed().initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});

/** A `(prefers-color-scheme: dark)` query whose answer the test changes like the OS would. */
class FakeMedia {
  readonly listeners = new Set<(event: { matches: boolean }) => void>();

  constructor(public matches: boolean) {}

  addEventListener(type: string, listener: (event: { matches: boolean }) => void) {
    if (type === "change") this.listeners.add(listener);
  }

  removeEventListener(type: string, listener: (event: { matches: boolean }) => void) {
    if (type === "change") this.listeners.delete(listener);
  }

  set(dark: boolean) {
    this.matches = dark;
    this.listeners.forEach((listener) => listener({ matches: dark }));
  }
}

const original = window.matchMedia;

function start(dark: boolean): FakeMedia {
  const media = new FakeMedia(dark);
  window.matchMedia = ((query: string) => {
    expect(query).toBe("(prefers-color-scheme: dark)");
    return media;
  }) as unknown as typeof window.matchMedia;
  TestBed.configureTestingModule({ providers: [provideSystemTheme()] });
  TestBed.inject(HxTheme);
  TestBed.tick();
  return media;
}

const isDark = () => document.documentElement.classList.contains("app-dark");

describe("provideSystemTheme", () => {
  afterEach(() => {
    TestBed.resetTestingModule();
    window.matchMedia = original;
    document.documentElement.classList.remove("app-dark");
  });

  it("starts light when the OS is light", () => {
    start(false);
    expect(isDark()).toBe(false);
  });

  it("starts dark when the OS is dark", () => {
    start(true);
    expect(isDark()).toBe(true);
  });

  it("switches to dark and back when the OS does", () => {
    const media = start(false);
    media.set(true);
    TestBed.tick();
    expect(isDark()).toBe(true);
    media.set(false);
    TestBed.tick();
    expect(isDark()).toBe(false);
  });

  it("stops following the OS once the app is destroyed", () => {
    const media = start(false);
    expect(media.listeners.size).toBe(1);
    TestBed.resetTestingModule();
    expect(media.listeners.size).toBe(0);
  });
});
