import { vi } from "vitest";

/**
 * A stand-in for `@tauri-apps/api/core` and `@tauri-apps/api/event`: records `invoke` calls and
 * answers them from `answers`; `listen` registers handlers that `emit` fires.
 */
function tauriMock() {
  const answers = new Map<string, (args?: Record<string, unknown>) => unknown>();
  const listeners = new Map<string, Set<(event: { event: string; payload: unknown }) => void>>();

  const listen = vi.fn(
    async (event: string, handler: (event: { event: string; payload: unknown }) => void) => {
      const handlers = listeners.get(event) ?? new Set();
      listeners.set(event, handlers.add(handler));
      return () => {
        handlers.delete(handler);
      };
    },
  );

  const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
    const answer = answers.get(command);
    if (!answer) throw new Error(`no answer for the command ${command}`);
    return answer(args);
  });

  return {
    invoke,
    listen,
    /** Fires `event` with `payload` at every handler registered with `listen`. */
    emit(event: string, payload: unknown = null) {
      for (const handler of listeners.get(event) ?? []) handler({ event, payload });
    },
    /** Sets how `command` answers (a thrown error rejects the call). */
    answer(command: string, fn: (args?: Record<string, unknown>) => unknown) {
      answers.set(command, fn);
    },
    calls(command: string) {
      return invoke.mock.calls.filter(([c]) => c === command).map(([, args]) => args);
    },
    reset() {
      answers.clear();
      listeners.clear();
      invoke.mockClear();
      listen.mockClear();
    },
  };
}

/**
 * Shared by the `vi.mock` factory and the test, so both see the same instance:
 *
 * ```ts
 * vi.mock("@tauri-apps/api/core", async () => ({ invoke: (await import("./tauri.mock")).tauri.invoke }));
 * vi.mock("@tauri-apps/api/event", async () => ({ listen: (await import("./tauri.mock")).tauri.listen }));
 * ```
 */
export const tauri = tauriMock();
