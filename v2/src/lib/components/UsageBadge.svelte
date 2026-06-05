<script lang="ts">
  import type { AppUsage } from "$lib/types";

  // "last used" cue for the Review / remove-if-unused decision. Renders nothing
  // until usage data is loaded. Never-opened and long-idle apps read as stale
  // (highlighted) — the candidates to remove.
  let { usage }: { usage?: AppUsage } = $props();

  const DAY_MS = 86_400_000;

  function daysSince(u: AppUsage): number | null {
    if (!u.last_used || u.launch_count === 0) return null;
    // "YYYY-MM-DD HH:MM:SS" → parse as local time (device-local, close enough).
    const then = new Date(u.last_used.replace(" ", "T"));
    if (Number.isNaN(then.getTime())) return null;
    return Math.floor((Date.now() - then.getTime()) / DAY_MS);
  }

  function label(u: AppUsage): string {
    const days = daysSince(u);
    if (days === null) return "never opened";
    if (days <= 0) return "used today";
    if (days === 1) return "used yesterday";
    if (days < 30) return `${days}d ago`;
    if (days < 365) return `${Math.floor(days / 30)}mo ago`;
    return `${Math.floor(days / 365)}y ago`;
  }

  // Stale = a removal candidate: never opened, or untouched for 30+ days.
  function isStale(u: AppUsage): boolean {
    const days = daysSince(u);
    return days === null || days >= 30;
  }
</script>

{#if usage}
  <span class="usage-tag" class:stale={isStale(usage)} title="Last opened (dumpsys usagestats)">
    {label(usage)}
  </span>
{/if}

<style>
  .usage-tag {
    font-size: 0.72rem;
    color: var(--fg-muted);
  }
  .usage-tag.stale {
    color: var(--warn);
  }
</style>
