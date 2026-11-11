// Shared by the desktop tests that start a Vite dev server.
//
// Vite reads `port: 0` as its default 5173, and vite.config.ts sets
// strictPort, so two runs at once collide. Ask the OS for a free port instead
// and pass it with `strictPort: true`.
//
// SvelteKit's Vite plugin takes its root and writes `.svelte-kit/` from
// process.cwd(), not from the `root` passed to createServer, so a test run
// from the repo root (`node v2/tests/x.mjs`) would build the wrong app and
// leave `.svelte-kit/` and `.vite/` there. Importing this module pins the
// working directory to v2/ before any server starts.

import { createServer } from "node:net";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const V2 = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

process.chdir(V2);

export function freePort() {
  return new Promise((resolve, reject) => {
    const probe = createServer();
    probe.unref();
    probe.on("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const { port } = probe.address();
      probe.close(() => resolve(port));
    });
  });
}
