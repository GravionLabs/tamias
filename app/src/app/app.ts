import { ChangeDetectionStrategy, Component, DestroyRef, inject } from "@angular/core";
import { Router, RouterOutlet } from "@angular/router";
import { Ipc } from "./core/ipc";

/** The event the backend emits to route the main window (`tray.rs`), no payload. */
export const SHOW_CALENDAR = "show-calendar";

@Component({
  selector: "app-root",
  imports: [RouterOutlet],
  template: "<router-outlet />",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class App {
  constructor() {
    const router = inject(Router);
    let destroyed = false;
    let unlisten: (() => void) | null = null;
    inject(DestroyRef).onDestroy(() => {
      destroyed = true;
      unlisten?.();
    });
    inject(Ipc)
      .listen(SHOW_CALENDAR, () => {
        // The tray card shares this shell and must stay on its own route.
        if (!router.url.startsWith("/tray")) void router.navigateByUrl("/");
      })
      .then((stop) => {
        if (destroyed) stop();
        else unlisten = stop;
      })
      .catch((error: unknown) => console.error(`cannot listen to ${SHOW_CALENDAR}`, error));
  }
}
