<script lang="ts">
  import type { Snippet } from "svelte";

  let { label, children }: { label: string; children: Snippet } = $props();

  let details: HTMLDetailsElement;

  function closeOnEscape(event: KeyboardEvent) {
    if (event.key !== "Escape" || !details?.open || !details.contains(document.activeElement)) return;
    event.stopPropagation();
    details.open = false;
    details.querySelector("summary")?.focus();
  }
</script>

<svelte:window onkeydown={closeOnEscape} />

<details class="help-details" bind:this={details}>
  <summary>{label}</summary>
  <div class="help-content">{@render children()}</div>
</details>

<style>
  .help-details { margin: 0.65rem 0; color: var(--fg-secondary); font-size: 0.82rem; }
  summary { width: fit-content; max-width: 100%; cursor: pointer; line-height: 1.5; }
  summary:focus-visible { outline: 2px solid var(--accent); outline-offset: 4px; border-radius: 2px; }
  .help-content { max-width: 70ch; margin-top: 0.5rem; line-height: 1.5; overflow-wrap: anywhere; }
  .help-content :global(p) { margin: 0.45rem 0; }
</style>
