import type { Routes } from "@angular/router";
import { Calendar } from "./calendar/calendar";
import { Overview } from "./overview/overview";

/** The calendar is the start route; today's overview of #82 stays reachable. */
export const routes: Routes = [
  { path: "", component: Calendar },
  { path: "today", component: Overview },
  { path: "**", redirectTo: "" },
];
