import { Injectable } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";
import type { Report } from "./report";

/** The commands of the Tauri backend; components inject this, tests provide a double. */
export abstract class Ipc {
  abstract invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;

  /** Today's report of every provider (the document of `tamias today --json`). */
  today(): Promise<Report> {
    return this.invoke<Report>("today");
  }
}

/** The only caller of Tauri's `invoke`. */
@Injectable()
export class TauriIpc extends Ipc {
  override invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    return invoke<T>(command, args);
  }
}
