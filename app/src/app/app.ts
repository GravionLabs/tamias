import { ChangeDetectionStrategy, Component, DestroyRef, inject } from "@angular/core";
import { Router, RouterOutlet } from "@angular/router";
import { DOCUMENT } from "@angular/common";
import { Ipc } from "./core/ipc";
import { StatusBar } from "./status-bar";

/** The event the backend emits to route the main window (`tray.rs`), no payload. */
export const SHOW_CALENDAR = "show-calendar";

@Component({
  selector: "app-root",
  imports: [RouterOutlet, StatusBar],
  // The main window scrolls its content above a fixed status bar; the tray popup has no bar.
  template: `
    @if (tray) {
      <router-outlet />
    } @else {
      <div class="shell">
        <main class="content"><router-outlet /></main>
        <app-status-bar />
      </div>
    }
  `,
  styles: `
    .shell {
      display: flex;
      flex-direction: column;
      height: 100vh;
    }
    .content {
      flex: 1;
      min-height: 0;
      overflow: auto;
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class App {
  /** The tray popup is its own window, loaded at `#/tray`, and never navigates elsewhere. */
  protected readonly tray = inject(DOCUMENT).location.hash.startsWith("#/tray");

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
