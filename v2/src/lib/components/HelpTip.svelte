<script lang="ts">
  import type { Snippet } from "svelte";

  let { label, forId, children }: { label: string; forId?: string; children: Snippet } = $props();
  const helpId = $props.id();
  let expanded = $state(false);
  let trigger: HTMLButtonElement;
  let content: HTMLDivElement;

  function closeOnEscape(event: KeyboardEvent) {
    if (event.key !== "Escape" || !expanded ||
        (document.activeElement !== trigger && !content?.contains(document.activeElement))) return;
    event.stopPropagation();
    expanded = false;
    trigger?.focus();
  }
</script>

<svelte:window onkeydown={closeOnEscape} />

<div class="help-title">
  {#if forId}
    <label for={forId}>{label}</label>
  {:else}
    <span>{label}</span>
  {/if}
  <button
    type="button"
    class="help-trigger"
    aria-label={`About ${label}`}
    aria-expanded={expanded}
    aria-controls={helpId}
    onclick={() => (expanded = !expanded)}
    bind:this={trigger}
  >?</button>
</div>
<div id={helpId} class="help-content" hidden={!expanded} bind:this={content}>
  {@render children()}
</div>

<style>
  .help-title { display: flex; align-items: center; gap: 0.5rem; font-size: 0.92rem; font-weight: 550; }
  label { cursor: pointer; }
  .help-trigger { flex: 0 0 1.65rem; width: 1.65rem; height: 1.65rem; padding: 0; display: inline-flex; align-items: center; justify-content: center; border: 1px solid var(--border-strong); border-radius: 50%; background: transparent; color: var(--fg-secondary); font-size: 0.9rem; font-weight: 600; cursor: pointer; }
  .help-trigger:hover { color: var(--fg); border-color: var(--accent); }
  .help-trigger:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  .help-content { margin-top: 0.6rem; max-width: 62ch; font-size: 0.9rem; font-weight: 400; line-height: 1.55; color: var(--fg-secondary); overflow-wrap: anywhere; }
  .help-content :global(p) { margin: 0.5rem 0; }
</style>
