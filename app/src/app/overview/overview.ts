import {
  ChangeDetectionStrategy,
  Component,
  effect,
  inject,
  signal,
  untracked,
} from "@angular/core";
import { HxCard, HxMessage, HxProgressSpinner, HxTag } from "@gravionlabs/helix-ui";
import { Ipc, errorText, isBusy } from "../core/ipc";
import { StoreEvents } from "../core/store-events";
import {
  localTime,
  providerLabel,
  staleText,
  todayText,
  usedText,
  windowLabel,
  type Report,
  type Status,
  type Timestamp,
} from "../core/report";

const STATUS_SEVERITY: Record<Status, "success" | "warn" | "secondary"> = {
  ok: "success",
  stale: "warn",
  unavailable: "secondary",
};

/** Today's report: one card per provider with its windows, used amount and today's budget. */
@Component({
  selector: "app-overview",
  imports: [HxCard, HxMessage, HxProgressSpinner, HxTag],
  templateUrl: "./overview.html",
  styleUrl: "./overview.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Overview {
  private readonly ipc = inject(Ipc);
  private loads = 0;

  protected readonly report = signal<Report | null>(null);
  protected readonly error = signal<string | null>(null);

  protected readonly usedText = usedText;
  protected readonly todayText = todayText;
  protected readonly providerLabel = providerLabel;
  protected readonly windowLabel = windowLabel;
  protected readonly severity = (status: Status) => STATUS_SEVERITY[status];
  protected readonly time = (at: Timestamp) => localTime(at);
  protected readonly staleText = (since?: Timestamp) => staleText(since);

  constructor() {
    // Loads at the start and again on every `store-changed` event.
    const storeEvents = inject(StoreEvents);
    effect(() => {
      storeEvents.changed();
      untracked(() => void this.load());
    });
  }

  /**
   * Reads today's report from the backend. A busy store keeps what is shown (the loading state
   * or the last report); the next `store-changed` event retries.
   */
  async load(): Promise<void> {
    // Only the latest load is shown: an earlier one that answers late is dropped.
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
