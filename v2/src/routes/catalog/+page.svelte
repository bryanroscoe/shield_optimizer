<script lang="ts">
  import { page } from "$app/state";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import ProfileCatalog from "../../../shared/components/ProfileCatalog.svelte";
</script>

{#key page.url.searchParams.get("package")}
  <ProfileCatalog initialPackage={page.url.searchParams.get("package") ?? ""} openSource={openUrl} />
{/key}
