<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/stores";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import {
    getThemePref,
    setThemePref,
    watchOsTheme,
    type ThemePref,
  } from "$lib/theme";
  import {
    getAutoUpdate,
    getLastSeenVersion,
    setAutoUpdate,
    setLastSeenVersion,
  } from "$lib/prefs";
  import { api } from "$lib/api";
  import { parseReleaseNotes, type NoteBlock } from "$lib/release-notes";
  import type { UpdateInfo } from "$lib/types";

  let { children } = $props();

  let theme = $state<ThemePref>("system");
  let autoUpdate = $state(true);
  let update = $state<UpdateInfo | null>(null);
  let pendingUpdate = $state<Update | null>(null);
  let updateBusy = $state(false);
  let updateInstalled = $state(false);
  let updateProgress = $state("");
  /// Notes for the pending update, shown before it installs. This app disables
  /// packages on a user's TV and can update itself unattended, so "what does
  /// this change?" is a question worth answering before the answer arrives.
  let notesOpen = $state(false);
  const releaseNotes = $derived<NoteBlock[]>(
    pendingUpdate?.body ? parseReleaseNotes(pendingUpdate.body) : [],
  );
  /// The version being installed comes from the updater manifest, the same
  /// place the notes do. `update.latest` is a separate GitHub API read and the
  /// two can disagree — showing one version's number above another's notes
  /// would be worse than showing neither.
  const pendingVersion = $derived(pendingUpdate?.version ?? update?.latest ?? "");
  /// Set when this launch is the first on a newly-installed version. Someone
  /// with auto-update on never sees the pre-install notes, so this is the only
  /// point at which they learn what changed. Also covers an upgrade done
  /// outside the app, via Homebrew or by replacing it by hand.
  let arrivedOn = $state<string | null>(null);
  const arrivedNotes = $derived<NoteBlock[]>(
    update?.current_notes ? parseReleaseNotes(update.current_notes) : [],
  );

  onMount(() => {
    theme = getThemePref();
    autoUpdate = getAutoUpdate();
    // Keep Auto honest while the app is open, not just at launch.
    watchOsTheme();

    api
      .checkForUpdate()
      .then((u) => {
        update = u;
        const lastSeen = getLastSeenVersion();
        // A first run has nothing to compare against, and greeting a new user
        // with "what's new" makes no sense — record the version and say
        // nothing. Notes can also be absent for a dev build or while offline.
        if (lastSeen && lastSeen !== u.current && u.current_notes) {
          arrivedOn = u.current;
        }
        setLastSeenVersion(u.current);
      })
      .catch(() => {});

    checkForUpdate();
  });

  async function checkForUpdate() {
    try {
      const available = await check();
      if (available) {
        pendingUpdate = available;
        if (autoUpdate) {
          await installUpdate();
        }
      }
    } catch {
      /* silent — network/signing failures don't block the app */
    }
  }

  async function installUpdate() {
    if (!pendingUpdate || updateBusy) return;
    updateBusy = true;
    updateProgress = "Downloading…";
    try {
      await pendingUpdate.downloadAndInstall((event) => {
        if (event.event === "Started" && event.data.contentLength) {
          updateProgress = `Downloading (${Math.round(event.data.contentLength / 1024 / 1024)} MB)…`;
        } else if (event.event === "Finished") {
          updateProgress = "Installing…";
        }
      });
      updateInstalled = true;
      updateBusy = false;
    } catch (e) {
      updateProgress = `Update failed: ${e}`;
      updateBusy = false;
    }
  }

  async function restartApp() {
    try {
      await relaunch();
    } catch (e) {
      updateProgress = `Couldn't restart automatically (${e}) — quit and reopen to finish updating.`;
      updateInstalled = false;
      updateBusy = true;
    }
  }

  function openNotes() {
    notesOpen = true;
  }

  function dismissArrived() {
    arrivedOn = null;
  }

  async function installFromNotes() {
    notesOpen = false;
    await installUpdate();
  }

  function toggleAutoUpdate() {
    autoUpdate = !autoUpdate;
    setAutoUpdate(autoUpdate);
  }

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
      {#if update}
        <button
          class="version"
          onclick={() => openUrl(update!.url)}
          title="Installed version — open the release history on GitHub"
        >
          v{update.current}
        </button>
        {#if pendingUpdate}
          {#if updateInstalled}
            <button class="update-badge installed" onclick={restartApp} title="Relaunch to finish updating">
              Update installed — Restart now ↻
            </button>
          {:else if updateBusy}
            <span class="update-badge updating">{updateProgress}</span>
          {:else}
            <button class="update-badge" onclick={openNotes} title="See what changed, then install">
              Update now → v{pendingVersion}
            </button>
          {/if}
        {:else if update.update_available}
          <button class="update-badge" onclick={() => openUrl(update!.url)} title="Open the release page">
            Update available → v{update.latest}
          </button>
        {/if}
      {:else}
        <span class="version">v2</span>
      {/if}
    </div>
    <div class="header-right">
      <nav>
        <a href="/" class:active={$page.url.pathname === "/"}>Devices</a>
        <a href="/snapshots" class:active={$page.url.pathname.startsWith("/snapshots")}>
          Snapshots
        </a>
      </nav>
      <label class="auto-update-toggle" title="Automatically download and install updates on launch">
        <input type="checkbox" checked={autoUpdate} onchange={toggleAutoUpdate} />
        Auto-update
      </label>
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
    <button class="kofi" onclick={() => openUrl("https://ko-fi.com/bryanroscoe")}>
      ☕ Enjoying Shield Optimizer? Support it on Ko-fi
    </button>
  </footer>
</div>

{#if (notesOpen || arrivedOn) && update}
  {@const arrived = arrivedOn !== null && !notesOpen}
  {@const blocks = arrived ? arrivedNotes : releaseNotes}
  <!-- Blocks come from `parseReleaseNotes`, which returns data rather than
       markup. Everything below is rendered through the template, so remote
       text cannot become HTML. -->
  <div
    class="notes-backdrop"
    role="presentation"
    onclick={() => (arrived ? dismissArrived() : (notesOpen = false))}
  ></div>
  <div class="notes-dialog" role="dialog" aria-modal="true" aria-labelledby="notes-title">
    {#if arrived}
      <h2 id="notes-title">Updated to v{update.current}</h2>
      <p class="notes-current muted">Here's what changed.</p>
    {:else}
      <h2 id="notes-title">What's new in v{pendingVersion}</h2>
      <p class="notes-current muted">You're on v{update.current}.</p>
    {/if}
    <div class="notes-body">
      {#if blocks.length === 0}
        <p class="muted">
          This release didn't come with notes. The release history on GitHub has
          the details.
        </p>
      {:else}
        {#each blocks as block, i (i)}
          {#if block.kind === "heading"}
            <h3>{#each block.spans as span}{span.text}{/each}</h3>
          {:else}
            <p class:notes-item={block.kind === "item"}>
              {#if block.kind === "item"}<span class="notes-bullet">•</span>{/if}
              <span>
                {#each block.spans as span}
                  {#if span.href}
                    <button class="notes-link" onclick={() => openUrl(span.href!)}>{span.text}</button>
                  {:else if span.bold}<strong>{span.text}</strong>
                  {:else if span.code}<code>{span.text}</code>
                  {:else}{span.text}{/if}
                {/each}
              </span>
            </p>
          {/if}
        {/each}
      {/if}
    </div>
    <div class="notes-actions">
      <button class="notes-history" onclick={() => openUrl(update!.url)}>
        All releases ↗
      </button>
      <span class="spacer"></span>
      {#if arrived}
        <button class="primary" onclick={dismissArrived}>Got it</button>
      {:else}
        <button onclick={() => (notesOpen = false)}>Not now</button>
        <button class="primary" onclick={installFromNotes}>Install v{pendingVersion}</button>
      {/if}
    </div>
  </div>
{/if}

<style>
  /* Semantic color tokens, one block per theme. $lib/theme.ts resolves the
     "system" preference to a concrete data-theme, so each palette is written
     exactly once here — an earlier version repeated the light values under a
     prefers-color-scheme query, where they could drift from this copy. The
     bare :root keeps dark as the fallback if the attribute is ever missing. */
  :global(:root),
  :global(:root[data-theme="dark"]) {
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

  /* Light values. */
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
    font-weight: 500;
    font-size: 0.9rem;
    font-family: ui-monospace, monospace;
  }
  button.version {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
  }
  button.version:hover {
    color: var(--fg-primary);
    text-decoration: underline;
  }

  .notes-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    z-index: 10;
  }
  .notes-dialog {
    position: fixed;
    z-index: 11;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(38rem, calc(100vw - 3rem));
    max-height: min(34rem, calc(100vh - 4rem));
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 1.25rem;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.4);
  }
  .notes-dialog h2 {
    margin: 0;
    font-size: 1.1rem;
  }
  .notes-current {
    margin: 0;
    font-size: 0.85rem;
  }
  .notes-body {
    overflow-y: auto;
    padding-right: 0.25rem;
    line-height: 1.5;
  }
  .notes-body h3 {
    font-size: 0.82rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--fg-muted);
    margin: 1rem 0 0.35rem;
  }
  .notes-body h3:first-child {
    margin-top: 0;
  }
  .notes-body p {
    margin: 0 0 0.4rem;
  }
  .notes-item {
    display: flex;
    gap: 0.5rem;
    align-items: baseline;
  }
  .notes-bullet {
    color: var(--fg-faint);
    flex: none;
  }
  .notes-body code {
    font-family: ui-monospace, monospace;
    font-size: 0.85em;
    background: var(--bg-muted);
    padding: 0.05rem 0.3rem;
    border-radius: 3px;
  }
  /* A button, not an anchor: these open in the system browser via the opener
     plugin, and the href is remote text we only partly trust. */
  .notes-link {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: var(--accent);
    text-decoration: underline;
    cursor: pointer;
  }
  .notes-actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding-top: 0.25rem;
    border-top: 1px solid var(--border);
  }
  .notes-actions .spacer {
    flex: 1;
  }
  .notes-history {
    font-size: 0.85rem;
  }
  .update-badge {
    margin-left: 0.6rem;
    padding: 0.15rem 0.6rem;
    border: 1px solid var(--accent);
    border-radius: 999px;
    background: var(--accent-surface, transparent);
    color: var(--accent);
    font-size: 0.78rem;
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
  }
  .update-badge:hover { background: var(--accent); color: #fff; }
  .update-badge.updating {
    cursor: default;
    opacity: 0.8;
  }
  .update-badge.installed {
    background: var(--accent);
    color: #fff;
  }
  .update-badge.installed:hover {
    background: var(--accent-strong-hover);
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
  .auto-update-toggle {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.82rem;
    color: var(--fg-muted);
    cursor: pointer;
    white-space: nowrap;
  }
  .auto-update-toggle input {
    accent-color: var(--accent);
    cursor: pointer;
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
  /* Link-styled button: external URLs must go through the opener plugin
     (a plain <a target="_blank"> doesn't reach the system browser in Tauri). */
  .kofi {
    background: none;
    border: none;
    padding: 0;
    font-size: inherit;
    color: var(--fg-muted);
    cursor: pointer;
  }
  .kofi:hover {
    color: var(--fg-primary);
    text-decoration: underline;
  }
</style>
