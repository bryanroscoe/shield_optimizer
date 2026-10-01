import { afterNavigate } from "$app/navigation";
import { onMount } from "svelte";
import { api } from "$lib/api";

// UI breadcrumbs for the local session recording: route changes, tab switches
// and button labels, so a recorded run reads as "clicked X, then adb said Y".
// Never input values or anything typed — only the labels the UI itself shows.

const DUPLICATE_WINDOW_MS = 500;
const LABEL_LIMIT = 120;

let enabled = import.meta.env.DEV;
let last = { key: "", at: 0 };

/// Follow the Debug logging toggle; the backend drops breadcrumbs anyway while
/// recording is off, this just saves the round trip.
export function setBreadcrumbsEnabled(on: boolean) {
  enabled = on;
}

function send(event: "route" | "tab" | "click", label: string) {
  if (!enabled) return;
  const clean = label.replace(/\s+/g, " ").trim().slice(0, LABEL_LIMIT);
  if (!clean) return;
  const key = `${event}\u0000${clean}`;
  const now = Date.now();
  if (key === last.key && now - last.at < DUPLICATE_WINDOW_MS) return;
  last = { key, at: now };
  api.logUiEvent(event, clean, window.location.pathname).catch(() => {});
}

/// Visible text of a control, skipping decorative icon glyphs (aria-hidden)
/// and any form field, whose content is user input.
function visibleText(node: Node): string {
  let text = "";
  for (const child of Array.from(node.childNodes)) {
    if (child.nodeType === Node.TEXT_NODE) {
      text += child.textContent ?? "";
    } else if (child instanceof Element) {
      if (child.getAttribute("aria-hidden") === "true") continue;
      if (child.matches("input, textarea, select, [contenteditable]")) continue;
      text += ` ${visibleText(child)}`;
    }
  }
  return text;
}

function labelOf(el: Element): string {
  return (
    el.getAttribute("aria-label") ||
    visibleText(el).trim() ||
    el.getAttribute("title") ||
    ""
  );
}

function onClick(e: MouseEvent) {
  const target = e.target instanceof Element ? e.target : null;
  const el = target?.closest("button, [role=button], [role=tab], a");
  if (!el || el.closest("[data-no-breadcrumb]")) return;
  send(el.getAttribute("role") === "tab" ? "tab" : "click", labelOf(el));
}

/// Call once during the root layout's initialisation (it registers lifecycle
/// hooks, so it cannot run later, e.g. from inside onMount).
export function installBreadcrumbs() {
  afterNavigate(({ to }) => {
    if (to?.url) send("route", to.url.pathname);
  });
  onMount(() => {
    api
      .getDebugLogging()
      .then((on) => (enabled = on))
      .catch(() => {});
    document.addEventListener("click", onClick, { capture: true });
    return () => document.removeEventListener("click", onClick, { capture: true });
  });
}
