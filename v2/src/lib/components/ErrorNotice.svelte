<script lang="ts">
  import Icon from "./Icon.svelte";
  import { errorDetails, presentError } from "../../../shared/errors";
  import { parseSupport, TIER_LABEL } from "../../../shared/support";

  // One way to show a failed action: what happened in plain words, the code
  // (the same one the anonymous report carries), a hint when the TV's support
  // tier could explain it, the raw text behind a disclosure, and Copy details.
  let {
    error,
    support = null,
    action = "",
    device = "",
    summary = null,
    class: klass = "",
  }: {
    error: unknown;
    support?: unknown;
    /// What the user was doing, e.g. "Disable com.example".
    action?: string;
    /// The TV's model, for Copy details.
    device?: string;
    /// A more specific explanation from the caller (adb-errors.ts for
    /// connect/pair), used instead of the generic sentence for the code.
    summary?: string | null;
    /// Extra classes for the root, so existing selectors keep working.
    class?: string;
  } = $props();

  const info = $derived(support ? parseSupport(support) : null);
  const presented = $derived(presentError(error, info));
  const message = $derived(summary ?? presented.message);
  let copied = $state(false);

  async function copy() {
    const text = errorDetails(
      { ...presented, message },
      {
        action: action || undefined,
        device: device || undefined,
        tier: info ? TIER_LABEL[info.tier] : undefined,
        app: "ATV Optimizer (desktop)",
      },
    );
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch {
      copied = false;
    }
  }
</script>

<div class="error-notice {klass}" role="alert" data-error-code={presented.code}>
  <div class="error-head">
    <Icon name="error" size={18} />
    <span class="error-message">
      {#if action}<strong class="error-action">{action} failed.</strong>{/if}
      <span class="error-text">{message}</span>
    </span>
    <code class="error-code" title="Error code">{presented.code}</code>
  </div>
  {#if presented.tierHint}
    <p class="error-hint small">{presented.tierHint}</p>
  {/if}
  <div class="error-foot">
    {#if presented.raw && presented.raw !== message}
      <details class="error-details">
        <summary>Details</summary>
        <pre>{presented.raw}</pre>
      </details>
    {/if}
    <button class="error-copy" onclick={copy}>
      <Icon name="content_copy" size={14} />
      {copied ? "Copied" : "Copy details"}
    </button>
  </div>
</div>

<style>
  .error-notice {
    margin: 0.5rem 0;
    padding: 0.6rem 0.8rem;
    border: 1px solid var(--danger-border);
    border-radius: var(--radius-sm);
    background: var(--danger-surface);
    color: var(--fg-primary);
  }
  .error-head {
    display: flex;
    gap: 0.5rem;
    align-items: flex-start;
  }
  .error-head :global(.msr) {
    color: var(--danger-text);
  }
  .error-message {
    flex: 1;
  }
  .error-code {
    font-size: 0.75rem;
    padding: 0.05rem 0.35rem;
    border-radius: var(--radius-sm);
    background: var(--bg-muted);
    color: var(--fg-secondary);
    white-space: nowrap;
  }
  .error-hint {
    margin: 0.35rem 0 0 1.6rem;
    color: var(--fg-secondary);
  }
  .error-foot {
    display: flex;
    gap: 0.6rem;
    align-items: flex-start;
    justify-content: space-between;
    margin: 0.35rem 0 0 1.6rem;
  }
  .error-details summary {
    cursor: pointer;
    color: var(--fg-muted);
    font-size: 0.82rem;
  }
  .error-details pre {
    white-space: pre-wrap;
    word-break: break-word;
    font-size: 0.78rem;
    margin: 0.3rem 0 0;
  }
  .error-copy {
    font-size: 0.8rem;
    padding: 0.15rem 0.5rem;
    white-space: nowrap;
  }
</style>
