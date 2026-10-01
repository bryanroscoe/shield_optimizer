// E2E bridge. With VITE_E2E=1 the app runs in a plain browser driven by
// Playwright, and every `invoke()` goes to the dev-only E2E server
// (`src-tauri/src/bin/e2e_server.rs`), which runs the real Rust command layer
// against a simulated device. Unlike demo mode nothing here answers a
// command itself: app commands are forwarded verbatim.
//
// Plugin calls (opener, dialog, updater, process, events) and the clipboard
// would reach the host desktop, so they get recording stubs instead. Tests
// read `window.__E2E__.calls` and can preset `window.__E2E__.dialogResult`.
//
// Wired in `+layout.ts` behind the flag; never part of a real build.

type Call = { cmd: string; args: unknown };

interface E2EState {
  server: string;
  calls: Call[];
  clipboard: string[];
  dialogResult: unknown;
}

declare global {
  interface Window {
    __E2E__?: E2EState;
  }
}

function pluginStub(cmd: string, args: Record<string, unknown>, state: E2EState): unknown {
  state.calls.push({ cmd, args });
  if (cmd === "plugin:dialog|open") return state.dialogResult ?? null;
  if (cmd === "plugin:event|listen") return state.calls.length;
  // `check()` resolves to null when there is no update.
  return null;
}

export function installE2EBridge(): void {
  const server = (import.meta.env.VITE_E2E_URL as string | undefined) ?? "http://127.0.0.1:1423";
  const state: E2EState = { server, calls: [], clipboard: [], dialogResult: null };
  window.__E2E__ = state;
  const callbacks = new Map<number, unknown>();
  let nextId = 1;

  const w = window as unknown as { __TAURI_INTERNALS__?: unknown };
  w.__TAURI_INTERNALS__ = {
    invoke: async (cmd: string, args: Record<string, unknown> = {}) => {
      if (cmd.startsWith("plugin:")) return pluginStub(cmd, args, state);
      const response = await fetch(`${server}/invoke`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ cmd, args }),
      });
      const result = (await response.json()) as { ok: boolean; value?: unknown; error?: unknown };
      if (!response.ok) throw (result as { error?: unknown }).error ?? `E2E server ${response.status}`;
      if (result.ok) return result.value;
      throw result.error;
    },
    transformCallback: (cb: unknown) => {
      const id = nextId++;
      callbacks.set(id, cb);
      return id;
    },
    unregisterCallback: (id: number) => callbacks.delete(id),
    convertFileSrc: (path: string) => path,
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { label: "main", windowLabel: "main" },
    },
  };

  try {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: async (text: string) => {
          state.clipboard.push(text);
        },
        readText: async () => state.clipboard.at(-1) ?? "",
      },
    });
  } catch {
    // Some browsers refuse to redefine it; tests then read the real clipboard.
  }
}
