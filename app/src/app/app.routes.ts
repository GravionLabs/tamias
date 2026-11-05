import type { Routes } from "@angular/router";
import { Calendar } from "./calendar/calendar";
import { Day } from "./day/day";
import { Overview } from "./overview/overview";
import { Tray } from "./tray/tray";
import { Settings } from "./settings/settings";

/** The calendar is the start route; today's overview of #82 stays reachable. */
export const routes: Routes = [
  { path: "", component: Calendar },
  { path: "day/:date", component: Day },
  { path: "today", component: Overview },
  { path: "tray", component: Tray },
  { path: "settings", component: Settings },
  { path: "**", redirectTo: "" },
];
