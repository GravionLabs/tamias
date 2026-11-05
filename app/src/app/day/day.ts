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
import { amount, providerLabel, staleText, type DayReport, type ProviderDay } from "../core/report";
import { StoreEvents } from "../core/store-events";
import { ChartBox, type ChartSize } from "./chart-box";
import { HEIGHT, WIDTH, buildChart, dayBounds, emptyReason, type Chart } from "./chart";

interface Panel {
  provider: ProviderDay["provider"];
  name: string;
  /** The chart, `null` when there is nothing to draw. */
  chart: Chart | null;
  /** viewBox of the chart: its measured size, so one unit is one pixel. */
  box: ChartSize;
  /** Why there is no chart. */
  empty: string | null;
  /** "stale since 09:00" for a stale provider with a curve. */
  stale: string | null;
  summary: string | null;
  fiveHour: string | null;
  colour: string;
}

/** The intraday curve of one day (`day/:date`): Claude and Copilot side by side. */
@Component({
  selector: "app-day",
  imports: [ChartBox, HxMessage, HxProgressSpinner, RouterLink],
  templateUrl: "./day.html",
  styleUrl: "./day.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Day {
  private readonly ipc = inject(Ipc);
  private readonly now = inject(NOW);
  private loads = 0;
  private readonly params = toSignal(inject(ActivatedRoute).paramMap);

  /** Measured size of the chart box per provider; the default until measured. */
  protected readonly sizes = signal<Record<string, ChartSize>>({});
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
    const sizes = this.sizes();
    return report.providers.map((p) => panel(report.date, p, now, sizes[p.provider]));
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

  protected resized(provider: string, size: ChartSize): void {
    this.sizes.update((s) => ({ ...s, [provider]: size }));
  }

  /** Reads the day from the backend. A busy store keeps what is shown; the next event retries. */
  async load(date: string): Promise<void> {
    // Only the latest load is shown: an earlier one that answers late is dropped.
    const load = ++this.loads;
    try {
      const report = await this.ipc.day(date);
      if (load !== this.loads) return;
      this.report.set(report);
      this.error.set(null);
    } catch (error) {
      if (load !== this.loads || isBusy(error)) return;
      this.error.set(errorText(error));
    }
  }
}

function panel(date: string, p: ProviderDay, now: Date, size?: ChartSize): Panel {
  const name = providerLabel(p.provider);
  const empty = emptyReason(p, now);
  const budget = p.budget;
  const chart =
    empty === null && budget !== undefined
      ? buildChart(
          date,
          p.points,
          budget,
          p.unit,
          // Today's line runs on to the present.
          dayBounds(date).start <= now.getTime() && now.getTime() < dayBounds(date).end
            ? now.getTime()
            : undefined,
          size ?? { width: WIDTH, height: HEIGHT },
        )
      : null;
  return {
    provider: p.provider,
    name,
    chart,
    box: size ?? { width: WIDTH, height: HEIGHT },
    empty: empty ?? (chart ? null : "No budget for this day."),
    stale: p.status === "stale" && p.points.length > 0 ? staleText(p.stale_since, now) : null,
    summary:
      p.usage !== undefined && budget !== undefined
        ? `${amount(p.usage, p.unit)} of ${amount(budget, p.unit)}`
        : null,
    fiveHour: p.five_hour
      ? `5-hour window: ${amount(p.five_hour.used, "percent")} of ${amount(p.five_hour.limit, "percent")}`
      : null,
    colour: p.colour ?? "neutral",
  };
}
