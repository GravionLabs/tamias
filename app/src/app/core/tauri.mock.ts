import { vi } from "vitest";

/** A stand-in for `@tauri-apps/api/core`: records `invoke` calls and answers them from `answers`. */
function tauriMock() {
  const answers = new Map<string, (args?: Record<string, unknown>) => unknown>();

  const invoke = vi.fn(async (command: string, args?: Record<string, unknown>) => {
    const answer = answers.get(command);
    if (!answer) throw new Error(`no answer for the command ${command}`);
    return answer(args);
  });

  return {
    invoke,
    /** Sets how `command` answers (a thrown error rejects the call). */
    answer(command: string, fn: (args?: Record<string, unknown>) => unknown) {
      answers.set(command, fn);
    },
    calls(command: string) {
      return invoke.mock.calls.filter(([c]) => c === command).map(([, args]) => args);
    },
    reset() {
      answers.clear();
      invoke.mockClear();
    },
  };
}

/**
 * Shared by the `vi.mock` factory and the test, so both see the same instance:
 *
 * ```ts
 * vi.mock("@tauri-apps/api/core", async () => ({ invoke: (await import("./tauri.mock")).tauri.invoke }));
 * ```
 */
export const tauri = tauriMock();
