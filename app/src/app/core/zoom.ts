import { Injectable, inject, signal } from "@angular/core";
import { Ipc, errorText } from "./ipc";

export const ZOOM_MIN = 50;
export const ZOOM_MAX = 200;
export const ZOOM_STEP = 10;
export const ZOOM_DEFAULT = 100;
/** The `localStorage` key of the zoom in percent. */
export const ZOOM_KEY = "tamias.zoom";

/** `percent` rounded to a step and held between `ZOOM_MIN` and `ZOOM_MAX`; anything else is 100. */
export function clampZoom(percent: number): number {
  if (!Number.isFinite(percent)) return ZOOM_DEFAULT;
  const stepped = Math.round(percent / ZOOM_STEP) * ZOOM_STEP;
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, stepped));
}

/**
 * The zoom of the main window in percent: applied as webview zoom through `Ipc` and remembered in
 * `localStorage` so it survives a restart.
 */
@Injectable({ providedIn: "root" })
export class Zoom {
  private readonly ipc = inject(Ipc);
  readonly percent = signal(ZOOM_DEFAULT);

  /** Reads the remembered zoom and applies it; call once when the main window starts. */
  async restore(): Promise<void> {
    let stored: string | null = null;
    try {
      stored = localStorage.getItem(ZOOM_KEY);
    } catch {
      // no storage: stay at 100 %
    }
    const percent = stored === null ? ZOOM_DEFAULT : clampZoom(Number(stored));
    this.percent.set(percent);
    if (percent !== ZOOM_DEFAULT) await this.apply(percent);
  }

  zoomIn(): Promise<void> {
    return this.set(this.percent() + ZOOM_STEP);
  }

  zoomOut(): Promise<void> {
    return this.set(this.percent() - ZOOM_STEP);
  }

  reset(): Promise<void> {
    return this.set(ZOOM_DEFAULT);
  }

  private async set(percent: number): Promise<void> {
    const next = clampZoom(percent);
    this.percent.set(next);
    try {
      localStorage.setItem(ZOOM_KEY, String(next));
    } catch {
      // not remembered
    }
    await this.apply(next);
  }

  private async apply(percent: number): Promise<void> {
    try {
      await this.ipc.setZoom(percent / 100);
    } catch (error) {
      console.error(`cannot zoom: ${errorText(error)}`);
    }
  }
}
