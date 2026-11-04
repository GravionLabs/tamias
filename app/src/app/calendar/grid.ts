/** Weekday numbers as in ISO 8601 and `Intl.Locale.getWeekInfo`: 1 = Monday ... 7 = Sunday. */
export type FirstDay = 1 | 2 | 3 | 4 | 5 | 6 | 7;

/** Number of days of `month` (1-12) of `year`. */
export function daysInMonth(year: number, month: number): number {
  return new Date(year, month, 0).getDate();
}

/** Empty cells before the 1st of `month` in a week that starts on `firstDay`. */
export function leadingBlanks(year: number, month: number, firstDay: FirstDay): number {
  const weekday = new Date(year, month - 1, 1).getDay() || 7;
  return (weekday - firstDay + 7) % 7;
}

/** The first day of the week of `locale` (Monday when the runtime cannot tell). */
export function localeFirstDay(locale: string | undefined = undefined): FirstDay {
  try {
    const l = new Intl.Locale(locale ?? navigator.language) as Intl.Locale & {
      getWeekInfo?: () => { firstDay: number };
      weekInfo?: { firstDay: number };
    };
    const day = l.getWeekInfo?.().firstDay ?? l.weekInfo?.firstDay;
    if (day !== undefined && day >= 1 && day <= 7) return day as FirstDay;
  } catch {
    // An unknown locale: fall through to the default.
  }
  return 1;
}

/** `YYYY-MM-DD` of a day. */
export function isoDate(year: number, month: number, day: number): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${String(year).padStart(4, "0")}-${pad(month)}-${pad(day)}`;
}

/** The month before or after `(year, month)`. */
export function shiftMonth(
  year: number,
  month: number,
  by: -1 | 1,
): { year: number; month: number } {
  const index = year * 12 + (month - 1) + by;
  return { year: Math.floor(index / 12), month: (index % 12) + 1 };
}
