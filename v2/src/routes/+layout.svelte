<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/stores";
  import { getThemePref, setThemePref, type ThemePref } from "$lib/theme";

  let { children } = $props();

  let theme = $state<ThemePref>("system");
  onMount(() => {
    theme = getThemePref();
  });

  function pickTheme(pref: ThemePref) {
    theme = pref;
    setThemePref(pref);
  }

  const THEMES: { id: ThemePref; label: string; title: string }[] = [
    { id: "system", label: "Auto", title: "Follow the system appearance" },
    { id: "light", label: "Light", title: "Always light" },
    { id: "dark", label: "Dark", title: "Always dark" },
  ];
</script>

<div class="app">
  <header>
    <div class="brand">
      <span class="logo-dot"></span>
      <span class="title">Shield Optimizer</span>
      <span class="version">v2</span>
    </div>
    <div class="header-right">
      <nav>
        <a href="/" class:active={$page.url.pathname === "/"}>Devices</a>
        <a href="/snapshots" class:active={$page.url.pathname.startsWith("/snapshots")}>
          Snapshots
        </a>
      </nav>
      <div class="theme-toggle" role="group" aria-label="Theme">
        {#each THEMES as t (t.id)}
          <button
            class:active={theme === t.id}
            title={t.title}
            aria-pressed={theme === t.id}
            onclick={() => pickTheme(t.id)}
          >
            {t.label}
          </button>
        {/each}
      </div>
    </div>
  </header>
  <main>
    {@render children?.()}
  </main>
  <footer>
    <span class="muted">v1 (PowerShell) is still supported. See repo README.</span>
  </footer>
</div>

<style>
  /* Semantic color tokens. Dark is the default (in :root); light values are
     applied either by an explicit data-theme="light" or, when no preference is
     set, by the OS via prefers-color-scheme. Dark values are unchanged from the
     original design — only light values are new. */
  :global(:root) {
    color-scheme: dark;
    --bg-page: #0e1116;
    --bg-surface: #161b22;
    --bg-surface-2: #1c2128;
    --bg-button: #21262d;
    --bg-button-hover: #30363d;
    --bg-input: #0d1117;
    --bg-inset: #0d1117;
    --bg-muted: #3d3d3d;
    --bg-nav-active: #1f2937;
    --border: #30363d;
    --fg-primary: #e6edf3;
    --fg-secondary: #c9d1d9;
    --fg-muted: #7d8590;
    --fg-faint: #aaaaaa;
    --accent: #58a6ff;
    --accent-strong: #1f6feb;
    --accent-strong-hover: #388bfd;
    --accent-glow: #58a6ff80;
    --danger: #da3633;
    --danger-strong: #f85149;
    --danger-surface: #5d1b1b;
    --danger-border: #8b3030;
    --danger-text: #ff8a80;
    --danger-surface-text: #ffffff;
    --ok: #3fb950;
    --ok-surface: #1b3d2c;
    --warn: #d29922;
    --warn-surface: #3d2f00;
    --warn-border: #5d4a00;
    --warn-surface-2: #5d3b1b;
    --advanced: #a371f7;
  }

  /* Light values — shared by explicit light and OS-light-when-unset. */
  :global(:root[data-theme="light"]) {
    color-scheme: light;
    --bg-page: #f6f8fa;
    --bg-surface: #ffffff;
    --bg-surface-2: #f0f3f6;
    --bg-button: #f1f3f5;
    --bg-button-hover: #e7ebef;
    --bg-input: #ffffff;
    --bg-inset: #eef1f4;
    --bg-muted: #e4e8ec;
    --bg-nav-active: #ddeaff;
    --border: #d0d7de;
    --fg-primary: #1f2328;
    --fg-secondary: #424a53;
    --fg-muted: #656d76;
    --fg-faint: #6e7781;
    --accent: #0969da;
    --accent-strong: #0969da;
    --accent-strong-hover: #0860ca;
    --accent-glow: #0969da55;
    --danger: #cf222e;
    --danger-strong: #cf222e;
    --danger-surface: #ffebe9;
    --danger-border: #ff9492;
    --danger-text: #cf222e;
    --danger-surface-text: #cf222e;
    --ok: #1a7f37;
    --ok-surface: #dafbe1;
    --warn: #9a6700;
    --warn-surface: #fff8c5;
    --warn-border: #d4a72c;
    --warn-surface-2: #fff1e5;
    --advanced: #8250df;
  }
  @media (prefers-color-scheme: light) {
    :global(:root:not([data-theme])) {
      color-scheme: light;
      --bg-page: #f6f8fa;
      --bg-surface: #ffffff;
      --bg-surface-2: #f0f3f6;
      --bg-button: #f1f3f5;
      --bg-button-hover: #e7ebef;
      --bg-input: #ffffff;
      --bg-inset: #eef1f4;
      --bg-muted: #e4e8ec;
      --bg-nav-active: #ddeaff;
      --border: #d0d7de;
      --fg-primary: #1f2328;
      --fg-secondary: #424a53;
      --fg-muted: #656d76;
      --fg-faint: #6e7781;
      --accent: #0969da;
      --accent-strong: #0969da;
      --accent-strong-hover: #0860ca;
      --accent-glow: #0969da55;
      --danger: #cf222e;
      --danger-strong: #cf222e;
      --danger-surface: #ffebe9;
      --danger-border: #ff9492;
      --danger-text: #cf222e;
      --danger-surface-text: #cf222e;
      --ok: #1a7f37;
      --ok-surface: #dafbe1;
      --warn: #9a6700;
      --warn-surface: #fff8c5;
      --warn-border: #d4a72c;
      --warn-surface-2: #fff1e5;
      --advanced: #8250df;
    }
  }

  :global(html, body) {
    margin: 0;
    padding: 0;
    height: 100vh;
    font-family:
      -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Oxygen, Ubuntu,
      Cantarell, sans-serif;
    background: var(--bg-page);
    color: var(--fg-primary);
  }
  :global(*) {
    box-sizing: border-box;
  }
  :global(a) {
    color: var(--accent);
    text-decoration: none;
  }
  :global(a:hover) {
    text-decoration: underline;
  }
  :global(button) {
    background: var(--bg-button);
    color: var(--fg-primary);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.4rem 0.9rem;
    cursor: pointer;
    font-size: 0.9rem;
    font-family: inherit;
  }
  :global(button:hover) {
    background: var(--bg-button-hover);
  }
  :global(button.primary) {
    background: var(--accent-strong);
    border-color: var(--accent-strong);
    color: #fff;
  }
  :global(button.primary:hover) {
    background: var(--accent-strong-hover);
  }
  :global(button.danger) {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
  }
  :global(button.danger:hover) {
    background: var(--danger-strong);
  }
  :global(input, select) {
    background: var(--bg-input);
    color: var(--fg-primary);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.45rem 0.7rem;
    font-size: 0.9rem;
    font-family: inherit;
  }
  :global(.muted) {
    color: var(--fg-secondary);
    opacity: 0.8;
  }
  :global(.risk-safe) { color: var(--ok); }
  :global(.risk-medium) { color: var(--warn); }
  :global(.risk-high) { color: var(--danger-strong); }
  :global(.risk-advanced) { color: var(--advanced); }
  :global(.risk-unknown) { color: var(--warn); font-weight: 500; }
  :global(.risk-blocked) { color: var(--fg-muted); font-weight: 500; }

  .app {
    display: grid;
    grid-template-rows: auto 1fr auto;
    min-height: 100vh;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.8rem 1.5rem;
    border-bottom: 1px solid var(--border);
    background: var(--bg-surface);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    font-weight: 600;
  }
  .logo-dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 8px var(--accent-glow);
  }
  .title {
    font-size: 1.05rem;
  }
  .version {
    color: var(--fg-muted);
    font-weight: 400;
    font-size: 0.85rem;
  }
  .header-right {
    display: flex;
    align-items: center;
    gap: 1.4rem;
  }
  nav {
    display: flex;
    gap: 1.2rem;
  }
  nav a {
    color: var(--fg-secondary);
    font-size: 0.92rem;
    padding: 0.3rem 0.5rem;
    border-radius: 4px;
  }
  nav a.active {
    color: var(--accent);
    background: var(--bg-nav-active);
  }
  .theme-toggle {
    display: flex;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
  }
  .theme-toggle button {
    border: none;
    border-radius: 0;
    padding: 0.3rem 0.6rem;
    font-size: 0.8rem;
    background: transparent;
    color: var(--fg-secondary);
  }
  .theme-toggle button:not(:last-child) {
    border-right: 1px solid var(--border);
  }
  .theme-toggle button.active {
    background: var(--accent-strong);
    color: #fff;
  }
  .theme-toggle button:hover:not(.active) {
    background: var(--bg-button-hover);
  }
  main {
    padding: 1.5rem;
    max-width: 1100px;
    width: 100%;
    margin: 0 auto;
  }
  footer {
    padding: 0.8rem 1.5rem;
    border-top: 1px solid var(--border);
    font-size: 0.82rem;
    text-align: center;
  }
</style>
