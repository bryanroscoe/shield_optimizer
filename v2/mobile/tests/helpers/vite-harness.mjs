import { fileURLToPath } from "node:url";
import { createServer } from "vite";

/// Start a Vite dev server for a browser suite.
///
/// Every browser suite must come through here. `session.test.mjs` once built
/// its own server and so inherited the pinned `strictPort`, which turned a
/// port collision into a hard failure and took its whole file down with it
/// (GitHub #113).
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
    server: { host: "127.0.0.1", port: 5200, strictPort: false },
  });
  await server.listen();
  return { server, origin: `http://127.0.0.1:${server.httpServer.address().port}` };
}
