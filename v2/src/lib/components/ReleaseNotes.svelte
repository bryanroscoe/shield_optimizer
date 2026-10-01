<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import type { NoteBlock } from "$lib/release-notes";

  // Blocks come from `parseReleaseNotes`, which returns data rather than
  // markup. Everything below is rendered through the template, so remote text
  // cannot become HTML.
  let { blocks }: { blocks: NoteBlock[] } = $props();
</script>

<div class="notes-blocks">
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
</div>

<style>
  .notes-blocks {
    line-height: 1.5;
  }
  h3 {
    font-size: 0.82rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--fg-muted);
    margin: 1rem 0 0.35rem;
  }
  h3:first-child {
    margin-top: 0;
  }
  p {
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
  code {
    font-family: var(--mono);
    font-size: 0.85em;
    background: var(--bg-muted);
    padding: 0.05rem 0.3rem;
    border-radius: var(--radius-xs);
  }
  /* A button, not an anchor: these open in the system browser via the opener
     plugin, and the href is remote text we only partly trust. */
  .notes-link {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: var(--accent);
    text-decoration: underline;
    cursor: pointer;
  }
</style>
