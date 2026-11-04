import { ChangeDetectionStrategy, Component, inject, signal } from "@angular/core";
import { RouterLink } from "@angular/router";
import { HxMessage, HxSwitch } from "@gravionlabs/helix-ui";
import { Ipc, errorText } from "../core/ipc";

/**
 * The settings view. Autostart is the only setting; the OS is its source of truth (the plugin's
 * `isEnabled`), the app keeps no settings file of its own.
 */
@Component({
  selector: "app-settings",
  imports: [HxMessage, HxSwitch, RouterLink],
  templateUrl: "./settings.html",
  styleUrl: "./settings.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class Settings {
  private readonly ipc = inject(Ipc);

  /** Off until the OS answered: never shown as enabled without knowing. */
  protected readonly autostart = signal(false);
  protected readonly ready = signal(false);
  protected readonly busy = signal(false);
  protected readonly error = signal<string | null>(null);

  constructor() {
    void this.read();
  }

  private async read(): Promise<void> {
    try {
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
