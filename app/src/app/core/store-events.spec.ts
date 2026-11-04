import { Injector, createEnvironmentInjector, type EnvironmentInjector } from "@angular/core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Ipc, type Unlisten } from "./ipc";
import { STORE_CHANGED, StoreEvents } from "./store-events";

/** An `Ipc` that records listeners; `emit` fires them. */
class EventIpc extends Ipc {
  readonly handlers = new Map<string, () => void>();
  readonly unlisten = vi.fn<Unlisten>();

  constructor(private readonly listening: Promise<void> = Promise.resolve()) {
    super();
  }

  override invoke<T>(): Promise<T> {
    throw new Error("no commands");
  }

  override async listen(event: string, handler: () => void): Promise<Unlisten> {
    await this.listening;
    this.handlers.set(event, handler);
    return this.unlisten;
  }

  emit(event: string): void {
    this.handlers.get(event)?.();
  }
}

let injector: EnvironmentInjector | null = null;

function storeEvents(ipc: Ipc): StoreEvents {
  injector = createEnvironmentInjector(
    [{ provide: Ipc, useValue: ipc }, StoreEvents],
    Injector.NULL as EnvironmentInjector,
  );
  return injector.get(StoreEvents);
}

/** Lets the pending `listen` promise settle. */
const flush = () => new Promise((resolve) => setTimeout(resolve));

afterEach(() => {
  injector?.destroy();
  injector = null;
});

describe("StoreEvents", () => {
  it("counts the store-changed events", async () => {
    const ipc = new EventIpc();
    const events = storeEvents(ipc);
    await flush();
    expect(events.changed()).toBe(0);

    ipc.emit(STORE_CHANGED);
    expect(events.changed()).toBe(1);
    ipc.emit(STORE_CHANGED);
    expect(events.changed()).toBe(2);
  });

  it("ignores other events", async () => {
    const ipc = new EventIpc();
    const events = storeEvents(ipc);
    await flush();

    ipc.emit("tray-clicked");

    expect(events.changed()).toBe(0);
    expect([...ipc.handlers.keys()]).toEqual(["store-changed"]);
  });

  it("stops listening when destroyed", async () => {
    const ipc = new EventIpc();
    storeEvents(ipc);
    await flush();

    injector?.destroy();
    injector = null;

    expect(ipc.unlisten).toHaveBeenCalledOnce();
  });

  it("stops a listener that is registered only after the destroy", async () => {
    let registered: () => void = () => undefined;
    const ipc = new EventIpc(new Promise((resolve) => (registered = resolve)));
    storeEvents(ipc);

    injector?.destroy();
    injector = null;
    registered();
    await flush();

    expect(ipc.unlisten).toHaveBeenCalledOnce();
  });

  it("keeps working without events when listening fails", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const ipc = new EventIpc(Promise.reject(new Error("event.listen not allowed")));
    const events = storeEvents(ipc);
    await flush();

    expect(events.changed()).toBe(0);
    expect(error).toHaveBeenCalledWith("cannot listen to store-changed", expect.any(Error));
    error.mockRestore();
  });
});
