import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  signal,
  untracked,
} from "@angular/core";
import { toSignal } from "@angular/core/rxjs-interop";
import { ActivatedRoute, RouterLink } from "@angular/router";
import { HxMessage, HxProgressSpinner } from "@gravionlabs/helix-ui";
import { NOW } from "../calendar/calendar";
import { Ipc, errorText, isBusy } from "../core/ipc";
import {
  amount,
  providerLabel,
  staleText,
  type DayReport,
  type ProviderDay,
  type Report,
  type WindowReport,
} from "../core/report";
import { StoreEvents } from "../core/store-events";
import { WindowList } from "../core/window-list";

/** First instant of the local day `date` (`YYYY-MM-DD`) and the first of the next one. */
export function dayBounds(date: string): { start: number; end: number } {
  const [year, month, day] = date.split("-").map(Number) as [number, number, number];
  return {
    start: new Date(year, month - 1, day).getTime(),
    end: new Date(year, month - 1, day + 1).getTime(),
  };
}

/** Why a provider has nothing to show for the day, `null` when it has data. */
export function emptyReason(p: ProviderDay, now: Date): string | null {
  if (p.status === "unavailable") return "Not set up or switched off.";
  if (p.points.length > 0) return null;
  const stale = p.status === "stale" ? ` The provider is ${staleText(p.stale_since, now)}.` : "";
  return `No data for this day.${stale}`;
}

interface Panel {
  provider: ProviderDay["provider"];
  name: string;
  /** Why the day has no data. */
  empty: string | null;
  /** "stale since 09:00" for a stale provider with data. */
  stale: string | null;
  summary: string | null;
  /** The windows like the tray card shows them; empty while today's report is not read. */
  windows: WindowReport[];
  colour: string;
}

/** One day (`day/:date`): per provider the usage of the day against its budget and the windows as the tray card shows them, Claude and Copilot side by side. */
@Component({
  selector: "app-day",
  imports: [HxMessage, HxProgressSpinner, RouterLink, WindowList],
  templateUrl: "./day.html",
  styleUrl: "./day.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Day {
  private readonly ipc = inject(Ipc);
  private readonly now = inject(NOW);
  private loads = 0;
  private readonly params = toSignal(inject(ActivatedRoute).paramMap);

  /** Today's report, whose windows the panels show; `null` until read or when it cannot be read. */
  private readonly overview = signal<Report | null>(null);
  protected readonly date = computed(() => this.params()?.get("date") ?? "");
  protected readonly report = signal<DayReport | null>(null);
  protected readonly error = signal<string | null>(null);

  /** `YYYY-MM` of the day, the month to go back to. */
  protected readonly month = computed(() => this.date().slice(0, 7));

  protected readonly title = computed(() => {
    const date = this.date();
    if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) return date;
    return new Intl.DateTimeFormat(undefined, { dateStyle: "full" }).format(
      new Date(dayBounds(date).start),
    );
  });

  protected readonly panels = computed<Panel[]>(() => {
    const report = this.report();
    if (!report || report.date !== this.date()) return [];
    const now = this.now();
    const windows = this.overview()?.providers ?? [];
    return report.providers.map((p) =>
      panel(p, now, windows.find((w) => w.provider === p.provider)?.windows ?? []),
    );
  });

  constructor() {
    // Loads at the start and again on every `store-changed` event and date change.
    const storeEvents = inject(StoreEvents);
    effect(() => {
      storeEvents.changed();
      const date = this.date();
      untracked(() => void this.load(date));
    });
  }

  /** Reads the day from the backend. A busy store keeps what is shown; the next event retries. */
  async load(date: string): Promise<void> {
    // Only the latest load is shown: an earlier one that answers late is dropped.
    const load = ++this.loads;
    try {
      const [report, overview] = await Promise.all([
        this.ipc.day(date),
        // The windows are extra: a failing overview leaves the panels without them.
        this.ipc.today().catch(() => null),
      ]);
      if (load !== this.loads) return;
      this.report.set(report);
      this.overview.set(overview);
      this.error.set(null);
    } catch (error) {
      if (load !== this.loads || isBusy(error)) return;
      this.error.set(errorText(error));
    }
  }
}

function panel(p: ProviderDay, now: Date, windows: WindowReport[]): Panel {
  const budget = p.budget;
  return {
    provider: p.provider,
    name: providerLabel(p.provider),
    empty: emptyReason(p, now) ?? (budget === undefined ? "No budget for this day." : null),
    stale: p.status === "stale" && p.points.length > 0 ? staleText(p.stale_since, now) : null,
    summary:
      p.usage !== undefined && budget !== undefined
        ? `${amount(p.usage, p.unit)} of ${amount(budget, p.unit)}`
        : null,
    windows,
    colour: p.colour ?? "neutral",
  };
}
