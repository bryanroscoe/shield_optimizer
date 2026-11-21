<script lang="ts">
  import type { Snippet } from "svelte";
  // Detail that belongs to one row: a toggle that unfurls it in place, so a row
  // shows its name, status and controls and keeps its explanation one tap away.
  // Screen-wide help goes in HelpSheet. A consequence needed before acting stays
  // on the row or in its confirmation, never only in here.
  let {
    label,
    context = "",
    open,
    onToggle,
    tone = "neutral",
    children,
  }: {
    label: string;
    /// Added to the accessible name after the visible label (the app's name),
    /// so every row's toggle has its own name.
    context?: string;
    open: boolean;
    onToggle: () => void;
    tone?: "neutral" | "caution" | "blocked";
    children: Snippet;
  } = $props();
  const id = $props.id();
</script>

<button
  type="button"
  class="unfurl-toggle {tone}"
  aria-expanded={open}
  aria-controls="{id}-panel"
  aria-label={context ? `${label}, ${context}` : undefined}
  onclick={onToggle}
>
  <span class="unfurl-label">{label}</span>
  <span class="msr unfurl-chevron" class:open aria-hidden="true">expand_more</span>
</button>
<div id="{id}-panel" class="unfurl-panel {tone}" hidden={!open}>
  {@render children()}
</div>

<style>
  .unfurl-toggle {
    box-sizing: border-box;
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-height: 44px;
    max-width: 100%;
    margin: 0;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text-soft);
    font-family: var(--sans);
    font-size: 0.75rem;
    font-weight: 600;
    line-height: 1.3;
    text-align: left;
    cursor: pointer;
  }
  .unfurl-toggle.caution {
    color: var(--amber);
  }
  .unfurl-toggle.blocked {
    color: var(--danger);
  }
  .unfurl-toggle:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
    border-radius: 4px;
  }
  .unfurl-label {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .unfurl-chevron {
    flex: none;
    font-size: 1.25em;
    transition: transform 0.15s ease;
  }
  .unfurl-chevron.open {
    transform: rotate(180deg);
  }
  .unfurl-panel {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 2px 0 2px 10px;
    border-left: 2px solid var(--line);
    color: var(--text-soft);
    font-size: 0.8125rem;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }
  .unfurl-panel.caution {
    border-left-color: color-mix(in srgb, var(--amber) 45%, transparent);
  }
  .unfurl-panel.blocked {
    border-left-color: color-mix(in srgb, var(--danger) 45%, transparent);
  }
  .unfurl-panel[hidden] {
    display: none;
  }
  .unfurl-panel :global(p) {
    margin: 0;
  }
  @media (prefers-reduced-motion: reduce) {
    .unfurl-chevron {
      transition: none;
    }
  }
</style>
