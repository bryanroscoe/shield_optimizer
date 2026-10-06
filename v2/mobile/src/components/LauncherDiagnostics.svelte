<script lang="ts">
  // The per-stage record from a failed launcher action: every command the
  // attempt ran and what the TV replied. Offered as a copy, not read on screen;
  // if the clipboard is refused the text is shown so it can still be selected.
  let { lines }: { lines: string[] } = $props();

  let copied = $state(false);
  let shown = $state(false);

  $effect(() => {
    void lines;
    copied = false;
    shown = false;
  });

  async function copy() {
    try {
      await navigator.clipboard.writeText(lines.join("\n"));
      copied = true;
    } catch {
      shown = true;
    }
  }
</script>

{#if lines.length > 0}
  <div class="diag">
    <button class="diag-btn" onclick={copy}>
      <span class="msr">content_copy</span>{copied ? "Copied" : "Copy diagnostic details"}
    </button>
    <p class="diag-note">
      Every command this attempt ran and what your TV replied. Paste it into a GitHub issue; it is
      what makes a launcher report fixable.
    </p>
    {#if shown}
      <pre class="diag-text" aria-label="Diagnostic details">{lines.join("\n")}</pre>
    {/if}
  </div>
{/if}

<style>
  .diag {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .diag-btn {
    align-self: flex-start;
    min-height: 40px;
    padding: 0 12px;
    border: 1px solid var(--line);
    border-radius: 11px;
    background: var(--surface-2);
    color: var(--text);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 600;
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }
  .diag-btn .msr {
    font-size: 16px;
  }
  .diag-note {
    margin: 0;
    font-size: 11px;
    line-height: 1.45;
    color: var(--muted);
  }
  .diag-text {
    margin: 0;
    max-height: 200px;
    overflow: auto;
    padding: 10px;
    border-radius: 10px;
    background: var(--canvas);
    border: 1px solid var(--line);
    font-family: var(--mono);
    font-size: 10px;
    line-height: 1.4;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
  }
</style>
