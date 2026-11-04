import { Injectable } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { DayReport, MonthReport, Report } from "./report";

/** Stops a listener registered with `Ipc.listen`. */
export type Unlisten = () => void;

/** The commands and events of the Tauri backend; components inject this, tests provide a double. */
export abstract class Ipc {
  abstract invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;

  /** Calls `handler` for every `event` the backend emits until the returned function is called. */
  abstract listen(event: string, handler: () => void): Promise<Unlisten>;

  /** Today's report of every provider (the document of `tamias today --json`). */
  today(): Promise<Report> {
    return this.invoke<Report>("today");
  }

  /** The days of `month` (1-12) of `year` of every provider, with usage against the budget. */
  month(year: number, month: number): Promise<MonthReport> {
    return this.invoke<MonthReport>("month", { year, month });
  }

  /** The intraday curve of the local day `date` (`YYYY-MM-DD`) of every provider. */
  day(date: string): Promise<DayReport> {
    return this.invoke<DayReport>("day", { date });
  }

  /** Whether the OS has the autostart entry (the autostart plugin's `isEnabled`). */
  autostartEnabled(): Promise<boolean> {
    return this.invoke<boolean>("plugin:autostart|is_enabled");
  }

  /** Creates (`true`) or removes (`false`) the autostart entry of the OS. */
  async setAutostart(enabled: boolean): Promise<void> {
    await this.invoke<void>(enabled ? "plugin:autostart|enable" : "plugin:autostart|disable");
  }
}

/** The only caller of Tauri's `invoke` and `listen`. */
@Injectable()
export class TauriIpc extends Ipc {
  override invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    return invoke<T>(command, args);
  }

  override listen(event: string, handler: () => void): Promise<Unlisten> {
    return listen(event, () => handler());
  }
}

/** The error a command of the backend rejects with (`CommandError` in `app/src-tauri/src/lib.rs`). */
export interface CommandError {
  message: string;
  /** The store was locked longer than its busy timeout; the next refresh retries. */
  busy: boolean;
}

function isCommandError(error: unknown): error is CommandError {
  return (
    typeof error === "object" &&
    error !== null &&
    typeof (error as CommandError).message === "string" &&
    typeof (error as CommandError).busy === "boolean"
  );
}

/** Whether a rejection is a busy store: not an error state, the next refresh retries. */
export function isBusy(error: unknown): boolean {
  return isCommandError(error) && error.busy;
}

/** The text of a rejection: a command error, an `Error` or anything else. */
export function errorText(error: unknown): string {
  if (isCommandError(error) || error instanceof Error) return error.message;
  return String(error);
}
