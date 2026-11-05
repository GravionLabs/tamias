// @vitest-environment jsdom
import { TestBed, getTestBed } from "@angular/core/testing";
import { BrowserTestingModule, platformBrowserTesting } from "@angular/platform-browser/testing";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { FIRST_DAY } from "../calendar/calendar";
import { Ipc, type Unlisten } from "./ipc";
import { WeekStart, firstDayOf } from "./week-start";

beforeAll(() => {
  getTestBed().initTestEnvironment(BrowserTestingModule, platformBrowserTesting());
});
afterEach(() => TestBed.resetTestingModule());

class StubIpc extends Ipc {
  constructor(private readonly answer: () => unknown) {
    super();
  }
  override async invoke<T>(command: string): Promise<T> {
    if (command !== "week_start") throw new Error(`unexpected command ${command}`);
    return this.answer() as T;
  }
  override async listen(): Promise<Unlisten> {
    return () => undefined;
  }
}

function setup(answer: () => unknown) {
  TestBed.configureTestingModule({ providers: [{ provide: Ipc, useValue: new StubIpc(answer) }] });
  return TestBed.inject(WeekStart);
}

describe("firstDayOf", () => {
  it("maps the values to ISO weekdays and asks the locale only for locale", () => {
    expect(firstDayOf("monday", () => 3)).toBe(1);
    expect(firstDayOf("sunday", () => 3)).toBe(7);
    expect(firstDayOf("saturday", () => 3)).toBe(6);
    expect(firstDayOf("locale", () => 3)).toBe(3);
  });
});

describe("WeekStart", () => {
  it("is Monday before it is loaded", () => {
    const week = setup(() => "sunday");
    expect(week.setting).toBe("monday");
    expect(week.firstDay).toBe(1);
  });

  it("takes the value of config.toml and provides it as FIRST_DAY", async () => {
    const week = setup(() => "sunday");
    await week.load();
    expect(week.setting).toBe("sunday");
    expect(TestBed.inject(FIRST_DAY)).toBe(7);
  });

  it("keeps Monday when the config cannot be read", async () => {
    const week = setup(() => {
      throw new Error("invalid config");
    });
    await week.load();
    expect(week.setting).toBe("monday");
  });

  it("ignores an unknown value", async () => {
    const week = setup(() => "friday");
    await week.load();
    expect(week.setting).toBe("monday");
  });
});
