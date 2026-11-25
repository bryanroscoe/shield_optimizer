<script lang="ts">
  import {
    brandGuide,
    brandList,
    genericGuide,
    savedBrand,
    saveBrand,
    SOURCE_NAME,
    type ConnectMethod,
  } from "../../../shared/connection-setup";
  import licenseText from "../../../catalogs/connection-setup/LICENSE?raw";

  /// The menu path to Developer options and the debugging switch for the TV's
  /// brand. Shown inside the "How to turn on debugging" help sheet.
  const id = $props.id();
  const brands = brandList();
  let brand = $state(readSaved());
  let model = $state(0);
  let guide = $derived(brandGuide(brand));
  let variant = $derived(guide ? guide.variants[Math.min(model, guide.variants.length - 1)] : genericGuide());

  function readSaved(): string {
    try {
      return savedBrand(localStorage) ?? "";
    } catch {
      return "";
    }
  }

  function chooseBrand(value: string) {
    brand = value;
    model = 0;
    try {
      saveBrand(localStorage, value || null);
    } catch {
      /* storage unavailable: the pick lasts for this screen */
    }
  }

  const NEXT: Record<ConnectMethod, string> = {
    port: "Then scan, or choose Enter IP address manually and Connect (no code) on port 5555.",
    pair: "Then scan and choose Pair with code, or enter the address and pairing port the TV shows.",
    either: "Then scan. If Connect (no code) is refused, turn on Wireless debugging and use Pair with code.",
  };
</script>

<div class="brand-setup" data-brand-setup>
  <label class="pick" for="{id}-brand">
    TV brand
    <select id="{id}-brand" value={brand} onchange={(e) => chooseBrand(e.currentTarget.value)}>
      <option value="">Other or not sure</option>
      {#each brands as b (b.id)}
        <option value={b.id}>{b.name}</option>
      {/each}
    </select>
  </label>
  {#if guide && guide.variants.length > 1}
    <fieldset class="models-pick">
      <legend>Model</legend>
      {#each guide.variants as v, i (v.models)}
        <label><input type="radio" name="{id}-model" checked={model === i} onchange={() => (model = i)} /> {v.models}</label>
      {/each}
    </fieldset>
  {:else if guide}
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
  <p class="attribution">From owners' notes in {SOURCE_NAME} (MIT).{#if guide} Menus can differ by firmware.{/if}</p>
  <details class="license">
    <summary>License</summary>
    <pre>{licenseText}</pre>
  </details>
</div>

<style>
  .brand-setup {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .pick {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--text-soft);
    font-size: 0.8125rem;
  }
  .pick select {
    box-sizing: border-box;
    width: 100%;
    min-height: 44px;
    font: inherit;
    font-size: 0.9375rem;
    color: var(--text);
  }
  .models {
    margin: 0;
    color: var(--text-soft);
  }
  .models-pick {
    margin: 0;
    padding: 0;
    border: 0;
    display: flex;
    flex-direction: column;
  }
  .models-pick legend {
    padding: 0;
    margin-bottom: 2px;
    color: var(--text-soft);
    font-size: 0.8125rem;
  }
  .models-pick label {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    color: var(--text);
    font-size: 0.875rem;
    line-height: 1.4;
    overflow-wrap: break-word;
  }
  .models-pick input {
    flex: none;
    width: 20px;
    height: 20px;
    accent-color: var(--accent);
  }
  .setup-steps {
    margin: 0;
    padding-left: 1.3em;
    display: flex;
    flex-direction: column;
    gap: 6px;
    color: var(--text);
  }
  .setup-note {
    margin: 0;
  }
  .attribution {
    margin: 0;
    font-size: 0.75rem;
  }
  .license summary {
    min-height: 44px;
    display: flex;
    align-items: center;
    cursor: pointer;
    font-size: 0.8125rem;
  }
  .license pre {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-size: 0.6875rem;
    color: var(--text-soft);
  }
</style>
