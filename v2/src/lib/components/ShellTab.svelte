<script lang="ts">
  import { api } from "$lib/api";
  import Icon from "$lib/components/Icon.svelte";
  import { getShellBookmarks, setShellBookmarks, type ShellBookmark } from "$lib/prefs";
  import type { ShellRunResult } from "$lib/types";
  import { onDestroy } from "svelte";

  let { serial, acknowledged = $bindable(false), onexecuted }: { serial: string; acknowledged?: boolean; onexecuted?: () => void } = $props();
  let alive = true;
  onDestroy(() => { alive = false; });

  let command = $state("");
  let running = $state(false);
  let result = $state<ShellRunResult | null>(null);
  let err = $state<string | null>(null);
  let bookmarks = $state<ShellBookmark[]>(getShellBookmarks());
  let bookmarkLabel = $state("");

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
    <h2><Icon name="terminal" size={17} /> Shell</h2>
    <button onclick={() => run()} disabled={!acknowledged || running || !command.trim()}>
      {running ? "Running…" : "Run"}
    </button>
  </div>
  <p class="muted small">
    Expert mode runs arbitrary commands via <code>adb shell</code> and can erase data
    or make the device unusable. This is an exception to the app's protected-package
    safeguards. A basic check catches some obvious dangerous commands, but shell
    expressions can bypass it. Output is limited to 256 KiB per stream and execution
    to 30 seconds; stopping local ADB does not guarantee remote work has stopped.
  </p>
  <label class="small"><input type="checkbox" bind:checked={acknowledged} disabled={running} />
    I understand these risks and want to enable expert shell for this device session.
  </label>

  <textarea
    class="shell-input mono"
    bind:value={command}
    onkeydown={onKeydown}
    rows="3"
    spellcheck="false"
    placeholder="pm list packages -d"
    aria-label="Shell command"
  ></textarea>

  <div class="presets">
    {#each PRESETS as p (p.label)}
      <button class="small-action" disabled={running} onclick={() => { command = p.command; }}>
        {p.label}
      </button>
    {/each}
  </div>

  <div class="bookmark-bar">
    <input
      bind:value={bookmarkLabel}
      placeholder="Bookmark name (optional)"
      aria-label="Bookmark name"
    />
    <button class="small-action" disabled={!command.trim()} onclick={saveBookmark}>
      Save bookmark
    </button>
  </div>

  {#if bookmarks.length}
    <h3>Bookmarks</h3>
    <div class="bookmarks">
      {#each bookmarks as b (b.label)}
        <div class="bookmark">
          <button class="small-action" disabled={running} onclick={() => { command = b.command; }}>
            {b.label}
          </button>
          <code class="bookmark-cmd">{b.command}</code>
          <button
            class="small-action"
            title="Remove bookmark"
            onclick={() => removeBookmark(b.label)}>×</button
          >
        </div>
      {/each}
    </div>
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
      <h3>
        Output
        {#if result.exit_code !== null && result.exit_code !== 0}
          <span class="exit-bad">exit {result.exit_code}</span>
        {/if}
      </h3>
      {#if result.stdout.trim()}
        <pre class="output">{result.stdout}</pre>
      {/if}
      {#if result.stderr.trim()}
        <pre class="output stderr">{result.stderr}</pre>
      {/if}
      {#if !result.stdout.trim() && !result.stderr.trim()}
        <p class="muted small">The command produced no output.</p>
      {/if}
    {/if}
  {/if}
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
  .presets,
  .bookmarks {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin: 0.6rem 0;
  }
  .bookmarks {
    flex-direction: column;
    gap: 0.35rem;
  }
  .bookmark {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .bookmark-cmd {
    flex: 1;
    overflow-x: auto;
    white-space: nowrap;
  }
  .bookmark-bar {
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
