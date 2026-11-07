import { ChangeDetectionStrategy, Component, Input } from "@angular/core";
import {
  localTime,
  projectionColour,
  projectionShortText,
  todayPercent,
  todayShortText,
  usedPercent,
  usedText,
  windowLabel,
  windowProjection,
  type WindowReport,
} from "./report";

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
 * The windows of one provider as the tray card shows them: label and percentage, the bar, and
 * under each percentage a short line (`7% used · resets 08:50`, `4% of 10% today`,
 * `≈ 240 of 300 by month end`). Shared by the tray card and the day view.
 */
@Component({
  selector: "app-window-list",
  templateUrl: "./window-list.html",
  styleUrl: "./window-list.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class WindowList {
  // A decorator input: the specs compile templates in JIT, which has no signal-input transform.
  @Input({ required: true }) windows: WindowReport[] = [];

  protected readonly windowLabel = windowLabel;
  protected readonly usedText = usedText;
  protected readonly todayShort = todayShortText;
  protected readonly projectionShort = projectionShortText;
  protected readonly percent = usedPercent;
  protected readonly width = barWidth;
  protected readonly today = todayPercent;
  protected readonly todayWidth = todayBarWidth;
  protected readonly projection = (w: WindowReport) => windowProjection(w);
  protected readonly projectionColour = projectionColour;
  protected readonly projectionWidth = (w: WindowReport) => projectionBarWidth(w);
  protected readonly resets = (at: string) => `resets ${localTime(at)}`;
}
