import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { sendTelemetry } from "./lib/api";
import { installGlobalErrorReporting } from "../../shared/telemetry";

async function start() {
  if (import.meta.env.VITE_DEMO === "1") {
    (await import("./lib/demo-mock")).installDemoMock();
  }
  installGlobalErrorReporting(sendTelemetry);
  mount(App, { target: document.getElementById("app")! });
}

void start();
