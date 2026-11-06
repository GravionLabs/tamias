import { Injectable, inject, signal } from "@angular/core";
import { errorText } from "./ipc";
import { AvailableUpdate, Updater } from "./updater";

/** The `localStorage` key of the auto-update switch (`"on"` or `"off"`). */
export const AUTO_UPDATE_KEY = "tamias.autoUpdate";

export type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "current" }
  | { kind: "available"; version: string }
  | { kind: "installing"; version: string }
  | { kind: "error"; message: string };

/**
 * The update flow of the status bar: check GitHub Releases, offer "Update to vX.Y.Z" and install on
 * confirmation. A failure (offline, bad signature) becomes an `error` state and never throws.
 */
@Injectable({ providedIn: "root" })
export class AutoUpdate {
  private readonly updater = inject(Updater);
  private found: AvailableUpdate | null = null;
  readonly state = signal<UpdateState>({ kind: "idle" });
  /** Check at start; on unless turned off. */
  readonly auto = signal(true);

  /** Reads the remembered switch and, when on, checks once; call when the main window starts. */
  async start(): Promise<void> {
    try {
      this.auto.set(localStorage.getItem(AUTO_UPDATE_KEY) !== "off");
    } catch {
      // no storage: stay on
    }
    if (this.auto()) await this.check();
  }

  setAuto(on: boolean): void {
    this.auto.set(on);
    try {
      localStorage.setItem(AUTO_UPDATE_KEY, on ? "on" : "off");
    } catch {
      // not remembered
    }
  }

  async check(): Promise<void> {
    if (this.busy()) return;
    this.state.set({ kind: "checking" });
    try {
      this.found = await this.updater.check();
      this.state.set(
        this.found ? { kind: "available", version: this.found.version } : { kind: "current" },
      );
    } catch (error) {
      this.found = null;
      this.state.set({ kind: "error", message: `Update check failed: ${errorText(error)}` });
    }
  }

  /** Downloads, installs and relaunches; the app restarts on success. */
  async install(): Promise<void> {
    const found = this.found;
    if (!found || this.busy()) return;
    this.state.set({ kind: "installing", version: found.version });
    try {
      await found.install();
    } catch (error) {
      this.state.set({ kind: "error", message: `Update failed: ${errorText(error)}` });
    }
  }

  private busy(): boolean {
    const kind = this.state().kind;
    return kind === "checking" || kind === "installing";
  }
}
