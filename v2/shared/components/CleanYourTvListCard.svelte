<script lang="ts">
  import type { ProfileCatalog, SourceProfile } from "../profile-catalog";
  import { candidateProfiles, LIST_OFF, matchReasonText, type TvIdentity } from "../cleanyourtv-lists";

  let {
    catalog,
    loadError = "",
    tv,
    choice,
    rowCount,
    busy = false,
    mode,
    onChoose,
    openSource,
  }: {
    /// Null while the bundled catalog loads.
    catalog: ProfileCatalog | null;
    loadError?: string;
    tv: TvIdentity;
    /// Profile id, `LIST_OFF`, or null when the person hasn't said.
    choice: string | null;
    /// Rows the chosen list added to the plan, or null while they load.
    rowCount: number | null;
    busy?: boolean;
    mode: "optimize" | "restore";
    onChoose: (choice: string | null) => void;
    openSource: (url: string) => Promise<void>;
  } = $props();

  const candidates = $derived(catalog ? candidateProfiles(catalog.profiles, tv) : []);
  const chosen = $derived(
    catalog && choice && choice !== LIST_OFF ? catalog.profiles.find((p) => p.profile === choice) ?? null : null,
  );
  let choosing = $state(false);
  let query = $state("");
  let picked = $state("");
  let linkError = $state("");
  /// Every word typed has to appear in the list's TV, OS, brand or name.
  const searchResults = $derived.by(() => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    if (!catalog || words.length === 0) return [];
    return catalog.profiles
      .filter((profile) => {
        const c = profile.upstream_context;
        const haystack = `${profile.profile} ${c.tv} ${c.os} ${c.brand}`.toLowerCase();
        return words.every((word) => haystack.includes(word));
      })
      .slice(0, 20);
  });
  const options = $derived<SourceProfile[]>(
    query.trim() ? searchResults : candidates.map((c) => c.profile),
  );

  function startChoosing() {
    choosing = true;
    query = "";
    picked = chosen?.profile ?? candidates[0]?.profile.profile ?? "";
  }

  function confirm() {
    if (!picked) return;
    choosing = false;
    onChoose(picked);
  }

  async function open(url: string) {
    linkError = "";
    try {
      await openSource(url);
    } catch (error) {
      linkError = `Couldn't open the list. ${error}`;
    }
  }

  function sourceLink(profile: SourceProfile): string {
    return `https://github.com/Gathin23/cleanyourtv/blob/${catalog?.upstream_commit}/${profile.source}`;
  }

  function reasonFor(profile: SourceProfile): string {
    const candidate = candidates.find((c) => c.profile.profile === profile.profile);
    return candidate ? matchReasonText(candidate, tv) : profile.upstream_context.os;
  }
</script>

<section class="cytv" aria-label="CleanYourTV list" data-cleanyourtv-card>
  <div class="cytv-head">
    <span class="cytv-title">CleanYourTV list</span>
    <details class="cytv-help">
      <summary aria-label="About CleanYourTV lists">?</summary>
      <div class="cytv-help-body">
        <p>CleanYourTV is a community project where TV owners share the apps they turned off on their exact model.</p>
        <p>With your TV's list chosen, {mode === "optimize" ? "Optimize also lists" : "Restore also lists"} the apps on it that ATV Optimizer's own catalog has no record of. None is selected for you, they can only be disabled, never uninstalled, and the do-not-disable list still refuses anything on it.</p>
        <p>Each app's note is the list author's, not reviewed by us. The lists are a pinned copy, used under the MIT license.</p>
      </div>
    </details>
  </div>

  {#if loadError}
    <p class="cytv-line" role="alert">Couldn't load the lists. {loadError}</p>
  {:else if !catalog}
    <p class="cytv-line muted">Loading lists…</p>
  {:else if choosing}
    <fieldset class="cytv-choose">
      <legend class="cytv-line">Which list is your TV's?</legend>
      <input
        class="cytv-search"
        type="search"
        aria-label="Search CleanYourTV lists"
        placeholder="Search brand or model"
        bind:value={query}
      />
      {#if options.length === 0}
        <p class="cytv-line muted">{query.trim() ? "No list matches that search." : "No list names this TV. Search by brand or model."}</p>
      {/if}
      <div class="cytv-options">
        {#each options as profile (profile.profile)}
          <label class="cytv-option" class:selected={picked === profile.profile}>
            <input type="radio" name="cytv-profile" value={profile.profile} bind:group={picked} />
            <span class="cytv-option-text">
              <strong>{profile.upstream_context.tv}</strong>
              <small>{reasonFor(profile)}</small>
            </span>
          </label>
        {/each}
      </div>
      <div class="cytv-actions">
        <button class="cytv-button primary" disabled={!picked || busy} onclick={confirm}>Use this list</button>
        <button class="cytv-button" onclick={() => (choosing = false)}>Cancel</button>
      </div>
    </fieldset>
  {:else if chosen}
    <p class="cytv-line">
      <span>
        {#if rowCount === null}
          Reading <strong>{chosen.upstream_context.tv}</strong>…
        {:else}
          {rowCount === 0 ? "No more apps" : `${rowCount} more ${rowCount === 1 ? "app" : "apps"}`} from <strong>{chosen.upstream_context.tv}</strong>{rowCount === 0 ? " to show." : ", listed below the catalog."}
        {/if}
      </span>
    </p>
    <div class="cytv-actions">
      <button class="cytv-button" disabled={busy} onclick={startChoosing}>Change</button>
      <button class="cytv-button" onclick={() => void open(sourceLink(chosen))}>View list</button>
      <button class="cytv-button" disabled={busy} onclick={() => onChoose(LIST_OFF)}>Turn off</button>
    </div>
  {:else if choice === LIST_OFF}
    <div class="cytv-row">
      <p class="cytv-line muted">Off for this TV.</p>
      <button class="cytv-button" disabled={busy} onclick={startChoosing}>Choose a list</button>
    </div>
  {:else if candidates.length > 0}
    <p class="cytv-line">
      {candidates.length === 1 ? "A list" : `${candidates.length} lists`} on CleanYourTV {candidates.length === 1 ? "names" : "name"} this TV:
      <strong>{candidates[0].profile.upstream_context.tv}</strong>{candidates.length > 1 ? " and others" : ""}.
    </p>
    <div class="cytv-actions">
      {#if candidates.length === 1}
        <button class="cytv-button primary" disabled={busy} onclick={() => onChoose(candidates[0].profile.profile)}>Use this list</button>
      {/if}
      <button class="cytv-button" class:primary={candidates.length > 1} disabled={busy} onclick={startChoosing}>
        {candidates.length === 1 ? "Choose another" : "Choose a list"}
      </button>
      <button class="cytv-button" disabled={busy} onclick={() => onChoose(LIST_OFF)}>Not now</button>
    </div>
  {:else}
    <div class="cytv-row">
      <p class="cytv-line muted">No list names this TV.</p>
      <button class="cytv-button" disabled={busy} onclick={startChoosing}>Choose a list</button>
    </div>
  {/if}
  {#if linkError}<p class="cytv-line" role="alert">{linkError}</p>{/if}
</section>

<style>
  .cytv {
    --cytv-border: var(--border, var(--line));
    --cytv-soft: var(--fg-secondary, var(--text-soft));
    --cytv-inset: var(--bg-inset, var(--canvas));
    border: 1px solid var(--cytv-border);
    border-radius: 10px;
    padding: 0.75rem 0.9rem;
    margin: 0.75rem 0;
    display: grid;
    gap: 0.55rem;
    min-width: 0;
  }
  .cytv-head { display: flex; align-items: center; gap: 0.5rem; }
  .cytv-title { font-weight: 600; font-size: 0.9rem; }
  .cytv-help { position: relative; }
  .cytv-help summary {
    list-style: none;
    width: 1.5rem;
    height: 1.5rem;
    display: grid;
    place-items: center;
    border: 1px solid var(--cytv-border);
    border-radius: 50%;
    cursor: pointer;
    font-size: 0.8rem;
    color: var(--cytv-soft);
  }
  .cytv-help summary::-webkit-details-marker { display: none; }
  .cytv-help-body { color: var(--cytv-soft); font-size: 0.85rem; line-height: 1.5; max-width: 60ch; }
  .cytv-help[open] { flex-basis: 100%; }
  .cytv-head:has(.cytv-help[open]) { flex-wrap: wrap; }
  .cytv-line { margin: 0; line-height: 1.5; overflow-wrap: anywhere; }
  .muted { color: var(--cytv-soft); }
  .cytv-row { display: flex; align-items: center; justify-content: space-between; gap: 0.75rem; flex-wrap: wrap; }
  .cytv-actions { display: flex; flex-wrap: wrap; gap: 0.5rem; }
  .cytv-button {
    font: inherit;
    font-size: 0.85rem;
    color: inherit;
    background: var(--cytv-inset);
    border: 1px solid var(--cytv-border);
    border-radius: 8px;
    padding: 0.45rem 0.8rem;
    min-height: 2.25rem;
    cursor: pointer;
  }
  .cytv-button.primary { background: var(--accent-strong, var(--accent)); color: var(--accent-ink); border-color: var(--accent-strong, var(--accent)); font-weight: 600; }
  .cytv-button:disabled { opacity: 0.55; cursor: default; }
  .cytv-choose { border: 0; margin: 0; padding: 0; display: grid; gap: 0.55rem; min-width: 0; }
  .cytv-search {
    font: inherit;
    color: inherit;
    background: var(--cytv-inset);
    border: 1px solid var(--cytv-border);
    border-radius: 8px;
    padding: 0.55rem 0.7rem;
    min-width: 0;
  }
  .cytv-options { display: grid; gap: 0.4rem; max-height: 16rem; overflow-y: auto; }
  .cytv-option {
    display: flex;
    gap: 0.6rem;
    align-items: flex-start;
    border: 1px solid var(--cytv-border);
    border-radius: 8px;
    padding: 0.55rem 0.7rem;
    cursor: pointer;
  }
  .cytv-option.selected { border-color: var(--accent-strong, var(--accent)); }
  .cytv-option input { margin-top: 0.2rem; accent-color: var(--accent); }
  .cytv-option-text { display: grid; gap: 0.15rem; min-width: 0; }
  .cytv-option-text strong { font-size: 0.88rem; line-height: 1.4; overflow-wrap: anywhere; }
  .cytv-option-text small { color: var(--cytv-soft); font-size: 0.78rem; overflow-wrap: anywhere; }
  summary:focus-visible, .cytv-button:focus-visible, .cytv-search:focus-visible, .cytv-option:focus-within {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
</style>
