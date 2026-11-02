<script lang="ts">
  import { THEMES, chooseTheme, uiPrefs } from "$lib/ui-prefs.svelte";
</script>

<div class="theme-toggle" role="group" aria-label="Theme">
  {#each THEMES as t (t.id)}
    <button
      class:active={uiPrefs.theme === t.id}
      title={t.title}
      aria-pressed={uiPrefs.theme === t.id}
      onclick={() => chooseTheme(t.id)}
    >
      {t.label}
    </button>
  {/each}
</div>

<style>
  .theme-toggle {
    display: flex;
    align-items: stretch;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .theme-toggle button {
    border: none;
    border-radius: 0;
    padding: 0.3rem 0.6rem;
    font-size: 0.8rem;
    background: transparent;
    color: var(--fg-secondary);
  }
  .theme-toggle button:not(:last-child) {
    border-right: 1px solid var(--border);
  }
  .theme-toggle button.active {
    background: var(--accent-strong);
    color: var(--accent-ink);
  }
  .theme-toggle button:hover:not(.active) {
    background: var(--bg-button-hover);
  }
</style>
