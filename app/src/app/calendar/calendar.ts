import {
  ChangeDetectionStrategy,
  Component,
  InjectionToken,
  computed,
  effect,
  inject,
  signal,
  untracked,
} from "@angular/core";
import { ActivatedRoute, RouterLink } from "@angular/router";
import { HxButton, HxMessage, HxProgressSpinner } from "@gravionlabs/helix-ui";
import { Collection, hasNoData } from "../core/collection";
import { Ipc, errorText, isBusy } from "../core/ipc";
import { StoreEvents } from "../core/store-events";
import {
  amount,
  providerLabel,
  staleText,
  type Colour,
  type MonthReport,
  type ProviderMonth,
  type ProviderName,
} from "../core/report";
import {
  daysInMonth,
  isoDate,
  leadingBlanks,
  localeFirstDay,
  shiftMonth,
  type FirstDay,
} from "./grid";

/** First day of the week; defaults to the locale's, tests provide a fixed one. */
export const FIRST_DAY = new InjectionToken<FirstDay>("FIRST_DAY", {
  factory: () => localeFirstDay(),
});

/** The current time; tests provide a fixed clock. */
export const NOW = new InjectionToken<() => Date>("NOW", { factory: () => () => new Date() });

/** A neutral chip: a future day, a day without data, an unavailable provider. */
export type ChipColour = Colour | "neutral";

interface Chip {
  provider: ProviderName;
  letters: string;
  colour: ChipColour;
  title: string;
}

interface Cell {
  day: number;
  date: string;
  today: boolean;
  /** Days up to today open the day view. */
  open: boolean;
  chips: Chip[];
  /** "stale since 09:00" on today's cell when a provider is stale. */
  stale: string | null;
}

const LETTERS: Record<ProviderName, string> = { claude: "C", copilot: "CP" };

/**
 * The month calendar: one cell per day, Claude and Copilot side by side, coloured by the daily budget.
 * Without data and without a collector it offers to set up collection.
 */
@Component({
  selector: "app-calendar",
  imports: [HxButton, HxMessage, HxProgressSpinner, RouterLink],
  templateUrl: "./calendar.html",
  styleUrl: "./calendar.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Calendar {
  private readonly ipc = inject(Ipc);
  private readonly now = inject(NOW);
  private readonly firstDay = inject(FIRST_DAY);
  protected readonly collection = inject(Collection);
  private loads = 0;

  // `?month=YYYY-MM` is where the day view came from; otherwise the current month.
  private readonly start = initialMonth(
    inject(ActivatedRoute).snapshot.queryParamMap.get("month"),
    this.now(),
  );
  protected readonly year = signal(this.start.year);
  protected readonly month = signal(this.start.month);
  protected readonly report = signal<MonthReport | null>(null);
  protected readonly error = signal<string | null>(null);

  protected readonly title = computed(() =>
    new Intl.DateTimeFormat(undefined, { month: "long", year: "numeric" }).format(
      new Date(this.year(), this.month() - 1, 1),
    ),
  );

  protected readonly weekdays = computed(() => {
    // 2024-01-01 is a Monday.
    const format = new Intl.DateTimeFormat(undefined, { weekday: "short" });
    return Array.from({ length: 7 }, (_, i) =>
      format.format(new Date(2024, 0, 1 + ((this.firstDay - 1 + i) % 7))),
    );
  });

  protected readonly blanks = computed(() =>
    Array.from({ length: leadingBlanks(this.year(), this.month(), this.firstDay) }),
  );

  protected readonly cells = computed<Cell[]>(() => {
    const year = this.year();
    const month = this.month();
    const report = this.report();
    const now = this.now();
    const today = isoDate(now.getFullYear(), now.getMonth() + 1, now.getDate());
    const shown = report && report.year === year && report.month === month ? report : null;
    return Array.from({ length: daysInMonth(year, month) }, (_, i) => {
      const date = isoDate(year, month, i + 1);
      const isToday = date === today;
      const providers = shown?.providers ?? [];
      const stale = isToday ? providers.find((p) => p.status === "stale") : undefined;
      return {
        day: i + 1,
        date,
        today: isToday,
        open: date <= today,
        chips: providers.map((p) => chip(p, date, date <= today, isToday)),
        stale: stale ? staleText(stale.stale_since, now) : null,
      };
    });
  });

  /**
   * "Collection is not set up — set up now?": no data and no collector, until "Not now". It stays
   * after a failed setup to show its error, unless the CLI then reports an installed collector.
   */
  protected readonly setupPrompt = computed(() => {
    const report = this.report();
    const status = this.collection.status();
    if (!report || !hasNoData(report) || !status || this.collection.dismissed()) return false;
    if (status.state === "installed") return false;
    return status.state === "not_installed" || this.collection.actionError() !== null;
  });

  /** Set up from the prompt and the CLI now reports the collector: the first data is on its way. */
  protected readonly setupDone = signal(false);

  protected readonly isCurrentMonth = computed(() => {
    const now = this.now();
    return this.year() === now.getFullYear() && this.month() === now.getMonth() + 1;
  });

  constructor() {
    // Loads at the start and again on every `store-changed` event and month change.
    const storeEvents = inject(StoreEvents);
    effect(() => {
      storeEvents.changed();
      const year = this.year();
      const month = this.month();
      untracked(() => void this.load(year, month));
    });
  }

  protected previous(): void {
    this.go(shiftMonth(this.year(), this.month(), -1));
  }

  protected next(): void {
    this.go(shiftMonth(this.year(), this.month(), 1));
  }

  protected goToday(): void {
    const now = this.now();
    this.go({ year: now.getFullYear(), month: now.getMonth() + 1 });
  }

  protected async setUp(): Promise<void> {
    await this.collection.install();
    this.setupDone.set(this.collection.status()?.state === "installed");
  }

  private go(to: { year: number; month: number }): void {
    this.year.set(to.year);
    this.month.set(to.month);
  }

  /**
   * Reads the month from the backend. A busy store keeps what is shown; the next `store-changed`
   * event retries.
   */
  async load(year: number, month: number): Promise<void> {
    // Only the latest load is shown: an earlier one that answers late is dropped.
    const load = ++this.loads;
    try {
      const report = await this.ipc.month(year, month);
      if (load !== this.loads) return;
      this.report.set(report);
      this.error.set(null);
      // Only without data: the status runs the CLI, which a filled calendar does not need.
      if (hasNoData(report) && this.collection.status() === null) void this.collection.refresh();
    } catch (error) {
      if (load !== this.loads || isBusy(error)) return;
      this.error.set(errorText(error));
    }
  }
}

/** The month of a `YYYY-MM` query value, else the month of `now`. */
function initialMonth(value: string | null, now: Date): { year: number; month: number } {
  const match = value ? /^(\d{4})-(\d{2})$/.exec(value) : null;
  const year = Number(match?.[1]);
  const month = Number(match?.[2]);
  if (match && month >= 1 && month <= 12) return { year, month };
  return { year: now.getFullYear(), month: now.getMonth() + 1 };
}

function chip(p: ProviderMonth, date: string, upToToday: boolean, isToday: boolean): Chip {
  const name = providerLabel(p.provider);
  const base = { provider: p.provider, letters: LETTERS[p.provider] };
  if (p.status === "unavailable") {
    return { ...base, colour: "neutral", title: `${name}: unavailable` };
  }
  if (isToday && p.status === "stale") {
    return { ...base, colour: "grey", title: `${name}: ${staleText(p.stale_since)}` };
  }
  const day = upToToday ? p.days.find((d) => d.date === date) : undefined;
  if (!day) return { ...base, colour: "neutral", title: `${name}: no data` };
  return {
    ...base,
    colour: day.colour,
    title: `${name}: ${amount(day.usage, p.unit)} of ${amount(day.budget, p.unit)}`,
  };
}
