<script lang="ts">
  import type { ProfileCatalog, SourceProfile } from "../profile-catalog";
  import { candidateProfiles, LIST_OFF, matchReasonText, type TvIdentity } from "../cleanyourtv-lists";

  let {
    catalog,
    loadError = "",
    tv,
    choice,
    lists,
    busy = false,
    onChoose,
    help,
  }: {
    /// Null while the bundled catalog loads.
    catalog: ProfileCatalog | null;
    loadError?: string;
    tv: TvIdentity;
    /// Profile id, `LIST_OFF`, or null for the lists that name this TV.
    choice: string | null;
    /// The CleanYourTV lists in use, in rank order.
    lists: SourceProfile[];
    busy?: boolean;
    /// A profile id, `LIST_OFF`, or null to go back to the suggested lists.
    onChoose: (choice: string | null) => void;
    /// The app's own help control, holding `PLAN_SOURCES_HELP`. Typed without
    /// importing "svelte": each app resolves its own copy of the package.
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    help?: any;
  } = $props();

  const candidates = $derived(catalog ? candidateProfiles(catalog.profiles, tv) : []);
  let choosing = $state(false);
  let query = $state("");
  let picked = $state("");

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
    picked = lists[0]?.profile ?? candidates[0]?.profile.profile ?? "";
  }

  function confirm() {
    if (!picked) return;
    choosing = false;
    onChoose(picked);
  }

  function reasonFor(profile: SourceProfile): string {
    const candidate = candidates.find((c) => c.profile.profile === profile.profile);
    return candidate ? matchReasonText(candidate, tv) : profile.upstream_context.os;
  }
</script>

<section class="cytv" aria-label="Plan sources" data-cleanyourtv-card>
  {#if loadError}
    <p class="cytv-line" role="alert">Couldn't load the CleanYourTV lists. {loadError}</p>
  {:else if !catalog}
    <p class="cytv-line muted">Loading CleanYourTV lists…</p>
  {:else if choosing}
    <fieldset class="cytv-choose">
      <legend class="cytv-line">Which CleanYourTV list is your TV's?</legend>
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
        <button class="cytv-button cytv-primary" disabled={!picked || busy} onclick={confirm}>Use this list</button>
        <button class="cytv-button" onclick={() => (choosing = false)}>Cancel</button>
      </div>
    </fieldset>
  {:else}
    <div class="cytv-row">
      <p class="cytv-line">
        {#if lists.length > 0}
          Sources: <strong>ATV Optimizer catalog</strong>, then CleanYourTV's
          {lists.length === 1 ? "list" : "lists"} for
          {#each lists as list, index (list.profile)}<strong>{list.upstream_context.tv}</strong>{index < lists.length - 2 ? ", " : index === lists.length - 2 ? " and " : ""}{/each}.
        {:else}
          Source: <strong>ATV Optimizer catalog</strong>.
          <span class="muted">{choice === LIST_OFF
            ? "CleanYourTV lists are off for this TV."
            : candidates.length > 0
              ? "CleanYourTV has lists for this model. Choose yours to use it."
              : "No CleanYourTV list names this TV."}</span>
        {/if}
      </p>
      <div class="cytv-actions">
        {#if choice === LIST_OFF}
          <button class="cytv-button" disabled={busy} onclick={() => onChoose(null)}>Turn on CleanYourTV</button>
        {:else}
          <button class="cytv-button" disabled={busy} onclick={startChoosing}>
            {lists.length > 0 ? "Change list" : "Choose a list"}
          </button>
          {#if choice}
            <button class="cytv-button" disabled={busy} onclick={() => onChoose(null)}>Use suggested</button>
          {/if}
          {#if lists.length > 0}
            <button class="cytv-button" disabled={busy} onclick={() => onChoose(LIST_OFF)}>Turn off</button>
          {/if}
        {/if}
        {@render help?.()}
      </div>
    </div>
  {/if}
</section>

<style>
  .cytv {
    --cytv-border: var(--border, var(--line));
    --cytv-soft: var(--fg-secondary, var(--text-soft));
    --cytv-inset: var(--bg-inset, var(--canvas));
    border: 1px solid var(--cytv-border);
    border-radius: 10px;
    background: var(--bg-surface, var(--surface));
    padding: 0.65rem 0.85rem;
    margin: 0.75rem 0;
    min-width: 0;
  }
  .cytv-row { display: flex; align-items: center; justify-content: space-between; gap: 0.6rem 1rem; flex-wrap: wrap; }
  .cytv-line { margin: 0; line-height: 1.5; overflow-wrap: anywhere; flex: 1 1 22rem; min-width: 0; }
  .muted { color: var(--cytv-soft); }
  .cytv-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; }
  .cytv-button {
    font: inherit;
    font-size: 0.85rem;
    color: inherit;
    background: var(--bg-surface-2, var(--surface-2));
    border: 1px solid var(--cytv-border);
    border-radius: 8px;
    padding: 0.4rem 0.75rem;
    min-height: 2.25rem;
    width: auto;
    cursor: pointer;
  }
  @media (pointer: coarse) {
    .cytv-button { min-height: 2.75rem; }
  }
  .cytv-primary {
    background: var(--accent-strong, var(--accent));
    color: var(--accent-ink);
    border-color: var(--accent-strong, var(--accent));
    font-weight: 600;
  }
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
  .cytv-option.selected { border-color: var(--accent); }
  .cytv-option input { margin-top: 0.2rem; accent-color: var(--accent-strong, var(--accent)); }
  .cytv-option-text { display: grid; gap: 0.15rem; min-width: 0; }
  .cytv-option-text strong { font-size: 0.88rem; line-height: 1.4; overflow-wrap: anywhere; }
  .cytv-option-text small { color: var(--cytv-soft); font-size: 0.78rem; overflow-wrap: anywhere; }
  .cytv-button:focus-visible, .cytv-search:focus-visible, .cytv-option:focus-within {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
</style>
