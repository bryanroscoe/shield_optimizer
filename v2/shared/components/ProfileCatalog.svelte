<script lang="ts">
  import data from "../../catalogs/cleanyourtv/catalog.json";
  import licenseNotice from "../../catalogs/cleanyourtv/upstream/LICENSE?raw";
  import {
    matchingProfiles, packageObservations, sourceFormLabel, sourceUrl,
    type ProfileCatalog, type SourceProfile,
  } from "../profile-catalog";

  let { initialPackage = "", openSource }: {
    initialPackage?: string;
    openSource: (url: string) => Promise<void>;
  } = $props();

  const catalog = data as unknown as ProfileCatalog;
  const packageCount = new Set(catalog.profiles.flatMap((profile) => profile.entries.map((entry) => entry.package))).size;
  const sourceFiles = import.meta.glob<string>("../../catalogs/cleanyourtv/upstream/examples/*/debloat.txt", {
    query: "?raw", import: "default", eager: true,
  });
  let deviceQuery = $state("");
  let packageQuery = $state("");
  let selectedId = $state("");
  let sourceLines = $state<string[]>([]);
  let sourceError = $state("");
  let linkError = $state("");
  let profileLimit = $state(40);
  let packageLimit = $state(40);

  const profiles = $derived(matchingProfiles(catalog.profiles, deviceQuery, packageQuery));
  const selected = $derived(catalog.profiles.find((profile) => profile.profile === selectedId));
  const rows = $derived(selected ? packageObservations(selected, packageQuery) : []);
  const comments = $derived(sourceLines.filter((line) => line.trimStart().startsWith("#")).join("\n"));

  function resetSelection() {
    selectedId = "";
    sourceLines = [];
    sourceError = "";
    profileLimit = 40;
  }

  function chooseProfile(profile: SourceProfile) {
    selectedId = profile.profile;
    sourceLines = [];
    sourceError = "";
    packageLimit = 40;
    try {
      const text = sourceFiles[`../../catalogs/cleanyourtv/upstream/${profile.source}`];
      if (typeof text !== "string") throw new Error("The bundled source is unavailable.");
      sourceLines = text.split(/\r?\n/);
    } catch (error) {
      sourceError = String(error);
    }
  }

  async function open(url: string) {
    linkError = "";
    try { await openSource(url); }
    catch (error) { linkError = `Could not open the source. ${error}`; }
  }

  $effect(() => {
    packageQuery = initialPackage;
    resetSelection();
  });
</script>

<section class="profile-catalog" aria-label="Device and package catalog">
  <header class="catalog-heading">
    <div>
      <span class="eyebrow">Research reference</span>
      <h1>Device & package catalog</h1>
      <p class="counts">{catalog.profiles.length} device profiles · {packageCount.toLocaleString()} package IDs</p>
    </div>
    <span class="evidence-status">Unreviewed source</span>
  </header>
  <p class="scope-note">Source profiles have no verified match to your TV or firmware. Removal safety stays with the app’s reviewed checks.</p>
  <details class="catalog-help">
    <summary>How to read this catalog</summary>
    <p>Choose an upstream profile to inspect its original observations. An upstream disable entry records that author’s choice. A commented entry may be kept, optional, or absent. Source notes retain warnings and dependencies. These observations do not change package verdicts or select apps for removal.</p>
    <p>Model, OS and firmware statements belong to the selected source. They have not been independently verified for your device.</p>
  </details>

  <div class="catalog-filters">
    <label>Device or OS
      <input aria-label="Search device profiles" type="search" placeholder="Brand, model or Android version" bind:value={deviceQuery} oninput={resetSelection} />
    </label>
    <label>Package ID
      <input aria-label="Search package IDs" type="search" placeholder="com.example.app" bind:value={packageQuery} oninput={resetSelection} />
    </label>
  </div>

  <div class="catalog-columns">
    <aside class="profile-list" aria-label="Source profiles">
      <h2>{profiles.length} source {profiles.length === 1 ? "profile" : "profiles"}</h2>
      {#if profiles.length === 0}
        <p>No source profiles match these filters.</p>
      {/if}
      {#each profiles.slice(0, profileLimit) as profile (profile.profile)}
        <button class="profile-option" class:selected={selectedId === profile.profile}
          aria-pressed={selectedId === profile.profile} onclick={() => chooseProfile(profile)} data-profile={profile.profile}>
          <strong>{profile.upstream_context.tv}</strong>
          <span>{profile.upstream_context.os}</span>
          <small>{profile.profile}</small>
        </button>
      {/each}
      {#if profiles.length > profileLimit}
        <button class="catalog-button" onclick={() => profileLimit += 40}>Show more profiles</button>
      {/if}
    </aside>

    <div class="profile-content" aria-live="polite">
      {#if !selected}
        <div class="catalog-empty"><h2>Choose a source profile</h2><p>Search a package to find the device profiles that mention it.</p></div>
      {:else}
        <div class="source-heading">
          <span class="eyebrow">Selected source profile</span>
          <h2>{selected.upstream_context.tv}</h2>
          <p>{selected.upstream_context.os} · {selected.upstream_context.brand}</p>
          <span class="evidence-status">TV and firmware applicability unverified</span>
        </div>
        <details class="source-context" data-source-context>
          <summary>Model, firmware & source context</summary>
          <p>{selected.upstream_context.notes}</p>
          {#if selected.upstream_context.readme_os}<p>Additional source OS label · {selected.upstream_context.readme_os}</p>{/if}
          {#if !sourceError}<pre>{comments}</pre>{/if}
        </details>
        <button class="catalog-button source-link" onclick={() => open(sourceUrl(catalog.upstream_commit, selected))}>Open pinned source</button>
        {#if linkError}<p role="alert">{linkError}</p>{/if}
        {#if sourceError}
          <p role="alert">{sourceError}</p>
          <button class="catalog-button" onclick={() => selected && chooseProfile(selected)}>Retry source</button>
        {:else}
          <h3>{rows.length} package {rows.length === 1 ? "identity" : "identities"}</h3>
          {#each rows.slice(0, packageLimit) as row (row.package)}
            <article class="package-source" data-package-source={row.package}>
              <h4>{row.package}</h4>
              <div class="source-forms">
                {#each [...new Set(row.entries.map((entry) => entry.upstream_form))] as form (form)}
                  <span class="form-label">{sourceFormLabel(form)}</span>
                {/each}
              </div>
              <details>
                <summary>Original observations ({row.entries.length})</summary>
                {#each row.entries as entry (entry.line)}
                  <div class="source-observation">
                    <span>{sourceFormLabel(entry.upstream_form)} · line {entry.line}</span>
                    <pre>{sourceLines[entry.line - 1]}</pre>
                    <button class="catalog-link" onclick={() => selected && open(sourceUrl(catalog.upstream_commit, selected, entry.line))}>View source line</button>
                  </div>
                {/each}
              </details>
            </article>
          {/each}
          {#if rows.length > packageLimit}<button class="catalog-button" onclick={() => packageLimit += 40}>Show more packages</button>{/if}
        {/if}
      {/if}
    </div>
  </div>

  <details class="catalog-license">
    <summary>Source attribution & MIT license</summary>
    <p>CleanYourTV source materials · Copyright (c) 2026 Gathin.</p>
    <p>Source revision <code>{catalog.upstream_commit}</code>. ATV Optimizer indexes the original package lines and comments and marks the observations unreviewed. Source text is preserved verbatim.</p>
    <p>The MIT notice below applies to these imported source materials.</p>
    <pre>{licenseNotice}</pre>
  </details>
</section>

<style>
  .profile-catalog { --catalog-surface: var(--bg-surface, var(--surface)); --catalog-inset: var(--bg-inset, var(--canvas)); --catalog-border: var(--border, var(--line)); --catalog-soft: var(--fg-secondary, var(--text-soft)); color: var(--fg-primary, var(--text)); max-width: 1200px; margin: 0 auto; padding: 1.5rem; }
  .catalog-heading { display: flex; align-items: center; justify-content: space-between; gap: 1rem; }
  .eyebrow { color: var(--accent); text-transform: uppercase; letter-spacing: .08em; font-size: .75rem; }
  h1 { margin: .4rem 0; font-size: clamp(1.5rem, 3vw, 2rem); line-height: 1.2; }
  h2 { margin: .35rem 0 .75rem; font-size: 1.1rem; line-height: 1.4; }
  h3 { margin: 1.25rem 0 .75rem; font-size: 1rem; }
  p { line-height: 1.5; }
  .counts, .scope-note, .catalog-help, .catalog-license { color: var(--catalog-soft); }
  .counts { margin: .5rem 0; }
  .scope-note { margin: 1rem 0 .5rem; }
  .evidence-status { display: inline-block; flex-shrink: 0; border: 1px solid var(--catalog-border); border-radius: 6px; padding: .4rem .6rem; font-size: .75rem; line-height: 1.4; }
  details { margin: .75rem 0; }
  summary { cursor: pointer; line-height: 1.5; }
  summary:focus-visible, button:focus-visible, input:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  .catalog-filters { display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; margin: 1.5rem 0; }
  label { display: grid; gap: .5rem; font-size: .9rem; }
  input { width: 100%; min-width: 0; box-sizing: border-box; font: inherit; color: inherit; background: var(--catalog-inset); border: 1px solid var(--catalog-border); border-radius: 8px; padding: .8rem; }
  .catalog-columns { display: grid; grid-template-columns: minmax(230px, 34%) minmax(0, 1fr); gap: 1.25rem; align-items: start; }
  .profile-list { min-width: 0; max-height: 70vh; overflow-y: auto; padding: .25rem; }
  .profile-option { width: 100%; text-align: left; display: grid; gap: .35rem; background: var(--catalog-surface); color: inherit; border: 1px solid var(--catalog-border); border-radius: 8px; padding: .9rem; margin-bottom: .6rem; font: inherit; cursor: pointer; }
  .profile-option.selected { border-color: var(--accent); }
  .profile-option strong { font-size: .9rem; line-height: 1.45; overflow-wrap: anywhere; }
  .profile-option span, .profile-option small { color: var(--catalog-soft); font-size: .8rem; overflow-wrap: anywhere; }
  .profile-content { min-width: 0; border: 1px solid var(--catalog-border); border-radius: 10px; padding: 1.25rem; background: var(--catalog-surface); }
  .catalog-empty { padding: 2rem .25rem; color: var(--catalog-soft); }
  .source-heading p { margin: .5rem 0 .75rem; color: var(--catalog-soft); }
  .source-context { border-bottom: 1px solid var(--catalog-border); padding-bottom: .5rem; }
  pre { white-space: pre-wrap; overflow-wrap: anywhere; font: .8rem/1.6 "Geist Mono", monospace; margin: .65rem 0; }
  code { overflow-wrap: anywhere; }
  .catalog-button { font: inherit; color: inherit; background: var(--catalog-inset); border: 1px solid var(--catalog-border); padding: .65rem .85rem; border-radius: 7px; cursor: pointer; }
  .catalog-link { color: var(--accent); font: inherit; font-size: .8rem; background: none; border: 0; padding: .35rem 0; cursor: pointer; text-decoration: underline; }
  .package-source { border-top: 1px solid var(--catalog-border); padding: 1rem 0 .3rem; }
  h4 { font: .9rem/1.5 "Geist Mono", monospace; margin: 0 0 .5rem; overflow-wrap: anywhere; }
  .source-forms { display: flex; flex-wrap: wrap; gap: .4rem; }
  .form-label { font-size: .75rem; color: var(--catalog-soft); padding: .2rem .45rem; border: 1px solid var(--catalog-border); border-radius: 4px; }
  .package-source summary { font-size: .85rem; color: var(--catalog-soft); }
  .source-observation { border-left: 2px solid var(--catalog-border); padding: .3rem 0 .3rem .75rem; margin: .8rem 0; }
  .source-observation > span { color: var(--catalog-soft); font-size: .75rem; }
  .catalog-license { margin-top: 1.5rem; border-top: 1px solid var(--catalog-border); padding-top: 1rem; }
  @media (max-width: 700px) {
    .profile-catalog { padding: 1rem; }
    .catalog-heading { display: block; }
    .catalog-heading > .evidence-status { margin-top: .5rem; }
    .catalog-filters { grid-template-columns: 1fr; }
    .catalog-columns { grid-template-columns: 1fr; }
    .profile-list { max-height: 270px; }
    .profile-content { padding: 1rem; }
  }
</style>
