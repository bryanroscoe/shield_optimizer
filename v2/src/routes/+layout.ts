// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
export const ssr = false;

// Screenshot / offline-UI demo mode. With VITE_DEMO=1 there's no Tauri host,
// so we install a fixture-backed `invoke()` before any page component mounts.
// No-op (and tree-shaken) in real builds. See src/lib/demo-mock.ts.
if (import.meta.env.VITE_DEMO === "1" && typeof window !== "undefined") {
  const { installDemoMock } = await import("$lib/demo-mock");
  installDemoMock();
}

// End-to-end harness mode: invoke() goes to the E2E server, which runs the
// real command layer against a simulated device. See v2/e2e/README.md.
if (import.meta.env.VITE_E2E === "1" && typeof window !== "undefined") {
  const { installE2EBridge } = await import("$lib/e2e-bridge");
  installE2EBridge();
}
