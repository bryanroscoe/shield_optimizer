import { fileURLToPath } from "node:url";
import { createServer } from "vite";

/// Every bare package the app imports, pre-bundled before the first page
/// load. Vite's own scan finds these too; the list makes a scan failure (as
/// More.svelte once caused) unable to reload a page mid-test.
/// tests/optimize-deps.test.mjs keeps it complete.
export const OPTIMIZE_DEPS_INCLUDE = ["@tauri-apps/api/app", "@tauri-apps/api/core", "@tauri-apps/api/event"];

/// Start a Vite dev server for a browser suite.
///
/// Every browser suite must come through here (GitHub #113): a suite that
/// builds its own server inherits the pinned `strictPort`, so a port collision
/// becomes a hard failure that takes its whole file down with it.
///
/// `vite.config.ts` pins port 1421 with `strictPort: true` because `tauri
/// android dev` expects the app there. Test suites must not inherit that: the
/// node test runner executes the suites concurrently, so several servers come
/// up at once and a pinned port makes every collision a hard failure rather
/// than something Vite steps around. Overriding the port alone is not enough --
/// `strictPort` has to be turned off too, and picking a port up front cannot be
/// made safe, because anything can take it between the check and the bind.
export async function startViteServer() {
  const server = await createServer({
    root: fileURLToPath(new URL("../../", import.meta.url)),
    logLevel: "silent",
    optimizeDeps: { include: OPTIMIZE_DEPS_INCLUDE },
    server: { host: "127.0.0.1", port: 5200, strictPort: false },
  });
  await server.listen();
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`;
  // Compile the app once here so a cold server's transform time is not
  // charged to the first test's timeout.
  await warm(server, origin);
  return { server, origin };
}

async function warm(server, origin) {
  try {
    const html = await (await fetch(`${origin}/`)).text();
    await server.transformIndexHtml("/", html);
    await server.warmupRequest("/src/main.ts");
    // warmupRequest follows static imports; wait for that graph and the
    // dependency optimizer to settle.
    await server.waitForRequestsIdle?.("/src/main.ts");
  } catch {
    // Warming is best effort; a real failure surfaces in the test itself.
  }
}
