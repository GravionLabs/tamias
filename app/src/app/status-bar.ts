import { ChangeDetectionStrategy, Component, computed, inject, signal } from "@angular/core";
import { AutoUpdate } from "./core/auto-update";
import { Ipc } from "./core/ipc";
import { ZOOM_MAX, ZOOM_MIN, Zoom } from "./core/zoom";

/** The bottom bar of the main window: the installed version and the zoom control. */
@Component({
  selector: "app-status-bar",
  template: `
    <footer class="bar" data-testid="status-bar">
      <span class="left">
        <span class="version" data-testid="version">{{ version() }}</span>
        <span class="update">
          @switch (update.state().kind) {
            @case ("checking") {
              <span data-testid="update-status">Checking for updates...</span>
            }
            @case ("current") {
              <span data-testid="update-status">Up to date</span>
            }
            @case ("installing") {
              <span data-testid="update-status">Installing...</span>
            }
            @case ("error") {
              <span class="error" role="alert" data-testid="update-error">{{
                errorMessage()
              }}</span>
            }
          }
          @if (available(); as version) {
            <button type="button" data-testid="update-install" (click)="update.install()">
              Update to v{{ version }}
            </button>
          } @else {
            <button
              type="button"
              data-testid="update-check"
              [disabled]="busy()"
              (click)="update.check()"
            >
              Check for updates
            </button>
          }
          <label class="auto">
            <input
              type="checkbox"
              role="switch"
              data-testid="update-auto"
              [checked]="update.auto()"
              (change)="update.setAuto($any($event.target).checked)"
            />
            Auto-update
          </label>
        </span>
      </span>
      <span class="zoom" role="group" aria-label="Zoom">
        <button
          type="button"
          data-testid="zoom-out"
          aria-label="Zoom out"
          title="Zoom out"
          [disabled]="zoom.percent() <= min"
          (click)="zoom.zoomOut()"
        >
          &minus;
        </button>
        <button
          type="button"
          class="percent"
          data-testid="zoom-reset"
          aria-label="Reset zoom to 100 %"
          title="Reset to 100 %"
          (click)="zoom.reset()"
        >
          {{ zoom.percent() }}%
        </button>
        <button
          type="button"
          data-testid="zoom-in"
          aria-label="Zoom in"
          title="Zoom in"
          [disabled]="zoom.percent() >= max"
          (click)="zoom.zoomIn()"
        >
          +
        </button>
      </span>
    </footer>
  `,
  styles: `
    .bar {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 2px 12px;
      font-size: 0.8rem;
      color: var(--h-text-muted-color);
      border-top: 1px solid var(--h-content-border-color);
      background: var(--h-content-background);
    }
    .left,
    .update {
      display: inline-flex;
      align-items: center;
      gap: 12px;
    }
    .error {
      color: var(--h-danger-color, #c0392b);
    }
    .auto {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      cursor: pointer;
    }
    .zoom {
      display: inline-flex;
      align-items: center;
      gap: 2px;
    }
    button {
      font: inherit;
      color: inherit;
      background: none;
      border: none;
      border-radius: 4px;
      padding: 0 8px;
      cursor: pointer;
    }
    button:hover:not(:disabled) {
      background: var(--h-content-hover-background);
      color: var(--h-text-color);
    }
    button:disabled {
      opacity: 0.4;
      cursor: default;
    }
    .percent {
      min-width: 3.5em;
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class StatusBar {
  private readonly ipc = inject(Ipc);
  protected readonly zoom = inject(Zoom);
  protected readonly update = inject(AutoUpdate);
  protected readonly available = computed(() => {
    const state = this.update.state();
    return state.kind === "available" ? state.version : null;
  });
  protected readonly busy = computed(() =>
    ["checking", "installing"].includes(this.update.state().kind),
  );
  protected readonly errorMessage = computed(() => {
    const state = this.update.state();
    return state.kind === "error" ? state.message : "";
  });
  protected readonly min = ZOOM_MIN;
  protected readonly max = ZOOM_MAX;
  /** Empty outside Tauri, where the version is unknown. */
  protected readonly version = signal("");

  constructor() {
    void this.zoom.restore();
    void this.update.start();
    void this.readVersion();
  }

  private async readVersion(): Promise<void> {
    try {
      this.version.set(`v${await this.ipc.appVersion()}`);
    } catch {
      // not running in Tauri: no version to show
    }
  }
}
