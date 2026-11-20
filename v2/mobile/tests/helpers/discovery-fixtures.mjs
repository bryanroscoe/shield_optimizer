export async function discoveryPage(browser, origin, options = {}) {
  const context = await browser.newContext({
    viewport: options.viewport ?? { width: 384, height: 812 },
    colorScheme: options.colorScheme ?? "dark",
    ...(options.videoDir ? { recordVideo: { dir: options.videoDir, size: options.viewport ?? { width: 384, height: 812 } } } : {}),
  });
  const page = await context.newPage();
  await page.addInitScript((savedDevices) => {
    localStorage.clear();
    if (savedDevices.length) {
      localStorage.setItem("atv.savedDevices.v1", JSON.stringify(savedDevices));
      localStorage.setItem("atv.autoConnect.v1", "0");
    }
    window.connects = [];
    window.pendingConnections = [];
    window.discoveryMode = "named";
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      switch (command) {
        case "get_entitlement": return "free";
        case "wireless_status": return { connected: false, host: null, serial: null };
        case "wireless_discover": return {
          devices: [
            { name: "adb-tv-a", friendly_name: window.discoveryMode === "named" ? "Studio TV" : null, host: "192.0.2.10", port: 5555, service: "_adb._tcp." },
            { name: "adb-tv-b", friendly_name: "Kitchen TV", host: "192.0.2.20", port: 5555, service: "_adb._tcp." },
            { name: "adb-tv-c", friendly_name: "A very long television name in the upstairs entertainment room", host: "192.0.2.30", port: 5555, service: "_adb._tcp." },
          ], message: "",
        };
        case "wireless_connect":
          window.connects.push(args);
          return new Promise(resolve => window.pendingConnections.push(resolve));
        case "wireless_cancel_connect": return;
        default: return null;
      }
    } };
  }, options.savedDevices ?? []);
  await page.goto(origin);
  if (options.savedDevices?.length) await page.getByRole("button", { name: "Scan for a different TV" }).click();
  await page.getByRole("button", { name: "Scan network" }).click();
  await page.getByRole("button", { name: /Studio TV/ }).waitFor();
  return { page, context };
}

