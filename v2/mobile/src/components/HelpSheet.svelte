<script lang="ts">
  import type { Snippet } from "svelte";
  import { modal } from "../lib/modal";
  // Routine help behind a "?" link: tapping it opens a short bottom sheet instead of
  // expanding text into the screen. Closes on Done, a tap above the sheet, or Escape;
  // focus returns to the link (lib/modal.ts). Risks and consequences stay on the screen.
  let { title, children }: { title: string; children: Snippet } = $props();
  const id = $props.id();
  let open = $state(false);
  const close = () => (open = false);
</script>

<button
  type="button"
  class="help-link"
  aria-haspopup="dialog"
  aria-expanded={open}
  onclick={() => (open = true)}
>
  <span class="help-mark" aria-hidden="true">?</span>
  <span class="help-label">{title}</span>
</button>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="help-overlay" use:modal={{ onClose: close }} onclick={close}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div
      class="help-sheet"
      role="dialog"
      aria-modal="true"
      aria-labelledby="{id}-title"
      tabindex="-1"
      onclick={(event) => event.stopPropagation()}
    >
      <!-- Text scrolls; Done stays pinned, so focusing it never scrolls the title away. -->
      <div class="help-scroll">
        <h3 id="{id}-title">{title}</h3>
        <div class="help-body">{@render children()}</div>
      </div>
      <button type="button" class="ghost help-done" onclick={close}>Done</button>
    </div>
  </div>
{/if}

<style>
  /* Closed, the link takes the same single 44px row the inline disclosure did. */
  .help-link {
    box-sizing: border-box;
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-height: 44px;
    max-width: 100%;
    margin: 0;
    padding: 10px 0;
    border: 0;
    background: transparent;
    color: var(--text-soft);
    font-family: var(--sans);
    font-size: 0.8125rem;
    font-weight: 500;
    line-height: 1.4;
    text-align: left;
    cursor: pointer;
  }
  .help-link:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 3px;
    border-radius: 4px;
  }
  .help-mark {
    flex: none;
    display: inline-grid;
    place-items: center;
    width: 1.55em;
    height: 1.55em;
    border: 1px solid color-mix(in srgb, var(--text-soft) 45%, transparent);
    border-radius: 50%;
    font-size: 0.85em;
    font-weight: 600;
    line-height: 1;
  }
  .help-link:active .help-mark,
  .help-link[aria-expanded="true"] .help-mark {
    border-color: var(--accent);
    color: var(--accent);
  }
  .help-label {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .help-overlay {
    position: fixed;
    inset: 0;
    z-index: 500;
    display: flex;
    align-items: flex-end;
    background: rgba(0, 0, 0, 0.6);
    overscroll-behavior: contain;
  }
  .help-sheet {
    box-sizing: border-box;
    width: 100%;
    max-width: var(--content-max);
    max-height: 85vh;
    margin-inline: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow: hidden;
    padding: 22px max(24px, env(safe-area-inset-right)) calc(env(safe-area-inset-bottom) + 20px)
      max(24px, env(safe-area-inset-left));
    background: var(--surface-2);
    border-top: 1px solid var(--line);
    border-radius: 22px 22px 0 0;
    animation: helpSheetUp 0.22s ease-out;
  }
  .help-sheet:focus {
    outline: none;
  }
  .help-scroll {
    flex: 0 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow-y: auto;
    overscroll-behavior-y: contain;
  }
  h3 {
    margin: 0;
    font-size: 1.0625rem;
    font-weight: 700;
    color: var(--text);
    overflow-wrap: anywhere;
  }
  .help-body {
    color: var(--text-soft);
    font-size: 0.875rem;
    line-height: 1.55;
    overflow-wrap: anywhere;
  }
  .help-body :global(:is(p, span)) {
    font-size: inherit;
    line-height: inherit;
    color: inherit;
  }
  .help-body :global(p) {
    margin: 0 0 10px;
  }
  .help-body :global(p:last-child) {
    margin-bottom: 0;
  }
  .help-done {
    flex: none;
    width: 100%;
    min-height: 48px;
    margin-top: 6px;
    font-size: 0.9375rem;
  }
  @keyframes helpSheetUp {
    from {
      transform: translateY(100%);
    }
    to {
      transform: translateY(0);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .help-sheet {
      animation: none;
    }
  }
</style>
