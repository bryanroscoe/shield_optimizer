import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";

async function start() {
  if (import.meta.env.VITE_DEMO === "1") {
    (await import("./lib/demo-mock")).installDemoMock();
  }
  mount(App, { target: document.getElementById("app")! });
}

void start();
