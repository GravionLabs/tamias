import { ChangeDetectionStrategy, Component, inject, signal } from "@angular/core";
import { HxCard, HxMessage, HxProgressSpinner, HxTag } from "@gravionlabs/helix-ui";
import { Ipc } from "../core/ipc";
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
    void this.load();
  }

  /** Reads today's report from the backend. */
  async load(): Promise<void> {
    try {
      this.report.set(await this.ipc.today());
      this.error.set(null);
    } catch (error) {
      this.error.set(error instanceof Error ? error.message : String(error));
    }
  }
}
