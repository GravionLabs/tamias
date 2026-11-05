import {
  ApplicationConfig,
  inject,
  provideAppInitializer,
  provideBrowserGlobalErrorListeners,
} from "@angular/core";
import { WeekStart } from "./core/week-start";
import { provideRouter, withHashLocation } from "@angular/router";
import { routes } from "./app.routes";
import { Ipc, TauriIpc } from "./core/ipc";
import { provideSystemTheme } from "./core/theme";

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes, withHashLocation()),
    provideSystemTheme(),
    { provide: Ipc, useClass: TauriIpc },
    // The calendar's FIRST_DAY comes from config.toml: read before the first view is created.
    provideAppInitializer(() => inject(WeekStart).load()),
  ],
};
