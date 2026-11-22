<script lang="ts">
  import { SvelteSet } from "svelte/reactivity";
  import type { ConfirmSection } from "../lib/optimizeRows";

  // The Apply confirmation's list: one line per change under its tier, each
  // folding its safety reason one tap away, as the row it came from does.
  let { sections }: { sections: ConfirmSection[] } = $props();
  const id = $props.id();
  const open = new SvelteSet<string>();

  function toggle(key: string) {
    if (open.has(key)) open.delete(key);
    else open.add(key);
  }
</script>

<div class="change-list">
  {#each sections as section (section.kind)}
    <section class="change-section {section.cls}" aria-labelledby="{id}-{section.kind}">
      <h4 id="{id}-{section.kind}"><span class="msr" aria-hidden="true">{section.icon}</span>{section.label}</h4>
      <ul>
        {#each section.changes as change, i (i)}
          {@const key = `${section.kind}-${i}`}
          <li>
            {#if change.reason}
              <button
                type="button"
                class="change-toggle"
                aria-expanded={open.has(key)}
                aria-controls="{id}-{key}"
                onclick={() => toggle(key)}
              >
                <span class="change-label">{change.label}</span>
                <span class="msr chevron" class:open={open.has(key)} aria-hidden="true">expand_more</span>
              </button>
              <p class="change-reason" id="{id}-{key}" hidden={!open.has(key)}>{change.reason}</p>
            {:else}
              <span class="change-label static">{change.label}</span>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {/each}
</div>

<style>
  .change-list {
    align-self: stretch;
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 4px 0;
    text-align: left;
  }
  .change-section h4 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 2px;
    font-size: 0.75rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
  }
  .change-section h4 .msr {
    font-size: 1rem;
  }
  .change-section.caution h4 {
    color: var(--amber);
  }
  .change-section.safe h4 {
    color: var(--teal);
  }
  .change-section.blocked h4 {
    color: var(--danger);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    position: relative;
    padding-left: 14px;
  }
  li::before {
    content: "";
    position: absolute;
    left: 3px;
    top: 19px;
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--muted);
  }
  .change-section.caution li::before {
    background: var(--amber);
  }
  .change-toggle {
    box-sizing: border-box;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    width: 100%;
    min-height: 40px;
    margin: 0;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text);
    font-family: var(--sans);
    text-align: left;
    cursor: pointer;
  }
  .change-toggle:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
    border-radius: 4px;
  }
  .change-label {
    min-width: 0;
    font-size: 0.8125rem;
    font-weight: 600;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }
  .change-label.static {
    display: flex;
    align-items: center;
    min-height: 40px;
  }
  .chevron {
    flex: none;
    font-size: 1.25rem;
    color: var(--muted);
    transition: transform 0.15s ease;
  }
  .chevron.open {
    transform: rotate(180deg);
  }
  .change-reason {
    margin: 0 0 8px;
    font-size: 0.75rem;
    color: var(--muted);
    line-height: 1.45;
  }
  .change-reason[hidden] {
    display: none;
  }
  @media (prefers-reduced-motion: reduce) {
    .chevron {
      transition: none;
    }
  }
</style>
