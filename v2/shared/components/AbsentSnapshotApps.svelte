<script lang="ts">
  let { packages }: { packages: string[] } = $props();
</script>

{#if packages.length > 0}
  <div class="absent-apps" data-plan-item="absent-skipped">
    <p>Recorded absent apps are left unchanged because saved-TV hardware identity is unverified.</p>
    <details>
      <summary>Recorded absent apps ({packages.length})</summary>
      <ul>{#each packages as pkg (pkg)}<li>{pkg}</li>{/each}</ul>
    </details>
  </div>
{/if}

<style>
  .absent-apps { margin: 0.75rem 0; }
  p { margin: 0 0 0.4rem; }
  summary { cursor: pointer; }
  ul { overflow-wrap: anywhere; padding-left: 1.4rem; }
</style>
