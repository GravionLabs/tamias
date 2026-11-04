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

  /** Whether collection is set up: `tamias service status` of the bundled CLI; never rejects. */
  serviceStatus(): Promise<ServiceStatus> {
    return this.invoke<ServiceStatus>("service_status");
  }

  /** Sets up collection: `tamias service install` of the bundled CLI. */
  async serviceInstall(): Promise<void> {
    await this.invoke<string>("service_install");
  }

  /** Removes collection: `tamias service uninstall` of the bundled CLI (keeps the data). */
  async serviceUninstall(): Promise<void> {
    await this.invoke<string>("service_uninstall");
  }
}

/** Whether collection is set up (`State` in `app/src-tauri/src/service.rs`). */
export type ServiceState = "installed" | "not_installed" | "error";

/** The answer of `service_status` (`ServiceStatus` in `app/src-tauri/src/service.rs`). */
export interface ServiceStatus {
  state: ServiceState;
  /** The error, or the CLI's note on a broken installation. */
  message: string | null;
  /** The executable the scheduler runs, when the CLI names it. */
  exe: string | null;
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
