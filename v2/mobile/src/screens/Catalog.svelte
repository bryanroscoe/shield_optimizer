<script lang="ts">
  import { api } from "../lib/api";
  import ProfileCatalog from "../../../shared/components/ProfileCatalog.svelte";

  let { back, initialPackage = "" }: { back: () => void; initialPackage?: string } = $props();
</script>

<div class="catalog-screen">
  <button class="back-button" onclick={back}>← Back</button>
  <ProfileCatalog {initialPackage} openSource={api.openUrl} />
</div>

<style>
  .catalog-screen { padding: max(env(safe-area-inset-top), .75rem) 0 max(env(safe-area-inset-bottom), 1rem); }
  .back-button { color: var(--text); font: inherit; background: var(--surface); border: 1px solid var(--line); border-radius: 8px; margin-left: 1rem; padding: .65rem .9rem; }
</style>
