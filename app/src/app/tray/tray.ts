import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  HostListener,
  afterNextRender,
  afterRenderEffect,
  effect,
  inject,
  signal,
  untracked,
} from "@angular/core";
import { DOCUMENT } from "@angular/common";
import { Ipc, errorText, isBusy } from "../core/ipc";
import { StoreEvents } from "../core/store-events";
import { WindowList } from "../core/window-list";
import { providerLabel, staleText, type ProviderReport, type Report } from "../core/report";

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

/**
 * The compact usage card of the tray popup (`/tray`): per set-up provider the windows of today's
 * report with percentage, bar in the budget colour and absolute numbers. Loads like the overview
 * (at the start and on every `store-changed`); the popup window is sized to this card.
 */
@Component({
  selector: "app-tray",
  imports: [WindowList],
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
  protected readonly note = providerNote;
  protected readonly shown = shownProviders;

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

    // Sizes the popup window to the card. The ResizeObserver alone is not enough: a hidden popup
    // webview draws no frames, so it stays silent while the report loads and the window keeps the
    // height of the placeholder, clipping the rows below. The height is therefore also read after
    // every render of the card.
    const element = this.host.nativeElement;
    const fit = () => {
      const height = Math.ceil(element.getBoundingClientRect().height);
      if (height > 0 && height !== this.fitted) {
        this.fitted = height;
        this.ipc.fitTrayPopup(height).catch((e: unknown) => console.error("cannot fit", e));
      }
    };
    afterRenderEffect(() => {
      this.report();
      this.error();
      fit();
    });
    afterNextRender(() => {
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
