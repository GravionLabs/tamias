import { DestroyRef, Injectable, inject, signal } from "@angular/core";
import { Ipc, type Unlisten } from "./ipc";

/** The event the backend emits when the collector wrote to the store, and every 60 s (`watch.rs`). */
export const STORE_CHANGED = "store-changed";

/**
 * Live refresh: `changed` counts the `store-changed` events. Views read it in an effect around
 * their loaders, so the loaders run once at the start and again after every event.
 */
@Injectable({ providedIn: "root" })
export class StoreEvents {
  private readonly count = signal(0);
  private unlisten: Unlisten | null = null;
  private destroyed = false;

  /** Number of `store-changed` events so far. */
  readonly changed = this.count.asReadonly();

  constructor() {
    inject(DestroyRef).onDestroy(() => {
      this.destroyed = true;
      this.unlisten?.();
    });
    inject(Ipc)
      .listen(STORE_CHANGED, () => this.count.update((n) => n + 1))
      .then((unlisten) => {
        if (this.destroyed) unlisten();
        else this.unlisten = unlisten;
      })
      // Without the listener the view stays as loaded; nothing to show in the window for it.
      .catch((error: unknown) => console.error(`cannot listen to ${STORE_CHANGED}`, error));
  }
}
