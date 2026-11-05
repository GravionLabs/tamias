import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  HostListener,
  afterNextRender,
  effect,
  inject,
  signal,
  untracked,
} from "@angular/core";
import { DOCUMENT } from "@angular/common";
import { Ipc, errorText, isBusy } from "../core/ipc";
import { StoreEvents } from "../core/store-events";
import {
  localTime,
  providerLabel,
  staleText,
  todayPercent,
  usedOfText,
  usedPercent,
  windowProjection,
  windowLabel,
  type ProviderReport,
  type Report,
  type WindowReport,
} from "../core/report";

/** What a provider block says besides its windows, like the overview; `null` for a healthy one. */
export function providerNote(p: ProviderReport): string | null {
  if (p.status === "unavailable") return "Not set up on this machine, or disabled.";
  if (p.status === "stale") return staleText(p.stale_since);
  if (p.windows.length === 0) return "No data yet.";
  return null;
}

/** The providers that are set up; the unavailable ones only when none is set up. */
export function shownProviders(report: Report): ProviderReport[] {
  const setUp = report.providers.filter((p) => p.status !== "unavailable");
  return setUp.length > 0 ? setUp : report.providers;
}

/** Fill of the bar in percent: the share used, at most the full track. */
export function barWidth(w: WindowReport): number {
  return Math.min(100, Math.max(0, usedPercent(w)));
}

/** Fill of the today bar in percent: today's share of the budget, at most the full track. */
export function todayBarWidth(w: WindowReport): number {
  return Math.min(100, Math.max(0, todayPercent(w) ?? 0));
}

/** Fill of the projection bar in percent: the projected share of the monthly budget, at most the full track. */
export function projectionBarWidth(w: WindowReport, now: Date = new Date()): number {
  return Math.min(100, Math.max(0, windowProjection(w, now)?.percent ?? 0));
}

/**
 * The compact usage card of the tray popup (`/tray`): per set-up provider the windows of today's
 * report with percentage, bar in the budget colour and absolute numbers. Loads like the overview
 * (at the start and on every `store-changed`); the popup window is sized to this card.
 */
@Component({
  selector: "app-tray",
  templateUrl: "./tray.html",
  styleUrl: "./tray.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Tray {
  private readonly ipc = inject(Ipc);
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef);
  private readonly destroyRef = inject(DestroyRef);
  private loads = 0;
  private fitted = 0;

  protected readonly report = signal<Report | null>(null);
  protected readonly error = signal<string | null>(null);

  protected readonly providerLabel = providerLabel;
  protected readonly windowLabel = windowLabel;
  protected readonly usedOfText = usedOfText;
  protected readonly percent = usedPercent;
  protected readonly width = barWidth;
  protected readonly today = todayPercent;
  protected readonly todayWidth = todayBarWidth;
  protected readonly projection = (w: WindowReport) => windowProjection(w);
  protected readonly projectionWidth = (w: WindowReport) => projectionBarWidth(w);
  protected readonly note = providerNote;
  protected readonly shown = shownProviders;
  protected readonly resets = (at: string) => `resets ${localTime(at)}`;

  constructor() {
    const doc = inject(DOCUMENT);
    // The page is transparent so the rounded corners of the card show the desktop.
    doc.documentElement.classList.add("tray-popup");
    this.destroyRef.onDestroy(() => doc.documentElement.classList.remove("tray-popup"));

    const storeEvents = inject(StoreEvents);
    effect(() => {
      storeEvents.changed();
      untracked(() => void this.load());
    });

    // Sizes the popup window to the card after every change of its content.
    afterNextRender(() => {
      const element = this.host.nativeElement;
      const fit = () => {
        const height = Math.ceil(element.getBoundingClientRect().height);
        if (height > 0 && height !== this.fitted) {
          this.fitted = height;
          this.ipc.fitTrayPopup(height).catch((e: unknown) => console.error("cannot fit", e));
        }
      };
      fit();
      if (typeof ResizeObserver !== "undefined") {
        const observer = new ResizeObserver(fit);
        observer.observe(element);
        this.destroyRef.onDestroy(() => observer.disconnect());
      }
    });
  }

  @HostListener("document:keydown.escape")
  protected hide(): void {
    this.ipc.hideTrayPopup().catch((e: unknown) => console.error("cannot hide", e));
  }

  /** Keeps the popup open while the pointer is over the card (it hides when it leaves the icon). */
  @HostListener("mouseenter")
  protected enter(): void {
    this.hover(true);
  }

  @HostListener("mouseleave")
  protected leave(): void {
    this.hover(false);
  }

  private hover(over: boolean): void {
    this.ipc.trayPopupHover(over).catch((e: unknown) => console.error("cannot report hover", e));
  }

  protected open(): void {
    this.ipc.openMainWindow().catch((e: unknown) => console.error("cannot open", e));
  }

  /** Reads today's report; a busy store keeps what is shown, the next event retries. */
  async load(): Promise<void> {
    const load = ++this.loads;
    try {
      const report = await this.ipc.today();
      if (load !== this.loads) return;
      this.report.set(report);
      this.error.set(null);
    } catch (error) {
      if (load !== this.loads || isBusy(error)) return;
      this.error.set(errorText(error));
    }
  }
}
