<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "$lib/api";
  import type { SnapshotFile } from "$lib/types";

  let snapshots = $state<SnapshotFile[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  async function load() {
    loading = true;
    error = null;
    try {
      snapshots = await api.listSnapshots();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(load);
</script>

<section class="header-row">
  <h1>Snapshots</h1>
  <button onclick={load} disabled={loading}>{loading ? "Loading…" : "Refresh"}</button>
</section>

<p class="muted">
  Saved snapshots live in your OS app-data directory. Open a device to save a new one
  or preview applying an existing snapshot.
</p>

{#if error}
  <div class="error">{error}</div>
{:else if loading && snapshots.length === 0}
  <div class="muted">Loading…</div>
{:else if snapshots.length === 0}
  <div class="empty">
    <p>No snapshots yet.</p>
  </div>
{:else}
  <table>
    <thead>
      <tr>
        <th>Saved</th>
        <th>Device</th>
        <th>Disabled apps</th>
        <th>Filename</th>
      </tr>
    </thead>
    <tbody>
      {#each snapshots as s}
        <tr>
          <td class="mono small">{s.saved_at}</td>
          <td>{s.device_name}</td>
          <td>{s.disabled_count}</td>
          <td class="mono small">{s.filename}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}

<style>
  .header-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.6rem;
  }
  h1 {
    margin: 0;
    font-size: 1.4rem;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    margin-top: 1rem;
  }
  th, td {
    text-align: left;
    padding: 0.5rem 0.6rem;
    border-bottom: 1px solid #21262d;
    font-size: 0.9rem;
  }
  th {
    color: #7d8590;
    font-weight: 500;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .mono {
    font-family: ui-monospace, monospace;
  }
  .small {
    font-size: 0.82rem;
  }
  .empty {
    text-align: center;
    padding: 2rem 1rem;
    color: #7d8590;
  }
  .error {
    background: #5d1b1b;
    color: #ff8a80;
    padding: 0.7rem 1rem;
    border-radius: 6px;
  }
</style>
