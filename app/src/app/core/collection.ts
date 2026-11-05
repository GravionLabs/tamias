import { Injectable, InjectionToken, inject, signal } from "@angular/core";
import { Ipc, errorText, type ServiceStatus } from "./ipc";
import type { MonthReport } from "./report";

/** Pause before the one automatic retry of a failed first status read (a cold start can time out). */
export const STATUS_RETRY_DELAY = new InjectionToken<number>("STATUS_RETRY_DELAY", {
  factory: () => 1500,
});

/** Longest wait for the status call, so a call that never settles ends as an error. */
export const STATUS_TIMEOUT = new InjectionToken<number>("STATUS_TIMEOUT", {
  factory: () => 15_000,
});

/**
 * Collection setup, shared by the calendar's first-start prompt and the settings view. The CLI is
 * the source of truth: the status is always what `tamias service status` answered last, also after
 * a failed install, never assumed.
 */
@Injectable({ providedIn: "root" })
export class Collection {
  private readonly ipc = inject(Ipc);
  private readonly retryDelay = inject(STATUS_RETRY_DELAY);
  private readonly timeout = inject(STATUS_TIMEOUT);
  private checking: Promise<void> | null = null;

  /** `null` until the CLI answered. */
  readonly status = signal<ServiceStatus | null>(null);
  /** An install or removal runs: the buttons are disabled, a second one is not started. */
  readonly running = signal(false);
  /** Why the last install or removal failed; cleared by the next one. */
  readonly actionError = signal<string | null>(null);
  /** "Not now" in the calendar's prompt, for the rest of this run of the app. */
  readonly dismissed = signal(false);

  /**
   * Reads the status from the CLI; a rejection, or no answer within `STATUS_TIMEOUT`, becomes the
   * `error` state with its message.
   */
  async refresh(): Promise<void> {
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      const timeout = new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(`no answer within ${this.timeout / 1000} s`)),
          this.timeout,
        );
      });
      this.status.set(await Promise.race([this.ipc.serviceStatus(), timeout]));
    } catch (error) {
      this.status.set({ state: "error", message: errorText(error), exe: null });
    } finally {
      clearTimeout(timer);
    }
  }

  /**
   * The first status read of a run: like `refresh`, but a failure is tried once more after a
   * short pause (a cold start can time out) before it stays the `error` state. Calls made while
   * one runs share it.
   */
  check(): Promise<void> {
    this.checking ??= (async () => {
      try {
        await this.refresh();
        if (this.status()?.state === "error") {
          await new Promise((resolve) => setTimeout(resolve, this.retryDelay));
          await this.refresh();
        }
      } finally {
        this.checking = null;
      }
    })();
    return this.checking;
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
 * Whether the store has no data yet: no provider, or every provider is unavailable (no database,
 * or one the collector never wrote to), stale without ever having succeeded, or ok without a
 * single day (a report that has nothing to show).
 */
export function hasNoData(report: MonthReport): boolean {
  return report.providers.every(
    (p) =>
      p.status === "unavailable" ||
      (p.status === "stale" && !p.stale_since) ||
      (p.status === "ok" && p.days.length === 0),
  );
}
