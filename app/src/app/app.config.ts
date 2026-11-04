import { ApplicationConfig, provideBrowserGlobalErrorListeners } from "@angular/core";
import { Ipc, TauriIpc } from "./core/ipc";
import { provideSystemTheme } from "./core/theme";

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideSystemTheme(),
    { provide: Ipc, useClass: TauriIpc },
  ],
};
