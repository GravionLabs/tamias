import { ApplicationConfig, provideBrowserGlobalErrorListeners } from "@angular/core";
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
  ],
};
