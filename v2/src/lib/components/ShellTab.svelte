<script lang="ts">
  import { api } from "$lib/api";
  import Icon from "$lib/components/Icon.svelte";
  import { getShellBookmarks, setShellBookmarks, type ShellBookmark } from "$lib/prefs";
  import type { ShellRunResult } from "$lib/types";
  import { onDestroy } from "svelte";

  let { serial, acknowledged = false, onacknowledge, onexecuted }: {
    serial: string;
    acknowledged?: boolean;
    /// The consent is remembered per TV by the page, not by this component —
    /// it is the one that knows the hardware id.
    onacknowledge?: (next: boolean) => void;
    onexecuted?: () => void;
  } = $props();
  let alive = true;
  onDestroy(() => { alive = false; });

  let command = $state("");
  let running = $state(false);
  let result = $state<ShellRunResult | null>(null);
  let err = $state<string | null>(null);
  let bookmarks = $state<ShellBookmark[]>(getShellBookmarks());
  let bookmarkLabel = $state("");

  let outputCopied = $state(false);

  async function copyOutput() {
    if (!result || result.blocked) return;
    const text = [result.stdout, result.stderr].filter((p) => p.trim()).join("\n");
    try {
      await navigator.clipboard.writeText(text);
      outputCopied = true;
      setTimeout(() => (outputCopied = false), 1500);
    } catch {
      /* clipboard blocked — the output is still selectable in the pane */
    }
  }

  function clearOutput() {
    result = null;
    err = null;
  }

  /// Line counts for the output footer the board carries under the pane.
  const outLines = $derived(
    result && !result.blocked
      ? (result.stdout.trim() ? result.stdout.trimEnd().split("\n").length : 0) +
        (result.stderr.trim() ? result.stderr.trimEnd().split("\n").length : 0)
      : 0,
  );

  const PRESETS: ShellBookmark[] = [
    { label: "Disabled packages", command: "pm list packages -d" },
    { label: "Third-party packages", command: "pm list packages -3" },
    { label: "Current launcher", command: "cmd shortcut get-default-launcher" },
    { label: "Device properties", command: "getprop | grep ro.product" },
    { label: "Display state", command: "dumpsys display | grep -E 'mBaseDisplayInfo|modeId'" },
    { label: "Running processes", command: "ps -A -o NAME,PID,RSS --sort=-RSS | head -20" },
    { label: "Uptime", command: "uptime" },
  ];

  async function run() {
    const trimmed = command.trim();
    if (!trimmed || running || !acknowledged) return;
    const target = serial;
    command = trimmed;
    running = true;
    err = null;
    result = null;
    try {
      const response = await api.runShell(target, trimmed);
      if (!alive || serial !== target) return;
      result = response;
      if (!response.blocked) onexecuted?.();
    } catch (e) {
      if (!alive || serial !== target) return;
      err = String(e);
      // A lost connection can follow a partially executed mutation.
      onexecuted?.();
    } finally {
      if (alive && serial === target) running = false;
    }
  }

  function saveBookmark() {
    const label = bookmarkLabel.trim() || command.trim();
    const cmd = command.trim();
    if (!label || !cmd) return;
    // Same label overwrites rather than accumulating near-duplicates.
    const next = [...bookmarks.filter((b) => b.label !== label), { label, command: cmd }];
    bookmarks = next;
    setShellBookmarks(next);
    bookmarkLabel = "";
  }

  function removeBookmark(label: string) {
    const next = bookmarks.filter((b) => b.label !== label);
    bookmarks = next;
    setShellBookmarks(next);
  }

  // Ctrl/Cmd+Enter runs, so a multi-line command can still be submitted from
  // the keyboard without the Enter key being unusable for newlines.
  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      run();
    }
  }
</script>

<div class="card" role="tabpanel" tabindex={0} id="tabpanel-shell" aria-labelledby="tab-shell">
  <div class="card-header">
    <div class="header-title">
      <h2><Icon name="terminal" size={17} /> Shell</h2>
      <p class="muted small mono header-sub">adb -s {serial} shell</p>
    </div>
    <span class="muted small header-note">Ctrl/⌘+Enter runs</span>
  </div>

  <!-- The board puts its warning at the foot of the screen, but ours carries
       the consent that gates Run, so it stays above the input: a disabled Run
       button with its reason further down the page is a dead end. -->
  <div class="callout callout-warn shell-warning">
    <Icon name="warning" size={16} />
    <div>
      <p class="shell-warning-text">
        Expert mode runs arbitrary commands via <code>adb shell</code> and can erase data
        or make the device unusable. This is an exception to the app's protected-package
        safeguards. A basic check catches some obvious dangerous commands, but shell
        expressions can bypass it. Output is limited to 256 KiB per stream and execution
        to 30 seconds; stopping local ADB does not guarantee remote work has stopped.
      </p>
      <label class="small shell-ack">
        <input
          type="checkbox"
          checked={acknowledged}
          disabled={running}
          onchange={(e) => onacknowledge?.((e.currentTarget as HTMLInputElement).checked)}
        />
        I understand these risks and want to enable expert shell on this TV.
        Remembered for this TV until you untick it.
      </label>
    </div>
  </div>

  <div class="shell-layout">
    <div class="shell-main">
      <div class="shell-run">
        <div class="shell-prompt">
          <span class="prompt-sigil" aria-hidden="true">$</span>
          <textarea
            class="shell-input mono"
            bind:value={command}
            onkeydown={onKeydown}
            rows="2"
            spellcheck="false"
            placeholder="pm list packages -d"
            aria-label="Shell command"
          ></textarea>
        </div>
        <button
          class="primary run-btn"
          onclick={() => run()}
          disabled={!acknowledged || running || !command.trim()}
          title={!acknowledged
            ? "Tick the box above to enable expert shell on this TV"
            : !command.trim()
              ? "Type a command first"
              : "Run this command on the TV"}
        >
          <Icon name="play_arrow" size={16} fill /> {running ? "Running…" : "Run"}
        </button>
      </div>
      {#if !acknowledged}
        <p class="muted small run-hint">
          Run is disabled until you tick the box above.
        </p>
      {/if}

      {#if err}
        <p class="error">{err}</p>
      {:else if result}
        {#if result.blocked}
          <p class="blocked">{result.blocked_reason}</p>
        {:else}
          {#if result.termination !== "completed"}
            <p class="blocked">{result.termination === "timeout" ? "Stopped after 30 seconds." : "Stopped at the output limit."} Partial output is shown below. The command may already have changed the device.</p>
          {/if}
          <div class="shell-pane">
            {#if result.stdout.trim()}
              <pre class="output">{result.stdout}</pre>
            {/if}
            {#if result.stderr.trim()}
              <pre class="output stderr">{result.stderr}</pre>
            {/if}
            {#if !result.stdout.trim() && !result.stderr.trim()}
              <p class="muted small no-output">The command produced no output.</p>
            {/if}
            <!-- The board's status strip: what it exited with, which stream
                 spoke, and how much came back. -->
            <div class="shell-meta mono">
              {#if result.exit_code !== null && result.exit_code !== 0}
                <span class="exit-bad">exit {result.exit_code}</span>
              {:else if result.exit_code === 0}
                <span class="exit-ok">exit 0</span>
              {/if}
              {#if result.stderr.trim()}<span class="muted">stderr</span>{/if}
              <span class="muted">{outLines} line{outLines === 1 ? "" : "s"}</span>
              <!-- On the output, not in the card header: these act on what is
                   in the pane, so they belong at its edge. -->
              <span class="meta-actions">
                <button class="small-action subtle" onclick={copyOutput}>
                  <Icon name={outputCopied ? "check" : "content_copy"} size={14} />
                  {outputCopied ? "Copied" : "Copy"}
                </button>
                <button class="small-action subtle" onclick={clearOutput}>
                  <Icon name="delete" size={14} /> Clear
                </button>
              </span>
            </div>
          </div>
        {/if}
      {/if}
    </div>

    <aside class="shell-rail">
      <p class="rail-label">Bookmarks · {bookmarks.length + PRESETS.length}</p>
      <div class="rail-list">
        <!-- The whole row is the button, and its accessible name is the
             label alone — the command is `aria-hidden` so a preset is still
             addressable as "Uptime" rather than "Uptime uptime". -->
        {#each PRESETS as p (p.label)}
          <div class="rail-item">
            <button
              class="rail-pick"
              disabled={running}
              onclick={() => { command = p.command; }}
              title={p.command}
            >
              <span class="rail-text">
                <span class="rail-name">{p.label}</span>
                <code class="rail-cmd" aria-hidden="true">{p.command}</code>
              </span>
              <span class="rail-go" aria-hidden="true"><Icon name="chevron_right" size={16} /></span>
            </button>
          </div>
        {/each}
        {#each bookmarks as b (b.label)}
          <div class="rail-item">
            <button
              class="rail-pick"
              disabled={running}
              onclick={() => { command = b.command; }}
              title={b.command}
            >
              <span class="rail-text">
                <span class="rail-name">{b.label}</span>
                <code class="rail-cmd" aria-hidden="true">{b.command}</code>
              </span>
              <span class="rail-go" aria-hidden="true"><Icon name="chevron_right" size={16} /></span>
            </button>
            <button
              class="rail-run rail-remove"
              title="Remove bookmark"
              aria-label={`Remove the bookmark ${b.label}`}
              onclick={() => removeBookmark(b.label)}
            ><Icon name="close" size={15} /></button>
          </div>
        {/each}
      </div>
      <div class="bookmark-bar">
        <input
          bind:value={bookmarkLabel}
          placeholder="Bookmark name (optional)"
          aria-label="Bookmark name"
        />
        <button class="small-action" disabled={!command.trim()} onclick={saveBookmark}>
          Bookmark current command
        </button>
      </div>
    </aside>
  </div>
</div>

<style>
  /* Shared scoped utilities duplicated from the page; global rules
     (.muted, button, input) live in the layout and are inherited. */
  .small {
    font-size: 0.82rem;
  }
  .mono {
    font-family: var(--mono);
  }
  .error {
    background: var(--danger-surface);
    color: var(--danger-text);
    padding: 0.7rem 1rem;
    border-radius: var(--radius-md);
    font-family: var(--mono);
    font-size: 0.85rem;
  }
  .small-action {
    padding: 0.2rem 0.6rem;
    font-size: 0.78rem;
  }
  code {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    padding: 0.1rem 0.4rem;
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 0.85em;
  }

  /* Shell-tab–specific styles. */
  .shell-input {
    width: 100%;
    resize: vertical;
    padding: 0.5rem 0.6rem;
    background: var(--bg-inset);
    color: var(--fg-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    font-size: 0.88rem;
    line-height: 1.5;
  }
  .header-title {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    min-width: 0;
  }
  .header-title h2 {
    margin: 0;
  }
  .header-sub {
    margin: 0;
  }
  .shell-warning {
    align-items: flex-start;
    margin-bottom: 1rem;
  }
  .shell-warning-text {
    margin: 0 0 0.5rem;
    font-size: 0.85rem;
    line-height: 1.5;
  }
  .shell-ack {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  /* Command and output on the left, the library of commands on the right —
     board 11.11. Presets and bookmarks were a wrapped row of buttons above the
     output, which pushed the thing you had just run below the fold. */
  .shell-layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 22rem);
    gap: 1.25rem;
    align-items: start;
  }
  .shell-main {
    min-width: 0;
  }
  .shell-run {
    display: flex;
    gap: 0.6rem;
    align-items: stretch;
  }
  .shell-prompt {
    display: flex;
    flex: 1;
    min-width: 0;
    align-items: flex-start;
    gap: 0.5rem;
    padding: 0.55rem 0.7rem;
    border: 1px solid var(--accent);
    border-radius: var(--radius-lg);
    background: var(--bg-inset);
  }
  .prompt-sigil {
    font-family: var(--mono);
    color: var(--accent);
    line-height: 1.5;
  }
  .shell-prompt .shell-input {
    flex: 1;
    min-width: 0;
    border: none;
    background: none;
    padding: 0;
    resize: vertical;
  }
  .shell-prompt .shell-input:focus {
    outline: none;
  }
  /* Aligned to the top of the input rather than stretched down its side: a
     resizable textarea made the button grow into a lime slab. */
  .run-btn {
    flex: none;
    align-self: flex-start;
    padding-inline: 1.4rem;
    padding-block: 0.7rem;
  }
  .run-hint {
    margin: 0.4rem 0 0;
  }
  .header-note {
    font-family: var(--mono);
  }
  .meta-actions {
    display: flex;
    gap: 0.4rem;
    margin-left: auto;
  }
  .shell-pane {
    margin-top: 0.8rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg-inset);
    overflow: hidden;
  }
  .shell-pane .output {
    margin: 0;
    border: none;
    border-radius: 0;
    background: none;
  }
  .no-output {
    margin: 0;
    padding: 0.8rem 1rem;
  }
  .shell-meta {
    display: flex;
    align-items: center;
    gap: 0.8rem;
    padding: 0.5rem 1rem;
    border-top: 1px solid var(--border);
    font-size: 0.75rem;
  }
  .exit-ok {
    color: var(--ok);
  }
  .shell-rail {
    min-width: 0;
  }
  .rail-label {
    margin: 0 0 0.5rem;
    font-family: var(--mono);
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--fg-muted);
  }
  .rail-list {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }
  .rail-item {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.55rem 0.7rem;
    background: var(--bg-surface-2);
    border-bottom: 1px solid var(--border);
  }
  .rail-item:last-child {
    border-bottom: none;
  }
  .rail-pick {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 0.4rem;
    min-width: 0;
    padding: 0;
    border: none;
    background: none;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }
  .rail-pick:hover:not(:disabled) {
    background: none;
  }
  .rail-go {
    flex: none;
    display: inline-flex;
    color: var(--accent);
  }
  .rail-item:hover {
    background: var(--bg-button-hover);
  }
  .rail-text {
    flex: 1;
    min-width: 0;
  }
  .rail-name {
    display: block;
    font-size: 0.85rem;
    font-weight: 600;
  }
  .rail-cmd {
    display: block;
    font-size: 0.72rem;
    color: var(--fg-muted);
    background: none;
    padding: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rail-run {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0.25rem;
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--accent);
    cursor: pointer;
  }
  .rail-run:hover:not(:disabled) {
    background: var(--bg-button-hover);
  }
  .rail-remove {
    color: var(--fg-muted);
  }
  .bookmark-bar {
    margin-top: 0.6rem;
    display: flex;
    gap: 0.5rem;
    align-items: center;
    flex-wrap: wrap;
  }
  .bookmark-bar input {
    flex: 1;
    min-width: 180px;
    max-width: 320px;
  }
  /* A refusal is not an error — it is the safety layer doing its job, so it
     reads as a warning rather than a failure. */
  .blocked {
    background: var(--warn-surface);
    border: 1px solid var(--warn-border);
    color: var(--fg-primary);
    padding: 0.7rem 1rem;
    border-radius: var(--radius-md);
    font-size: 0.88rem;
    line-height: 1.5;
  }
  .output {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 0.6rem 0.8rem;
    max-height: 420px;
    overflow: auto;
    font-family: var(--mono);
    font-size: 0.82rem;
    line-height: 1.45;
    white-space: pre;
    margin: 0 0 0.5rem;
  }
  .output.stderr {
    color: var(--danger-text);
  }
  .exit-bad {
    margin-left: 0.5rem;
    font-size: 0.78rem;
    color: var(--danger-text);
  }
</style>
