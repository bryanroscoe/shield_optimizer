<script lang="ts">
  import type { Snippet } from "svelte";

  // A label with a question-mark control that opens its explanation in a small floating
  // panel. The panel is a non-modal disclosure: it opens on click, tap or Enter/Space,
  // and closes on the trigger again, its close button, Escape, a press outside it, or
  // focus leaving it. Nothing is inserted into the page flow, so opening one doesn't
  // shift the rows beneath it.
  let { label, forId, children }: { label: string; forId?: string; children: Snippet } = $props();
  const helpId = $props.id();
  let expanded = $state(false);
  let trigger: HTMLButtonElement;
  let panel: HTMLDivElement;
  let top = $state(0);
  let left = $state(0);
  let maxHeight = $state(0);

  const GAP = 8;
  const MARGIN = 12;

  // Fixed positioning keeps the panel clear of any clipping container; it is placed
  // below the trigger (above when there is more room there) and kept inside the window.
  function place() {
    if (!expanded || !trigger || !panel) return;
    const t = trigger.getBoundingClientRect();
    const vw = document.documentElement.clientWidth;
    const vh = window.innerHeight;
    const below = vh - t.bottom - GAP - MARGIN;
    const above = t.top - GAP - MARGIN;
    const natural = panel.scrollHeight;
    const goBelow = natural <= below || below >= above;
    maxHeight = Math.max(80, goBelow ? below : above);
    const height = Math.min(natural, maxHeight);
    top = goBelow ? t.bottom + GAP : t.top - GAP - height;
    const width = panel.offsetWidth;
    left = Math.min(Math.max(MARGIN, t.left - 10), Math.max(MARGIN, vw - MARGIN - width));
  }

  function close(returnFocus: boolean) {
    if (!expanded) return;
    expanded = false;
    if (returnFocus) trigger?.focus();
  }

  $effect(() => {
    if (!expanded) return;
    place();
    // A second pass once the content has laid out at its final width.
    const frame = requestAnimationFrame(place);
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  });

  function onWindowKeydown(event: KeyboardEvent) {
    if (event.key !== "Escape" || !expanded) return;
    event.stopPropagation();
    const inside = document.activeElement === trigger || panel?.contains(document.activeElement);
    close(inside);
  }

  function onWindowPointerdown(event: PointerEvent) {
    if (!expanded) return;
    const target = event.target as Node | null;
    if (target && (trigger?.contains(target) || panel?.contains(target))) return;
    close(false);
  }

  function onFocusOut(event: FocusEvent) {
    const next = event.relatedTarget as Node | null;
    // Null means focus left the window or went to a non-focusable spot; the pointerdown
    // handler covers clicks, so only close when focus moved somewhere definite.
    if (!next || next === trigger || panel?.contains(next)) return;
    close(false);
  }
</script>

<svelte:window onkeydown={onWindowKeydown} onpointerdown={onWindowPointerdown} />

<div class="help-title">
  {#if forId}
    <label for={forId}>{label}</label>
  {:else}
    <span>{label}</span>
  {/if}
  <button
    type="button"
    class="help-trigger"
    class:open={expanded}
    aria-label={`About ${label}`}
    aria-expanded={expanded}
    aria-controls={helpId}
    onclick={() => (expanded = !expanded)}
    onfocusout={onFocusOut}
    bind:this={trigger}
  >?</button>
</div>
<div
  id={helpId}
  class="help-popover"
  role="region"
  aria-label={label}
  hidden={!expanded}
  style:top="{top}px"
  style:left="{left}px"
  style:max-height={maxHeight ? `${maxHeight}px` : undefined}
  onfocusout={onFocusOut}
  bind:this={panel}
>
  <button type="button" class="help-close" aria-label="Close" onclick={() => close(true)}>
    <span aria-hidden="true">×</span>
  </button>
  <div class="help-content">
    {@render children()}
  </div>
</div>

<style>
  .help-title { display: flex; align-items: center; gap: 0.5rem; font-size: 0.92rem; font-weight: 550; }
  label { cursor: pointer; }
  .help-trigger { flex: 0 0 1.65rem; width: 1.65rem; height: 1.65rem; padding: 0; display: inline-flex; align-items: center; justify-content: center; border: 1px solid var(--border-strong); border-radius: 50%; background: transparent; color: var(--fg-secondary); font-size: 0.9rem; font-weight: 600; cursor: pointer; }
  .help-trigger:hover,
  .help-trigger.open { color: var(--fg-primary); border-color: var(--accent); }
  .help-trigger.open { background: var(--accent-surface); }
  .help-trigger:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }

  .help-popover {
    position: fixed;
    z-index: 30;
    box-sizing: border-box;
    width: max-content;
    max-width: min(22rem, calc(100vw - 24px));
    overflow-y: auto;
    padding: 0.7rem 2.2rem 0.7rem 0.85rem;
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    background: var(--bg-surface-2);
    box-shadow: var(--shadow-modal);
    font-size: 0.9rem;
    font-weight: 400;
    line-height: 1.5;
    color: var(--fg-secondary);
    text-align: left;
    overflow-wrap: anywhere;
  }
  .help-popover[hidden] { display: none; }
  .help-content :global(p) { margin: 0.5rem 0; }
  .help-content :global(p:first-child) { margin-top: 0; }
  .help-content :global(p:last-child) { margin-bottom: 0; }
  .help-close {
    position: absolute;
    top: 0.35rem;
    right: 0.35rem;
    width: 1.6rem;
    height: 1.6rem;
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--fg-muted);
    font-size: 1.1rem;
    line-height: 1;
    cursor: pointer;
  }
  .help-close:hover { color: var(--fg-primary); background: var(--bg-button-hover); }
  .help-close:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
</style>
