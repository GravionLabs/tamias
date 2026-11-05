import { ChangeDetectionStrategy, Component, inject, signal } from "@angular/core";
import { RouterLink } from "@angular/router";
import { HxButton, HxMessage, HxSwitch } from "@gravionlabs/helix-ui";
import { Collection } from "../core/collection";
import { Ipc, errorText } from "../core/ipc";
import { WeekStart } from "../core/week-start";

/**
 * The settings view: autostart, and the collection status with "Set up" / "Remove". The OS is the
 * source of truth of autostart (the plugin's `isEnabled`), the CLI of collection (`tamias service
 * status`); the app keeps no settings file of its own.
 */
@Component({
  selector: "app-settings",
  imports: [HxButton, HxMessage, HxSwitch, RouterLink],
  templateUrl: "./settings.html",
  styleUrl: "./settings.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Settings {
  private readonly ipc = inject(Ipc);
  protected readonly collection = inject(Collection);
  /** The active week start, from `config.toml` (read only). */
  protected readonly weekStart = inject(WeekStart).setting;

  /** Off until the OS answered: never shown as enabled without knowing. */
  protected readonly autostart = signal(false);
  /** False in a dev build: its executable cannot run on its own after a reboot. */
  protected readonly available = signal(true);
  protected readonly ready = signal(false);
  protected readonly busy = signal(false);
  protected readonly error = signal<string | null>(null);

  constructor() {
    void this.read();
    void this.collection.refresh();
  }

  private async read(): Promise<void> {
    try {
      this.available.set(await this.ipc.autostartAvailable());
      this.autostart.set(await this.ipc.autostartEnabled());
      this.ready.set(true);
    } catch (error) {
      this.error.set(`Cannot read the autostart setting: ${errorText(error)}`);
    }
  }

  /** Applies the toggle; when the call fails the toggle goes back and the error shows. */
  protected async toggle(input: HTMLInputElement): Promise<void> {
    const enabled = input.checked;
    this.busy.set(true);
    this.autostart.set(enabled);
    try {
      await this.ipc.setAutostart(enabled);
      this.error.set(null);
    } catch (error) {
      // The browser already flipped the checkbox; the bound signal alone would not undo that.
      input.checked = !enabled;
      this.autostart.set(!enabled);
      this.error.set(`Cannot ${enabled ? "enable" : "disable"} autostart: ${errorText(error)}`);
    } finally {
      this.busy.set(false);
    }
  }
}
