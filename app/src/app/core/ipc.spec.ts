import { beforeEach, describe, expect, it, vi } from "vitest";
import { TauriIpc } from "./ipc";
import type { Report } from "./report";
import { tauri } from "./tauri.mock";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("./tauri.mock")).tauri.invoke,
}));

const REPORT: Report = {
  schema_version: 1,
  generated_at: "2026-11-03T10:05:00Z",
  providers: [{ provider: "claude", status: "unavailable", windows: [] }],
};

describe("TauriIpc", () => {
  beforeEach(() => tauri.reset());

  it("reads today's report with the today command, without arguments", async () => {
    tauri.answer("today", () => REPORT);
    await expect(new TauriIpc().today()).resolves.toEqual(REPORT);
    expect(tauri.calls("today")).toEqual([undefined]);
  });

  it("passes commands and arguments through to Tauri", async () => {
    tauri.answer("echo", (args) => args);
    await expect(new TauriIpc().invoke("echo", { day: "2026-11-03" })).resolves.toEqual({
      day: "2026-11-03",
    });
  });

  it("rejects with the error of the backend", async () => {
    tauri.answer("today", () => {
      throw new Error("database schema version 9 is newer than the supported version 1");
    });
    await expect(new TauriIpc().today()).rejects.toThrow("schema version 9");
  });
});
