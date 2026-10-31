<script lang="ts">
  // One way to show a failure: what happened in plain words, the short code
  // (the same one an anonymous report carries), a hint when the TV's support
  // tier could explain it, the raw text behind Details, and Copy details.
  import { errorDetails, presentError } from "../../../shared/errors";
  import type { SupportInfo } from "../../../shared/support";
  import { session } from "../lib/session.svelte";

  let {
    error,
    title = "",
    action = "",
    support = undefined,
    guidance = "",
    onDismiss = undefined,
  }: {
    error: unknown;
    /// What was being attempted ("Couldn't connect"). Optional.
    title?: string;
    action?: string;
    /// Defaults to the connected TV's tier.
    support?: SupportInfo | null;
    /// Screen-specific advice that's better than the generic sentence.
    guidance?: string;
    onDismiss?: () => void;
  } = $props();

  const presented = $derived(presentError(error, support === undefined ? session.support : support));
  let copied = $state<"" | "ok" | "failed">("");

  async function copyDetails() {
    const text = errorDetails(presented, {
      action: action || title || undefined,
      app: "ATV Optimizer (Android)",
      device: session.connectedDevice?.model || undefined,
      tier: session.connectedDevice ? session.support.tier : undefined,
    });
    try {
      await navigator.clipboard.writeText(text);
      copied = "ok";
    } catch {
      copied = "failed";
    }
  }
</script>

<div class="error-notice" role="alert" data-code={presented.code}>
  <span class="msr" aria-hidden="true">error</span>
  <div class="en-body">
    {#if title}<span class="en-title">{title}</span>{/if}
    <span class="en-msg">{guidance || presented.message}</span>
    {#if presented.tierHint}<span class="en-hint">{presented.tierHint}</span>{/if}
    <div class="en-meta">
      <span class="mono en-code">{presented.code}</span>
      <button type="button" class="en-copy" onclick={copyDetails}>
        <span class="msr" aria-hidden="true">content_copy</span>
        {copied === "ok" ? "Copied" : copied === "failed" ? "Couldn't copy" : "Copy details"}
      </button>
      {#if onDismiss}
        <button type="button" class="en-copy en-dismiss" onclick={onDismiss}>Dismiss</button>
      {/if}
    </div>
    {#if presented.raw}
      <details class="en-details">
        <summary>Details</summary>
        <pre class="mono">{presented.raw}</pre>
      </details>
    {/if}
  </div>
</div>

<style>
  .error-notice {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    padding: 14px 16px;
    border-radius: 16px;
    background: color-mix(in srgb, var(--danger) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--danger) 28%, transparent);
    font-size: 13px;
    line-height: 1.45;
    color: var(--text-soft);
    text-align: left;
  }
  .error-notice > .msr {
    color: var(--danger);
    font-size: 22px;
    flex: none;
  }
  .en-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    flex: 1;
  }
  .en-title {
    color: var(--text);
    font-weight: 600;
  }
  .en-hint {
    color: var(--amber);
  }
  .en-meta {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .en-code {
    font-size: 11px;
    color: var(--muted);
  }
  .en-copy {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: none;
    border: none;
    padding: 0;
    color: var(--accent);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .en-dismiss {
    color: var(--muted);
    margin-left: auto;
  }
  .en-copy .msr {
    font-size: 15px;
  }
  .en-details summary {
    cursor: pointer;
    color: var(--muted);
    font-size: 12px;
  }
  .en-details pre {
    margin: 6px 0 0;
    white-space: pre-wrap;
    word-break: break-word;
    font-size: 11px;
    color: var(--muted);
  }
</style>
