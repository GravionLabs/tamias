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
import { ActivatedRoute, Router, RouterLink } from "@angular/router";
import { HxButton, HxMessage, HxProgressSpinner } from "@gravionlabs/helix-ui";
import { NOW } from "../calendar/calendar";
import { Ipc, errorText, isBusy } from "../core/ipc";
import { StoreEvents } from "../core/store-events";
import {
  amount,
  providerLabel,
  staleText,
  type Colour,
  type ProviderName,
  type YearReport,
} from "../core/report";

/** A bar is full at this ratio; the budget mark sits at `1 / MAX_RATIO` of the bar's height. */
export const MAX_RATIO = 1.5;

interface Bar {
  month: number;
  /** "Jan". */
  label: string;
  /** `YYYY-MM`, the month the calendar opens. */
  value: string;
  hasData: boolean;
  /** Height of the fill in percent of the bar, 0 for an empty month. */
  height: number;
  colour: Colour;
  /** "64 %" under the bar, "" for an empty month. */
  percent: string;
  title: string;
  /** Aria label of the link. */
  aria: string;
}

interface Panel {
  provider: ProviderName;
  name: string;
  /** "stale since 09:00" for a stale provider. */
  stale: string | null;
  bars: Bar[];
}

/** The year view (`year/:year`): one bar per month and provider, usage against the budget. */
@Component({
  selector: "app-year",
  imports: [HxButton, HxMessage, HxProgressSpinner, RouterLink],
  templateUrl: "./year.html",
  styleUrl: "./year.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Year {
  private readonly router = inject(Router);
  private readonly ipc = inject(Ipc);
  private readonly now = inject(NOW);
  private loads = 0;
  private readonly params = toSignal(inject(ActivatedRoute).paramMap);

  protected readonly year = computed(() => {
    const value = Number(this.params()?.get("year"));
    return Number.isInteger(value) && value >= 1970 && value <= 9999
      ? value
      : this.now().getFullYear();
  });
  protected readonly isCurrentYear = computed(() => this.year() === this.now().getFullYear());
  protected readonly report = signal<YearReport | null>(null);
  protected readonly error = signal<string | null>(null);

  protected readonly panels = computed<Panel[]>(() => {
    const report = this.report();
    if (!report || report.year !== this.year()) return [];
    const format = new Intl.DateTimeFormat(undefined, { month: "short" });
    const long = new Intl.DateTimeFormat(undefined, { month: "long" });
    const now = this.now();
    return report.providers
      .filter((p) => p.status !== "unavailable" && p.months.length > 0)
      .map((p) => {
        const name = providerLabel(p.provider);
        const bars = p.months.map<Bar>((m) => {
          const date = new Date(report.year, m.month - 1, 1);
          const label = format.format(date);
          const full = `${long.format(date)} ${report.year}`;
          const value = `${report.year}-${String(m.month).padStart(2, "0")}`;
          const future =
            report.year > now.getFullYear() ||
            (report.year === now.getFullYear() && m.month > now.getMonth() + 1);
          if (!m.has_data || future) {
            return {
              month: m.month,
              label,
              value,
              hasData: false,
              height: 0,
              colour: "grey",
              percent: "",
              title: `${name}, ${full}: no data`,
              aria: `${full}, ${name}: no data, open the month`,
            };
          }
          const text = `${amount(m.usage, p.unit)} of ${amount(m.budget, p.unit)}`;
          const percent = m.ratio === undefined ? "" : `${Math.round(m.ratio * 100)} %`;
          return {
            month: m.month,
            label,
            value,
            hasData: true,
            height: Math.min(Math.max((m.ratio ?? 0) / MAX_RATIO, 0.02), 1) * 100,
            colour: m.colour,
            percent,
            title: `${name}, ${full}: ${text}`,
            aria: `${full}, ${name}: ${text}${percent ? `, ${percent}` : ""}, open the month`,
          };
        });
        return {
          provider: p.provider,
          name,
          stale: p.status === "stale" ? staleText(p.stale_since, now) : null,
          bars,
        };
      });
  });

  /** Where the budget mark sits, in percent of the bar's height. */
  protected readonly budgetAt = 100 / MAX_RATIO;

  constructor() {
    // Loads at the start and again on every `store-changed` event and year change.
    const storeEvents = inject(StoreEvents);
    effect(() => {
      storeEvents.changed();
      const year = this.year();
      untracked(() => void this.load(year));
    });
  }

  protected previous(): void {
    void this.router.navigate(["/year", this.year() - 1]);
  }

  protected next(): void {
    void this.router.navigate(["/year", this.year() + 1]);
  }

  protected goCurrent(): void {
    void this.router.navigate(["/year", this.now().getFullYear()]);
  }

  /** Reads the year; a busy store keeps what is shown, the next `store-changed` event retries. */
  async load(year: number): Promise<void> {
    const load = ++this.loads;
    try {
      const report = await this.ipc.year(year);
      if (load !== this.loads) return;
      this.report.set(report);
      this.error.set(null);
    } catch (error) {
      if (load !== this.loads || isBusy(error)) return;
      this.error.set(errorText(error));
    }
  }
}
