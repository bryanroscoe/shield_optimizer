<script lang="ts">
  import HelpSheet from "../components/HelpSheet.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import PaywallSheet from "../components/PaywallSheet.svelte";
  import { tick, untrack } from "svelte";
  import { api } from "../lib/api";
  import { billing } from "../lib/billing.svelte";
  import { session } from "../lib/session.svelte";
  import { currentTarget, sameTv } from "../lib/tvTarget";
  import { normalizeHardwareId } from "../lib/identity";
  import type { Screen } from "../lib/router.svelte";
  import {
    getShellAcknowledged,
    getShellBookmarks,
    setShellAcknowledged,
    setShellBookmarks,
  } from "../lib/shell";
  import {
    SHELL_PRESETS,
    shellOutputLines,
    shellOutputText,
    shellStopNotice,
    type ShellBookmark,
    type ShellRunResult,
  } from "../../../shared/shell";

  let { navigate, back }: { navigate: (screen: Screen) => void; back: () => void } = $props();

  let command = $state("");
  let running = $state(false);
  let result = $state<ShellRunResult | null>(null);
  let err = $state<string | null>(null);
  let showPaywall = $state(false);
  let copied = $state(false);
  let commandInput = $state<HTMLTextAreaElement>();

  let bookmarks = $state<ShellBookmark[]>(getShellBookmarks());
  let bookmarkLabel = $state("");
  let editingIndex = $state<number | null>(null);
  let editLabel = $state("");
  let editCommand = $state("");
  let bookmarkError = $state<string | null>(null);
  let editNameInput = $state<HTMLInputElement>();
  let editReturnFocus: HTMLButtonElement | null = null;

  // Consent is resolved once per TV and written back on every change; it is
  // never re-read over live state, so a refresh can't clear a fresh tick.
  let acknowledged = $state(false);
  // A run belongs to the TV it started on. Leaving that TV drops its late
  // result, says so, and releases Run for the new one.
  let runToken = 0;
  let leftRunning = $state(false);
  let resolvedFor: string | null = null;
  const hardwareId = $derived(normalizeHardwareId(session.connectedDevice?.properties?.serial_number));
  $effect(() => {
    const identity = `${session.serial}:${session.deviceEpoch}:${hardwareId ?? ""}`;
    if (identity === resolvedFor) return;
    const wasRunning = resolvedFor !== null && untrack(() => running);
    resolvedFor = identity;
    acknowledged = getShellAcknowledged(hardwareId);
    runToken++;
    running = false;
    result = null;
    err = null;
    leftRunning = wasRunning;
  });
  // The phone's ADB runs a command only on a TV with a verified hardware id,
  // so a TV without one can't use Shell and its box stays off.
  const identified = $derived(!!hardwareId);
  // A tick while no TV is live would land on whichever TV the session last
  // knew, so the box waits for a live, identified TV.
  const canAcknowledge = $derived(session.isConnected && identified && !running);

  function setAcknowledged(next: boolean) {
    acknowledged = next;
    setShellAcknowledged(hardwareId, next);
  }

  const canRun = $derived(
    acknowledged && identified && session.isConnected && !!session.serial && !running && !!command.trim(),
  );
  const runHint = $derived(
    !session.isConnected
      ? "Connect a TV to run commands."
      : !identified
        ? "Shell can't run on this TV because it didn't report a hardware ID."
        : !acknowledged
          ? "Check the box above to enable Run."
          : null,
  );
  const outLines = $derived(result ? shellOutputLines(result) : 0);
  const stopNotice = $derived(result ? shellStopNotice(result) : null);

  async function run() {
    const trimmed = command.trim();
    if (!canRun) return;
    const target = currentTarget();
    const token = ++runToken;
    command = trimmed;
    running = true;
    err = null;
    result = null;
    leftRunning = false;
    try {
      const response = await api.runShell(target.serial, trimmed);
      if (token !== runToken || !sameTv(target)) return;
      result = response;
      // Any command can change what the other screens show.
      if (!response.blocked) session.invalidateAll();
    } catch (e) {
      if (token !== runToken || !sameTv(target)) return;
      const message = String(e);
      if (message.includes("LOCKED:")) {
        showPaywall = true;
        return;
      }
      err = message;
      // A lost connection can follow a command that already ran.
      session.invalidateAll();
    } finally {
      if (token === runToken) running = false;
    }
  }

  async function copyOutput() {
    if (!result || result.blocked) return;
    try {
      await navigator.clipboard.writeText(shellOutputText(result));
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      /* clipboard refused: the output is still selectable */
    }
  }

  function clearOutput() {
    result = null;
    err = null;
  }

  async function pick(entry: ShellBookmark) {
    command = entry.command;
    await tick();
    commandInput?.focus();
    commandInput?.scrollIntoView({ block: "center" });
  }

  function persistBookmarks(next: ShellBookmark[]): boolean {
    try {
      setShellBookmarks(next);
      bookmarks = next;
      bookmarkError = null;
      return true;
    } catch {
      bookmarkError = "Couldn't save bookmarks on this phone. Your changes haven't been saved. Try again.";
      return false;
    }
  }

  function saveBookmark() {
    const cmd = command.trim();
    const label = bookmarkLabel.trim() || cmd;
    if (!label || !cmd) return;
    if (bookmarks.some((b) => b.label === label)) {
      bookmarkError = "A custom bookmark already has that name. Edit it or choose another name.";
      return;
    }
    if (persistBookmarks([...bookmarks, { label, command: cmd }])) bookmarkLabel = "";
  }

  async function editBookmark(index: number, trigger: HTMLButtonElement) {
    editingIndex = index;
    editLabel = bookmarks[index].label;
    editCommand = bookmarks[index].command;
    bookmarkError = null;
    editReturnFocus = trigger;
    await tick();
    editNameInput?.focus();
    editNameInput?.scrollIntoView({ block: "center" });
  }

  async function cancelEdit() {
    editingIndex = null;
    editLabel = "";
    editCommand = "";
    bookmarkError = null;
    await tick();
    if (editReturnFocus?.isConnected) editReturnFocus.focus();
    editReturnFocus = null;
  }

  function saveEdit() {
    if (editingIndex === null) return;
    const label = editLabel.trim();
    const cmd = editCommand.trim();
    if (!label || !cmd) {
      bookmarkError = "Enter a bookmark name and command.";
      return;
    }
    if (bookmarks.some((b, index) => index !== editingIndex && b.label === label)) {
      bookmarkError = "A custom bookmark already has that name. Choose another name.";
      return;
    }
    const next = bookmarks.map((b, index) => (index === editingIndex ? { label, command: cmd } : b));
    if (persistBookmarks(next)) void cancelEdit();
  }

  function removeBookmark(index: number) {
    if (persistBookmarks(bookmarks.filter((_, position) => position !== index)) && editingIndex !== null) {
      void cancelEdit();
    }
  }
</script>

<div class="screen shell-screen">
  <div class="topline">
    <div class="header-left">
      <button class="iconbtn" onclick={back} aria-label="Back">
        <span class="msr">arrow_back</span>
      </button>
      <FindRemoteButton />
    </div>
    <h3 class="header-title">Shell</h3>
    <span style="width:44px"></span>
  </div>

  {#if !session.isPro}
    <div class="locked-card">
      <span class="locked-icon msr">terminal</span>
      <h2>Shell is a Pro feature</h2>
      <p class="card-desc">Run any <code>adb shell</code> command on your TV from this phone, with bookmarks for the ones you use.</p>
      <button class="primary" onclick={() => (showPaywall = true)}>
        <span class="msr">bolt</span>Unlock Pro
      </button>
      {#if billing.licenseKeyEntry}
        <button class="ghost-link" onclick={() => navigate("more")}>Already have a key? Enter it in More</button>
      {/if}
    </div>
  {:else}
    <!-- The consent gates Run, so it sits above the command box. -->
    <div class="shell-warning" role="group" aria-label="Expert shell warning">
      <span class="msr" aria-hidden="true">warning</span>
      <div class="warning-body">
        <p class="warning-text">
          Expert mode runs any command via <code>adb shell</code> and can erase data or make the
          TV unusable. It is an exception to the app's protected-app safeguards.
        </p>
        <label class="ack">
          <input
            type="checkbox"
            checked={acknowledged}
            disabled={!canAcknowledge}
            onchange={(e) => setAcknowledged((e.currentTarget as HTMLInputElement).checked)}
          />
          <span>I understand these risks and want to enable expert shell on {session.isConnected ? session.deviceLabel : "this TV"}.</span>
        </label>
        <HelpSheet title="Shell limits">
          <p class="card-desc">A basic check catches some obvious dangerous commands, but shell expressions can bypass it.</p>
          <p class="card-desc">Commands time out after 30 seconds, and output is capped at 256 KiB per stream. Leaving this screen doesn't stop a command that is already running on the TV.</p>
          <p class="card-desc">Your choice is remembered for this TV until you uncheck it. Shell needs a TV that reports a hardware ID.</p>
        </HelpSheet>
      </div>
    </div>

    <div class="shell-card">
      <div class="prompt">
        <span class="sigil" aria-hidden="true">$</span>
        <textarea
          bind:this={commandInput}
          bind:value={command}
          class="mono"
          rows="3"
          placeholder="pm list packages -d --user 0"
          aria-label="Shell command"
          autocapitalize="off"
          autocomplete="off"
          spellcheck="false"
        ></textarea>
      </div>
      <button class="primary" onclick={run} disabled={!canRun}>
        <span class="msr" aria-hidden="true">play_arrow</span>{running ? "Running…" : "Run"}
      </button>
      {#if runHint}<p class="card-desc" role="status">{runHint}</p>{/if}
    </div>

    {#if leftRunning}
      <p class="blocked" role="status">A command was still running on the previous TV. It may have finished there; its output isn't shown here.</p>
    {/if}

    {#if err}
      <p class="error" role="alert">{err}</p>
    {:else if result}
      {#if result.blocked}
        <p class="blocked" role="alert">{result.blocked_reason}</p>
      {:else}
        {#if stopNotice}<p class="blocked" role="alert">{stopNotice}</p>{/if}
        <div class="output-pane">
          {#if result.stdout.trim()}
            <pre class="output mono" aria-label="Command output">{result.stdout}</pre>
          {/if}
          {#if result.stderr.trim()}
            <pre class="output mono stderr" aria-label="Error output">{result.stderr}</pre>
          {/if}
          {#if !result.stdout.trim() && !result.stderr.trim()}
            <p class="card-desc no-output">The command produced no output.</p>
          {/if}
          <div class="output-meta">
            <span class="meta-facts mono">
              {#if result.exit_code === 0}
                <span class="exit-ok">exit 0</span>
              {:else if result.exit_code !== null}
                <span class="exit-bad">exit {result.exit_code}</span>
              {/if}
              <span>{outLines} line{outLines === 1 ? "" : "s"}</span>
            </span>
            <span class="meta-actions">
              <button class="ghost small" onclick={copyOutput}>
                <span class="msr" aria-hidden="true">{copied ? "check" : "content_copy"}</span>{copied ? "Copied" : "Copy"}
              </button>
              <button class="ghost small" onclick={clearOutput}>
                <span class="msr" aria-hidden="true">delete</span>Clear
              </button>
            </span>
          </div>
        </div>
      {/if}
    {/if}

    <section class="shell-card" aria-labelledby="bookmarks-label">
      <span class="card-label" id="bookmarks-label">Bookmarks · {SHELL_PRESETS.length + bookmarks.length}</span>
      <p class="card-desc">Tap one to fill the command. Replace <code>PACKAGE_NAME</code> before uninstalling.</p>
      <ul class="bookmark-list">
        {#each SHELL_PRESETS as preset (preset.label)}
          <li>
            <button class="bookmark-pick" disabled={running} onclick={() => pick(preset)}>
              <span class="bookmark-text">
                <span class="bookmark-name">{preset.label}</span>
                <code class="bookmark-cmd" aria-hidden="true">{preset.command}</code>
              </span>
              <span class="msr go" aria-hidden="true">chevron_right</span>
            </button>
          </li>
        {/each}
        {#each bookmarks as bookmark, index (index)}
          <li>
            <button class="bookmark-pick" disabled={running} onclick={() => pick(bookmark)}>
              <span class="bookmark-text">
                <span class="bookmark-name">{bookmark.label}</span>
                <code class="bookmark-cmd" aria-hidden="true">{bookmark.command}</code>
              </span>
            </button>
            <button
              class="iconbtn row-action"
              aria-label={`Edit the bookmark ${bookmark.label}`}
              disabled={editingIndex !== null}
              onclick={(event) => editBookmark(index, event.currentTarget)}
            ><span class="msr" aria-hidden="true">edit</span></button>
            <button
              class="iconbtn row-action"
              aria-label={`Remove the bookmark ${bookmark.label}`}
              disabled={editingIndex !== null}
              onclick={() => removeBookmark(index)}
            ><span class="msr" aria-hidden="true">close</span></button>
          </li>
        {/each}
      </ul>
      {#if bookmarkError}<p class="error" role="alert">{bookmarkError}</p>{/if}
      {#if editingIndex !== null}
        <form class="bookmark-form" aria-label="Edit custom bookmark" onsubmit={(event) => { event.preventDefault(); saveEdit(); }}>
          <label>Name<input bind:this={editNameInput} bind:value={editLabel} autocapitalize="off" autocomplete="off" spellcheck="false" /></label>
          <label>Command<textarea class="mono" bind:value={editCommand} rows="4" autocapitalize="off" autocomplete="off" spellcheck="false"></textarea></label>
          <div class="form-actions">
            <button type="submit" class="primary small">Save changes</button>
            <button type="button" class="ghost small" onclick={cancelEdit}>Cancel</button>
          </div>
        </form>
      {:else}
        <div class="bookmark-form">
          <label>Bookmark name (optional)<input bind:value={bookmarkLabel} autocapitalize="off" autocomplete="off" spellcheck="false" /></label>
          <button class="ghost small" disabled={!command.trim()} onclick={saveBookmark}>
            <span class="msr" aria-hidden="true">bookmark_add</span>Bookmark this command
          </button>
        </div>
      {/if}
    </section>
  {/if}
</div>

<PaywallSheet open={showPaywall} {navigate} onClose={() => (showPaywall = false)} />

<style>
  .shell-screen {
    gap: 14px;
  }
  .header-left {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .header-title {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  code {
    font-family: var(--mono);
    font-size: 0.92em;
    overflow-wrap: anywhere;
  }
  .mono {
    font-family: var(--mono);
  }
  .card-label {
    font-family: var(--mono);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.12em;
    color: var(--dim);
    text-transform: uppercase;
  }
  .card-desc {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.45;
    margin: 0;
  }
  .locked-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 12px;
    padding: 32px 20px;
    border-radius: 22px;
    background: var(--surface);
    border: 1px solid var(--line);
    margin-top: 20px;
  }
  .locked-icon {
    font-size: 44px;
    color: var(--accent);
  }
  .locked-card h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 700;
  }
  .locked-card .primary {
    max-width: 260px;
  }
  .shell-warning {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 14px 16px;
    border-radius: 16px;
    background: color-mix(in srgb, var(--amber) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--amber) 35%, transparent);
    color: var(--text-soft);
    min-width: 0;
  }
  .shell-warning > .msr {
    font-size: 22px;
    color: var(--amber);
    flex: none;
  }
  .warning-body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .warning-text {
    margin: 0;
    font-size: 13px;
    line-height: 1.45;
  }
  .ack {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    font-size: 13px;
    line-height: 1.45;
    color: var(--text);
    min-height: 44px;
  }
  .ack input {
    flex: none;
    width: 22px;
    height: 22px;
    margin: 1px 0 0;
    accent-color: var(--accent);
  }
  .shell-card {
    padding: 16px;
    border-radius: 18px;
    background: var(--surface);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
  }
  .prompt {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 12px;
    border: 1px solid var(--accent);
    background: var(--canvas);
  }
  .sigil {
    font-family: var(--mono);
    color: var(--accent);
    line-height: 1.5;
  }
  .prompt textarea {
    flex: 1;
    min-width: 0;
    border: none;
    background: none;
    color: var(--text);
    font-size: 14px;
    line-height: 1.5;
    padding: 0;
    resize: vertical;
  }
  .prompt textarea:focus {
    outline: none;
  }
  .prompt:focus-within {
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .blocked {
    margin: 0;
    padding: 12px 14px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--amber) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--amber) 35%, transparent);
    color: var(--text);
    font-size: 13px;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }
  .error {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .output-pane {
    border-radius: 14px;
    border: 1px solid var(--line);
    background: var(--canvas);
    overflow: hidden;
    min-width: 0;
  }
  .output {
    margin: 0;
    padding: 12px;
    max-height: 60vh;
    overflow: auto;
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-soft);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
  }
  .output.stderr {
    color: var(--danger);
    border-top: 1px solid var(--line);
  }
  .no-output {
    padding: 12px;
  }
  .output-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    padding: 8px 12px;
    border-top: 1px solid var(--line);
    background: var(--surface);
  }
  .meta-facts {
    display: flex;
    gap: 12px;
    font-size: 12px;
    color: var(--muted);
  }
  .exit-ok {
    color: var(--teal);
  }
  .exit-bad {
    color: var(--danger);
  }
  .meta-actions {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }
  .meta-actions .ghost,
  .bookmark-form .ghost {
    gap: 6px;
    padding: 0 14px;
  }
  .meta-actions .msr,
  .bookmark-form .ghost .msr {
    font-size: 18px;
  }
  .bookmark-list {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 14px;
    overflow: hidden;
  }
  .bookmark-list li {
    display: flex;
    align-items: center;
    gap: 4px;
    border-bottom: 1px solid var(--line);
    min-width: 0;
  }
  .bookmark-list li:last-child {
    border-bottom: none;
  }
  .bookmark-pick {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    min-height: 52px;
    padding: 8px 12px;
    border: none;
    background: none;
    color: inherit;
    text-align: left;
    font: inherit;
  }
  .bookmark-pick:disabled {
    opacity: 0.6;
  }
  .bookmark-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .bookmark-name {
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
  }
  .bookmark-cmd {
    font-size: 11px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    overflow-wrap: normal;
  }
  .go {
    flex: none;
    color: var(--accent);
    font-size: 20px;
  }
  .row-action {
    flex: none;
  }
  .bookmark-form {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .bookmark-form label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-soft);
    min-width: 0;
  }
  .bookmark-form input,
  .bookmark-form textarea {
    width: 100%;
    box-sizing: border-box;
    padding: 10px 12px;
    border-radius: 11px;
    border: 1px solid var(--line);
    background: var(--canvas);
    color: var(--text);
    font-size: 14px;
  }
  .bookmark-form input {
    min-height: 44px;
    font-family: var(--sans);
  }
  .bookmark-form textarea {
    resize: vertical;
    overflow-wrap: anywhere;
  }
  .form-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .form-actions > button {
    flex: 1 1 140px;
  }
</style>
