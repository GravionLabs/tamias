import { Injectable } from "@angular/core";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/** An update found on GitHub Releases. */
export interface AvailableUpdate {
  version: string;
  /** Downloads, verifies the signature, installs and relaunches the app. */
  install(): Promise<void>;
}

/** The updater plugin; components inject this, tests provide a double. */
export abstract class Updater {
  /** The newer release, or `null` when this one is the latest. Rejects when offline or the answer is invalid. */
  abstract check(): Promise<AvailableUpdate | null>;
}

/** `tauri-plugin-updater` and `tauri-plugin-process` (capabilities `updater:allow-check`, `updater:allow-download-and-install`, `process:allow-restart`). */
@Injectable()
export class TauriUpdater extends Updater {
  override async check(): Promise<AvailableUpdate | null> {
    const update = await check();
    if (!update) return null;
    return {
      version: update.version,
      install: async () => {
        await update.downloadAndInstall();
        await relaunch();
      },
    };
  }
}
