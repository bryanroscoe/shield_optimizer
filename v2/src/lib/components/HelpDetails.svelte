<script lang="ts">
  import { onMount, type Snippet } from "svelte";

  let { label, children, open = $bindable(false) }: { label: string; children: Snippet; open?: boolean } = $props();

  let details: HTMLDetailsElement;

  // A native listener on the element, not the window (nor Svelte's delegated
  // `onkeydown`, which runs at the root), so an enclosing dialog's Escape
  // handler sees it already handled and stays open.
  function closeOnEscape(event: KeyboardEvent) {
    if (event.key !== "Escape" || !details?.open || !details.contains(document.activeElement)) return;
    event.preventDefault();
    event.stopPropagation();
    details.open = false;
    details.querySelector("summary")?.focus();
  }

  onMount(() => {
    details.addEventListener("keydown", closeOnEscape);
    return () => details.removeEventListener("keydown", closeOnEscape);
  });
</script>

<details class="help-details" bind:this={details} bind:open>
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
