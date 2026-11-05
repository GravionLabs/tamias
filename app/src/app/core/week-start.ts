import { Injectable, inject } from "@angular/core";
import { localeFirstDay, type FirstDay } from "../calendar/grid";
import { Ipc } from "./ipc";

/** `week_start` of `config.toml`. */
export type WeekStartSetting = "monday" | "sunday" | "saturday" | "locale";

const FIRST_DAYS: Record<Exclude<WeekStartSetting, "locale">, FirstDay> = {
  monday: 1,
  saturday: 6,
  sunday: 7,
};

/** The weekday number (1 = Monday ... 7 = Sunday) a `week_start` value stands for. */
export function firstDayOf(setting: WeekStartSetting, locale: () => FirstDay): FirstDay {
  return setting === "locale" ? locale() : FIRST_DAYS[setting];
}

/**
 * The first day of the week as `config.toml` sets it. Read once at startup (`load`); Monday until
 * then, and when the config cannot be read (the month view shows that error).
 */
@Injectable({ providedIn: "root" })
export class WeekStart {
  private readonly ipc = inject(Ipc);
  private value: WeekStartSetting = "monday";

  get setting(): WeekStartSetting {
    return this.value;
  }

  get firstDay(): FirstDay {
    return firstDayOf(this.value, localeFirstDay);
  }

  async load(): Promise<void> {
    try {
      const setting = await this.ipc.weekStart();
      if (setting in FIRST_DAYS || setting === "locale") this.value = setting;
    } catch {
      // Keep the default.
    }
  }
}
