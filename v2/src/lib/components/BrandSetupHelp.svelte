<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import {
    brandGuide,
    brandList,
    genericGuide,
    SOURCE_NAME,
    SOURCE_URL,
    type ConnectMethod,
  } from "../../../shared/connection-setup";
  import licenseText from "../../../catalogs/connection-setup/LICENSE?raw";
  import { chooseConnectionBrand, connectionBrand } from "$lib/connection-brand.svelte";

  // The menu path to Developer options and the debugging switch for the TV's
  // brand. Lives inside the existing help disclosures, never on the page.
  const id = $props.id();
  const brands = brandList();
  let brand = $derived(connectionBrand.id);
  let model = $state(0);
  let guide = $derived(brandGuide(brand));
  // Another disclosure picking a brand starts this one at its first model too.
  $effect(() => {
    void brand;
    model = 0;
  });
  let variant = $derived(guide ? guide.variants[Math.min(model, guide.variants.length - 1)] : genericGuide());

  function chooseBrand(value: string) {
    chooseConnectionBrand(value);
  }

  const NEXT: Record<ConnectMethod, string> = {
    port: "Then enter the TV's address in Add by IP. Port 5555 is used when you leave it out.",
    pair: "Then use Pair PIN with the address and code the TV shows.",
    either: "Then try Add by IP. If the TV refuses, turn on Wireless debugging and use Pair PIN.",
  };

  async function open(url: string, event: MouseEvent) {
    event.preventDefault();
    try {
      await openUrl(url);
    } catch {
      window.open(url, "_blank", "noopener");
    }
  }
</script>

<div class="brand-setup" data-brand-setup>
  <div class="pickers">
    <label for="{id}-brand">TV brand</label>
    <select id="{id}-brand" value={brand} onchange={(e) => chooseBrand(e.currentTarget.value)}>
      <option value="">Other or not sure</option>
      {#each brands as b (b.id)}
        <option value={b.id}>{b.name}</option>
      {/each}
    </select>
  </div>
  {#if guide && guide.variants.length > 1}
    <fieldset class="models-pick">
      <legend>Model</legend>
      {#each guide.variants as v, i (v.models)}
        <label><input type="radio" name="{id}-model" checked={model === i} onchange={() => (model = i)} /> {v.models}</label>
      {/each}
    </fieldset>
  {/if}
  {#if guide && guide.variants.length === 1}
    <p class="models">{variant.models}</p>
  {/if}
  <ol class="setup-steps" data-setup-steps>
    {#each variant.steps as step (step)}
      <li>{step}</li>
    {/each}
    <li data-connect={variant.connect}>{NEXT[variant.connect]}</li>
  </ol>
  {#each variant.notes as note (note)}
    <p class="setup-note">{note}</p>
  {/each}
  <p class="attribution">
    From owners' notes in <a href={SOURCE_URL} onclick={(e) => open(SOURCE_URL, e)}>{SOURCE_NAME}</a> (MIT).
    {#if guide}Menus can differ by firmware.{/if}
  </p>
  <details class="license">
    <summary>License</summary>
    <pre>{licenseText}</pre>
  </details>
</div>

<style>
  .brand-setup { margin: 0.5rem 0; }
  .pickers {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 0.4rem 0.6rem;
    align-items: center;
    max-width: 34rem;
  }
  .pickers select { min-width: 0; max-width: 100%; font: inherit; }
  .models { margin: 0.4rem 0 0; color: var(--fg-secondary); }
  .models-pick { margin: 0.5rem 0 0; padding: 0; border: 0; }
  .models-pick legend { padding: 0; margin-bottom: 0.2rem; }
  .models-pick label { display: flex; align-items: baseline; gap: 0.4rem; margin: 0.2rem 0; cursor: pointer; }
  .models-pick input { flex: none; accent-color: var(--accent-strong); }
  .setup-steps { margin: 0.5rem 0; padding-left: 1.3rem; }
  .setup-steps li { margin: 0.25rem 0; }
  .setup-note { margin: 0.35rem 0; }
  .attribution { color: var(--fg-muted); font-size: 0.78rem; }
  .attribution a { color: var(--accent); }
  .license { color: var(--fg-muted); font-size: 0.78rem; }
  .license summary { cursor: pointer; width: fit-content; }
  .license pre { white-space: pre-wrap; overflow-wrap: anywhere; max-height: 14rem; overflow-y: auto; font-size: 0.72rem; }
</style>
