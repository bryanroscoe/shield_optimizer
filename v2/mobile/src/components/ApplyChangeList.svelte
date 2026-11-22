<script lang="ts">
  import type { ConfirmSection } from "../lib/optimizeRows";

  let { sections }: { sections: ConfirmSection[] } = $props();
</script>

<div class="change-list">
  {#each sections as section (section.kind)}
    <section class="change-section {section.cls}" aria-label={section.label}>
      <h4><span class="msr" aria-hidden="true">{section.icon}</span>{section.label}</h4>
      <ul>
        {#each section.groups as group, i (i)}
          {#if group.reason}
            <li>
              <span class="change-names">{group.names.join(", ")}</span>
              <span class="change-reason">{group.reason}</span>
            </li>
          {:else}
            {#each group.names as name, j (j)}
              <li><span class="change-names">{name}</span></li>
            {/each}
          {/if}
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
    gap: 12px;
    margin: 4px 0;
    text-align: left;
  }
  .change-section h4 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 6px;
    font-size: 12px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--muted);
  }
  .change-section h4 .msr {
    font-size: 16px;
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
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  li {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-left: 10px;
    border-left: 2px solid var(--line);
  }
  .change-section.caution li {
    border-left-color: color-mix(in srgb, var(--amber) 45%, transparent);
  }
  .change-names {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
    line-height: 1.35;
    overflow-wrap: anywhere;
  }
  .change-reason {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.4;
  }
</style>
