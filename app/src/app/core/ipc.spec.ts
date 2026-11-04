import { beforeEach, describe, expect, it, vi } from "vitest";
import { TauriIpc, errorText, isBusy } from "./ipc";
import type { Report } from "./report";
import { tauri } from "./tauri.mock";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("./tauri.mock")).tauri.invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("./tauri.mock")).tauri.listen,
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

  it("reads and sets autostart with the commands of the autostart plugin", async () => {
    tauri.answer("plugin:autostart|is_enabled", () => true);
    tauri.answer("plugin:autostart|enable", () => undefined);
    tauri.answer("plugin:autostart|disable", () => undefined);
    const ipc = new TauriIpc();
    await expect(ipc.autostartEnabled()).resolves.toBe(true);
    await ipc.setAutostart(true);
    await ipc.setAutostart(false);
    expect(tauri.calls("plugin:autostart|enable")).toHaveLength(1);
    expect(tauri.calls("plugin:autostart|disable")).toHaveLength(1);
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

  it("calls the handler for every emitted event until unlistened", async () => {
    const handler = vi.fn();
    const unlisten = await new TauriIpc().listen("store-changed", handler);

    tauri.emit("store-changed");
    tauri.emit("store-changed");
    tauri.emit("other-event");
    expect(handler).toHaveBeenCalledTimes(2);

    unlisten();
    tauri.emit("store-changed");
    expect(handler).toHaveBeenCalledTimes(2);
  });

  it("passes no payload to the handler", async () => {
    const handler = vi.fn();
    await new TauriIpc().listen("store-changed", handler);
    tauri.emit("store-changed", { ignored: true });
    expect(handler).toHaveBeenCalledWith();
  });
});

describe("command errors", () => {
  it("are busy only when the backend says so", () => {
    expect(isBusy({ message: "database error: database is locked", busy: true })).toBe(true);
    expect(isBusy({ message: "invalid config", busy: false })).toBe(false);
  });

  it("are not busy when the rejection is not a command error", () => {
    expect(isBusy(new Error("busy"))).toBe(false);
    expect(isBusy("busy")).toBe(false);
    expect(isBusy(null)).toBe(false);
    expect(isBusy({ busy: true })).toBe(false);
  });

  it("show the message of a command error, an Error or anything else", () => {
    expect(errorText({ message: "invalid config", busy: false })).toBe("invalid config");
    expect(errorText(new Error("database is locked"))).toBe("database is locked");
    expect(errorText("plain")).toBe("plain");
    expect(errorText(undefined)).toBe("undefined");
  });
});
