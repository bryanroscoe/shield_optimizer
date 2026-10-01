<script lang="ts">
  import "../app.css";
  import Icon from "$lib/components/Icon.svelte";
  import BrandMark from "$lib/components/BrandMark.svelte";
  import ContextMenu from "$lib/components/ContextMenu.svelte";
  import ReleaseNotes from "$lib/components/ReleaseNotes.svelte";
  import { onMount } from "svelte";
  import { page } from "$app/stores";
  import { getVersion } from "@tauri-apps/api/app";
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
    getLegacyLastSeenVersion,
    getNotesSeenVersion,
    setAutoUpdate,
    setNotesSeenVersion,
  } from "$lib/prefs";
  import { api } from "$lib/api";
  import { installBreadcrumbs, setBreadcrumbsEnabled } from "$lib/breadcrumbs";
  import { parseReleaseNotes, type NoteBlock } from "$lib/release-notes";
  import { notesFor, parseChangelog, recentReleases } from "$lib/changelog";
  import { decideArrival } from "$lib/notes-seen";
  import { isNewerVersion } from "$lib/version";
  import type { UpdateInfo } from "$lib/types";
  // Bundled at build time, so the notes for the running version are always
  // there — offline, rate-limited, or seconds after a relaunch.
  import changelogSource from "../../CHANGELOG.md?raw";

  let { children } = $props();

  installBreadcrumbs();
  const RELEASES_PAGE = "https://github.com/bryanroscoe/shield_optimizer/releases";
  const changelog = parseChangelog(changelogSource);

  let theme = $state<ThemePref>("system");
  let autoUpdate = $state(true);
  let update = $state<UpdateInfo | null>(null);
  /// The running version, read from the app itself rather than from the
  /// GitHub API call, which can take seconds or fail outright.
  let appVersion = $state<string | null>(null);
  const currentVersion = $derived(appVersion ?? update?.current ?? null);
  let pendingUpdate = $state<Update | null>(null);
  let updateBusy = $state(false);
  let updateInstalled = $state(false);
  let updateProgress = $state("");
  /// The "Restart now / Later" prompt that follows a finished install.
  let restartPromptOpen = $state(false);
  let restarting = $state(false);
  let restartFailed = $state(false);
  /// On Windows the updater hands off to the NSIS/MSI installer and exits the
  /// app itself; the installer starts the new version. Offering Restart there
  /// as well could launch it twice, so that path never shows the prompt.
  const installerRelaunches =
    typeof navigator !== "undefined" && /Windows/i.test(navigator.userAgent);
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
  /// Release history: opened by the version button at any time, and on its
  /// own once after an upgrade (`arrived`). Someone with auto-update on never
  /// sees the pre-install notes, so the arrival is the only point at which
  /// they learn what changed. Also covers an upgrade done outside the app,
  /// via Homebrew or by replacing it by hand.
  let historyOpen = $state(false);
  let arrived = $state(false);
  const history = $derived(
    currentVersion
      ? recentReleases(changelog, currentVersion).map((entry) => ({
          ...entry,
          blocks: parseReleaseNotes(entry.body),
        }))
      : [],
  );

  /// GitHub has published a tag the updater manifest has not caught up with.
  ///
  /// There is a real window between a tag push and `latest.json` propagating,
  /// and the app used to fill it with a second clickable "Update available"
  /// badge sourced from the GitHub API — which could only open a release page,
  /// because the updater had nothing to install. Two badges, one of them a
  /// dead end. Only `pendingUpdate` is clickable now; this says the true thing
  /// instead, and says it inertly.
  const rollingOut = $derived(
    update?.update_available && update.latest
      ? !pendingUpdate || isNewerVersion(update.latest, pendingUpdate.version)
      : false,
  );

  onMount(() => {
    theme = getThemePref();
    autoUpdate = getAutoUpdate();
    // Keep Auto honest while the app is open, not just at launch.
    watchOsTheme();

    getVersion()
      .then((running) => {
        appVersion = running;
        announceArrival(running);
      })
      .catch(() => {});

    api
      .checkForUpdate()
      .then((u) => {
        update = u;
      })
      .catch(() => {});

    checkForUpdate();
  });

  function announceArrival(running: string) {
    const decision = decideArrival({
      running,
      notesSeen: getNotesSeenVersion(),
      legacyLastSeen: getLegacyLastSeenVersion(),
      hasNotes: notesFor(changelog, running) !== null,
      isNewer: isNewerVersion,
    });
    if (decision.show) {
      arrived = true;
      historyOpen = true;
    } else if (decision.record) {
      setNotesSeenVersion(decision.record);
    }
  }

  function openHistory() {
    arrived = false;
    historyOpen = true;
  }

  function closeHistory() {
    // Recorded on dismissal, not on display: a launch that never got as far
    // as showing the notes leaves them owed.
    if (arrived && appVersion) setNotesSeenVersion(appVersion);
    historyOpen = false;
    arrived = false;
  }

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

  function megabytes(bytes: number): string {
    return (bytes / 1024 / 1024).toFixed(bytes < 10 * 1024 * 1024 ? 1 : 0);
  }

  async function installUpdate() {
    if (!pendingUpdate || updateBusy) return;
    updateBusy = true;
    updateProgress = "Downloading…";
    let total = 0;
    let received = 0;
    try {
      await pendingUpdate.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
          received = 0;
          updateProgress = total ? `Downloading 0% of ${megabytes(total)} MB…` : "Downloading…";
        } else if (event.event === "Progress") {
          received += event.data.chunkLength;
          updateProgress = total
            ? `Downloading ${Math.min(100, Math.floor((received / total) * 100))}% of ${megabytes(total)} MB…`
            : `Downloading ${megabytes(received)} MB…`;
        } else if (event.event === "Finished") {
          updateProgress = installerRelaunches
            ? "Installing — the app will close and reopen on its own…"
            : "Installing…";
        }
      });
      if (installerRelaunches) {
        // Normally unreachable: the installer has already exited the app. If
        // it did not, say what is true rather than offering a second launch.
        updateProgress = "Installed. Quit and reopen the app to finish.";
        return;
      }
      updateInstalled = true;
      updateBusy = false;
      restartPromptOpen = true;
    } catch (e) {
      updateProgress = `Update failed: ${e}`;
      updateBusy = false;
    }
  }

  async function restartApp() {
    if (restarting) return;
    restarting = true;
    restartFailed = false;
    try {
      await relaunch();
    } catch {
      restartFailed = true;
      restartPromptOpen = true;
    } finally {
      restarting = false;
    }
  }

  function openNotes() {
    notesOpen = true;
  }

  async function installFromNotes() {
    notesOpen = false;
    await installUpdate();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    if (bugOpen) bugOpen = false;
    else if (restartPromptOpen) restartPromptOpen = false;
    else if (notesOpen) notesOpen = false;
    else if (historyOpen) closeHistory();
    else return;
    e.preventDefault();
  }

  // ---- Report a bug -------------------------------------------------
  //
  // Everything here is text the user reads before anything leaves the
  // machine. The app has no upload path: the only way out is the issue URL,
  // which the user opens and GitHub shows as a form they still have to submit.

  const ISSUE_BASE =
    "https://github.com/bryanroscoe/shield_optimizer/issues/new" +
    `?template=bug_report.yml&title=${encodeURIComponent("Bug: ")}`;
  /// Well under the ~8 KB GitHub accepts in a URL. Past it the form opens with
  /// Diagnostics empty and the dialog asks for a paste instead.
  const PREFILL_LIMIT = 6000;

  /// `diagnostics` is the field id in .github/ISSUE_TEMPLATE/bug_report.yml;
  /// issue forms prefill a field from the query parameter of the same id.
  function issueUrl(bundle: string): { url: string; prefilled: boolean } {
    if (bundle) {
      const url = `${ISSUE_BASE}&diagnostics=${encodeURIComponent(bundle)}`;
      if (url.length <= PREFILL_LIMIT) return { url, prefilled: true };
    }
    return { url: ISSUE_BASE, prefilled: false };
  }

  function openIssue() {
    const { url, prefilled } = issueUrl(bugBusy ? "" : bugBundle);
    if (!prefilled) {
      bugMessage = bugBundle
        ? "The diagnostics are too long to fill in for you. Click Copy, then paste them into the Diagnostics field on GitHub."
        : "Paste the diagnostics into the Diagnostics field on GitHub once they have been collected.";
    }
    void openUrl(url);
  }

  let bugOpen = $state(false);
  let bugBundle = $state("");
  let bugBusy = $state(false);
  let bugMessage = $state("");
  let bugCopied = $state(false);
  let debugLogging = $state(false);
  let logPath = $state("");

  /// The device the user is looking at, if they are looking at one. Its
  /// properties are usually the whole answer to "why won't this open?".
  const currentSerial = $derived.by(() => {
    const match = /^\/devices\/([^/]+)/.exec($page.url.pathname);
    return match ? decodeURIComponent(match[1]) : null;
  });

  async function openBugReport() {
    bugOpen = true;
    bugBusy = true;
    bugCopied = false;
    bugMessage = "";
    bugBundle = "";
    try {
      const [bundle, debug, dir] = await Promise.all([
        api.collectDiagnostics(currentSerial),
        api.getDebugLogging(),
        api.logDirPath(),
      ]);
      bugBundle = bundle;
      debugLogging = debug;
      logPath = dir;
    } catch (e) {
      bugMessage = String(e);
    } finally {
      bugBusy = false;
    }
  }

  async function toggleDebugLogging(e: Event) {
    const wanted = (e.currentTarget as HTMLInputElement).checked;
    try {
      // Believe the backend, not the click: a failed reload must not leave
      // the checkbox claiming a level that isn't running.
      debugLogging = await api.setDebugLogging(wanted);
    } catch (err) {
      bugMessage = String(err);
      debugLogging = await api.getDebugLogging().catch(() => debugLogging);
    }
    setBreadcrumbsEnabled(debugLogging);
  }

  async function copyBugBundle() {
    try {
      await navigator.clipboard.writeText(bugBundle);
      bugCopied = true;
    } catch (e) {
      // Clipboard access can be refused; the textarea is still selectable.
      bugMessage = `Couldn't reach the clipboard (${e}) — select the text above and copy it.`;
    }
  }

  async function openLogsFolder() {
    try {
      await api.openLogDir();
    } catch (e) {
      bugMessage = String(e);
    }
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
      <BrandMark size={22} />
      <span class="title">ATV Optimizer</span>
      {#if currentVersion}
        <button
          class="version"
          onclick={openHistory}
          aria-label={`v${currentVersion} — what's new`}
          data-tip="What's new"
          data-tip-side="bottom"
        >
          v{currentVersion}
        </button>
      {:else}
        <span class="version">v2</span>
      {/if}
      <!-- Not gated on the GitHub API call: the updater is its own source, and
           a slow or failed API read must never hide Restart. -->
      {#if pendingUpdate}
        {#if updateInstalled}
          <button
            class="update-badge installed"
            onclick={restartApp}
            disabled={restarting}
            title="Relaunch to finish updating"
          >
            {#if restartFailed}
              Quit and reopen the app to finish
            {:else}
              Update installed — Restart now <Icon name="restart_alt" size={16} />
            {/if}
          </button>
        {:else if updateBusy}
          <span class="update-badge updating" role="status">{updateProgress}</span>
        {:else}
          {#if updateProgress}
            <span class="update-badge failed" role="status">{updateProgress}</span>
          {/if}
          <button class="update-badge" onclick={openNotes} title="See what changed, then install">
            Update now → v{pendingVersion}
          </button>
        {/if}
      {/if}
      {#if update && rollingOut}
        <!-- Not a button: there is nothing useful to click yet. -->
        <span
          class="update-badge rolling"
          data-tip="The in-app updater will offer it within a few minutes"
          data-tip-side="bottom"
        >
          v{update.latest} rolling out
        </span>
      {/if}
    </div>
    <div class="header-right">
      <nav>
        <a href="/" class:active={$page.url.pathname === "/"}>Devices</a>
        <a href="/snapshots" class:active={$page.url.pathname.startsWith("/snapshots")}>
          Snapshots
        </a>
      </nav>
      <button
        class="bug-btn"
        onclick={openBugReport}
        aria-label="Report a bug"
        data-tip="Report a bug"
        data-tip-side="bottom"
      >
        <Icon name="bug_report" size={18} />
      </button>
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
  <ContextMenu />
  <footer>
    <button class="kofi" onclick={() => openUrl("https://ko-fi.com/bryanroscoe")}>
      <Icon name="local_cafe" size={16} /> Enjoying ATV Optimizer? Support it on Ko-fi
    </button>
  </footer>
</div>

<svelte:window onkeydown={onKeydown} />

{#if notesOpen && pendingUpdate}
  <div class="notes-backdrop" role="presentation" onclick={() => (notesOpen = false)}></div>
  <div class="notes-dialog" role="dialog" aria-modal="true" aria-labelledby="notes-title">
    <h2 id="notes-title">What's new in v{pendingVersion}</h2>
    {#if currentVersion}
      <p class="notes-current muted">You're on v{currentVersion}.</p>
    {/if}
    <div class="notes-body">
      {#if releaseNotes.length === 0}
        <p class="muted">
          This release didn't come with notes. The release history on GitHub has
          the details.
        </p>
      {:else}
        <ReleaseNotes blocks={releaseNotes} />
      {/if}
    </div>
    <div class="notes-actions">
      <button class="notes-history" onclick={() => openUrl(RELEASES_PAGE)}>
        All releases <Icon name="open_in_new" size={14} />
      </button>
      <span class="spacer"></span>
      <button onclick={() => (notesOpen = false)}>Not now</button>
      <button class="primary" onclick={installFromNotes}>Install v{pendingVersion}</button>
    </div>
  </div>
{:else if historyOpen}
  <div class="notes-backdrop" role="presentation" onclick={closeHistory}></div>
  <div class="notes-dialog" role="dialog" aria-modal="true" aria-labelledby="notes-title">
    {#if arrived}
      <h2 id="notes-title">Updated to v{currentVersion}</h2>
      <p class="notes-current muted">Here's what changed.</p>
    {:else}
      <h2 id="notes-title">What's new</h2>
      {#if currentVersion}
        <p class="notes-current muted">You're on v{currentVersion}.</p>
      {/if}
    {/if}
    <div class="notes-body">
      {#if history.length === 0}
        <p class="muted">
          This build doesn't carry its release notes. The release history on GitHub
          has the details.
        </p>
      {:else}
        {#each history as entry, i (entry.version)}
          <details class="release" open={i === 0} data-version={entry.version}>
            <summary>
              <span class="release-version">v{entry.version}</span>
              {#if entry.date}<span class="release-date muted">{entry.date}</span>{/if}
              {#if entry.version === currentVersion}<span class="release-current">Installed</span>{/if}
            </summary>
            {#if entry.blocks.length === 0}
              <p class="muted">No notes for this release.</p>
            {:else}
              <ReleaseNotes blocks={entry.blocks} />
            {/if}
          </details>
        {/each}
      {/if}
    </div>
    <div class="notes-actions">
      <button class="notes-history" onclick={() => openUrl(RELEASES_PAGE)}>
        See all releases on GitHub <Icon name="open_in_new" size={14} />
      </button>
      <span class="spacer"></span>
      <button class="primary" onclick={closeHistory}>{arrived ? "Got it" : "Close"}</button>
    </div>
  </div>
{/if}

{#if restartPromptOpen}
  <div class="notes-backdrop" role="presentation" onclick={() => (restartPromptOpen = false)}></div>
  <div class="notes-dialog restart-dialog" role="dialog" aria-modal="true" aria-labelledby="restart-title">
    <h2 id="restart-title">Update installed</h2>
    {#if restartFailed}
      <p class="notes-current" role="alert">
        The app couldn't restart itself. Quit and reopen the app to finish updating to
        v{pendingVersion}.
      </p>
    {:else}
      <p class="notes-current">
        v{pendingVersion} is ready. Restart now to start using it, or carry on and it
        will start the next time you open the app.
      </p>
    {/if}
    <div class="notes-actions">
      <span class="spacer"></span>
      {#if restartFailed}
        <button class="primary" onclick={() => (restartPromptOpen = false)}>OK</button>
      {:else}
        <button onclick={() => (restartPromptOpen = false)}>Later</button>
        <button class="primary" onclick={restartApp} disabled={restarting}>
          {restarting ? "Restarting…" : "Restart now"}
        </button>
      {/if}
    </div>
  </div>
{/if}

{#if bugOpen}
  <div class="notes-backdrop" role="presentation" onclick={() => (bugOpen = false)}></div>
  <div class="notes-dialog" role="dialog" aria-modal="true" aria-labelledby="bug-title">
    <h2 id="bug-title">Report a bug</h2>
    <p class="notes-current muted">
      The app sends nothing on its own. Open GitHub issue fills the text below into a new
      issue, which you review and submit yourself.
    </p>
    <label class="bug-toggle">
      <input type="checkbox" checked={debugLogging} onchange={toggleDebugLogging} />
      Debug logging
    </label>
    <p class="bug-path muted">
      While it's on, this session is also recorded (adb calls and the buttons you press) to
      <span class="mono">logs/sessions</span>. That recording stays on this computer and is never
      part of this report.
    </p>
    <p class="bug-path muted mono">
      {logPath ? `Logs: ${logPath}` : "Logs: (no log folder)"}
    </p>
    <textarea
      class="bug-bundle mono"
      readonly
      aria-label="Diagnostics"
      value={bugBusy ? "Collecting…" : bugBundle}
    ></textarea>
    {#if bugMessage}
      <p class="bug-message muted">{bugMessage}</p>
    {/if}
    <div class="notes-actions">
      <button onclick={copyBugBundle} disabled={bugBusy || !bugBundle}>
        <Icon name="content_copy" size={14} /> {bugCopied ? "Copied" : "Copy"}
      </button>
      <button onclick={openLogsFolder}>
        <Icon name="folder_open" size={14} /> Open logs folder
      </button>
      <span class="spacer"></span>
      <button onclick={() => (bugOpen = false)}>Close</button>
      <button class="primary" onclick={openIssue}>
        Open GitHub issue <Icon name="open_in_new" size={14} />
      </button>
    </div>
  </div>
{/if}

<style>
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
  .title {
    font-size: 1.05rem;
  }
  .version {
    color: var(--fg-muted);
    font-weight: 500;
    font-size: 0.9rem;
    font-family: var(--mono);
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
    background: var(--scrim);
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
    border-radius: var(--radius-xl);
    box-shadow: var(--shadow-modal);
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
  .notes-body p {
    margin: 0 0 0.4rem;
  }
  .release {
    border-bottom: 1px solid var(--border);
    padding: 0.5rem 0;
  }
  .release:last-child {
    border-bottom: none;
  }
  .release summary {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    cursor: pointer;
    padding: 0.15rem 0 0.4rem;
  }
  .release-version {
    font-weight: 600;
    font-family: var(--mono);
  }
  .release-date {
    font-size: 0.85rem;
  }
  .release-current {
    font-size: 0.75rem;
    color: var(--accent);
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
  /* Same shell as the release-notes dialog above; only the body differs. */
  .bug-toggle {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.88rem;
    cursor: pointer;
  }
  .bug-toggle input {
    accent-color: var(--accent-strong);
    cursor: pointer;
  }
  .bug-path {
    margin: 0;
    font-size: 0.76rem;
    word-break: break-all;
  }
  .bug-bundle {
    flex: 1;
    min-height: 12rem;
    resize: vertical;
    font-size: 0.76rem;
    line-height: 1.45;
    white-space: pre;
    overflow: auto;
  }
  .bug-message {
    margin: 0;
    font-size: 0.8rem;
  }
  .bug-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0.3rem;
    color: var(--fg-secondary);
  }
  .bug-btn:hover {
    color: var(--fg-primary);
  }
  .update-badge {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    margin-left: 0.6rem;
    padding: 0.15rem 0.6rem;
    border: 1px solid var(--accent);
    border-radius: var(--radius-pill);
    background: var(--accent-surface, transparent);
    color: var(--accent);
    font-size: 0.78rem;
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
  }
  .update-badge:hover {
    background: var(--accent-strong);
    color: var(--accent-ink);
  }
  .update-badge.updating {
    cursor: default;
    opacity: 0.8;
  }
  .update-badge.failed {
    cursor: default;
    white-space: normal;
    border-color: var(--danger);
    color: var(--danger);
    background: transparent;
  }
  /* Muted and inert: it is news, not an action. */
  .update-badge.rolling {
    cursor: default;
    border-color: var(--border);
    background: var(--bg-muted);
    color: var(--fg-muted);
    font-weight: 500;
  }
  .update-badge.installed {
    background: var(--accent-strong);
    color: var(--accent-ink);
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
    align-items: center;
    gap: 1.2rem;
  }
  nav a {
    color: var(--fg-secondary);
    font-size: 0.92rem;
    padding: 0.3rem 0.5rem;
    border-radius: var(--radius-sm);
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
    accent-color: var(--accent-strong);
    cursor: pointer;
  }
  .theme-toggle {
    display: flex;
    align-items: stretch;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
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
    color: var(--accent-ink);
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
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
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
