import { Injectable, inject, signal } from "@angular/core";
import { Ipc, errorText, type ServiceStatus } from "./ipc";
import type { MonthReport } from "./report";

/**
 * Collection setup, shared by the calendar's first-start prompt and the settings view. The CLI is
 * the source of truth: the status is always what `tamias service status` answered last, also after
 * a failed install, never assumed.
 */
@Injectable({ providedIn: "root" })
export class Collection {
  private readonly ipc = inject(Ipc);

  /** `null` until the CLI answered. */
  readonly status = signal<ServiceStatus | null>(null);
  /** An install or removal runs: the buttons are disabled, a second one is not started. */
  readonly running = signal(false);
  /** Why the last install or removal failed; cleared by the next one. */
  readonly actionError = signal<string | null>(null);
  /** "Not now" in the calendar's prompt, for the rest of this run of the app. */
  readonly dismissed = signal(false);

  /** Reads the status from the CLI; a rejection becomes the `error` state with its message. */
  async refresh(): Promise<void> {
    try {
      this.status.set(await this.ipc.serviceStatus());
    } catch (error) {
      this.status.set({ state: "error", message: errorText(error), exe: null });
    }
  }

  /** Sets up collection, then reads the status again (also when it failed). */
  install(): Promise<void> {
    return this.act("set up", () => this.ipc.serviceInstall());
  }

  /** Removes collection, then reads the status again (also when it failed). */
  remove(): Promise<void> {
    return this.act("remove", () => this.ipc.serviceUninstall());
  }

  dismiss(): void {
    this.dismissed.set(true);
  }

  private async act(what: string, call: () => Promise<void>): Promise<void> {
    if (this.running()) return;
    this.running.set(true);
    this.actionError.set(null);
    try {
      await call();
    } catch (error) {
      this.actionError.set(`Cannot ${what} collection: ${errorText(error)}`);
    } finally {
      await this.refresh();
      this.running.set(false);
    }
  }
}

/**
 * Whether the store has no data yet: every provider is unavailable (no database, or one the
 * collector never wrote to) or stale without ever having succeeded.
 */
export function hasNoData(report: MonthReport): boolean {
  return report.providers.every(
    (p) => p.status === "unavailable" || (p.status === "stale" && !p.stale_since),
  );
}
