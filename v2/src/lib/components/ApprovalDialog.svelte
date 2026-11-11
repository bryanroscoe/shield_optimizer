<script lang="ts">
  import { onMount, tick } from "svelte";
  import Icon from "$lib/components/Icon.svelte";
  import { dialogFocus } from "$lib/dialog-focus";

  /// "asking": the request is being made. "waiting": the TV was asked (or,
  /// under platform-tools, is probably asking) and the app waits for Allow.
  /// "approved", "not_approved" and "error" end it.
  export type ApprovalPhase = "asking" | "waiting" | "approved" | "not_approved" | "error";

  // The approval pop-up: one TV, a pulsing TV while it waits, one
  // instruction. Closing it keeps the wait running; only Cancel request stops
  // it. The same dialog serves both ADB engines.
  let {
    name,
    address,
    usb = false,
    wirelessDebugging = false,
    phase,
    error = null,
    deadline = null,
    onClose,
    onCancel,
    onRetry,
  }: {
    name: string;
    address: string | null;
    /// A TV connected by USB asks again when it is plugged back in.
    usb?: boolean;
    /// Held under a Wireless debugging key: the TV lists this computer there.
    wirelessDebugging?: boolean;
    phase: ApprovalPhase;
    error?: string | null;
    /// When the wait ends (ms since the epoch); null when unknown.
    deadline?: number | null;
    onClose: () => void;
    onCancel: () => void;
    onRetry: () => void;
  } = $props();

  let dialog: HTMLDivElement;
  const titleId = $props.id();

  const title = $derived(
    phase === "approved"
      ? `${name} allowed this computer`
      : phase === "not_approved"
        ? `${name} didn't allow this computer`
        : phase === "error"
          ? `Couldn't ask ${name}`
          : `Allow this computer on ${name}`,
  );
  const promptName = $derived(usb ? "“Allow USB debugging?”" : "“Allow debugging?”");

  let now = $state(Date.now());
  $effect(() => {
    void deadline;
    now = Date.now();
  });
  const minutesLeft = $derived(deadline === null ? null : Math.max(1, Math.ceil((deadline - now) / 60_000)));

  /// The control to land on in each phase. Its buttons change with the
  /// phase, and the one that had focus may be gone.
  function primary(): HTMLElement {
    return dialog.querySelector<HTMLElement>("[data-primary]") ?? dialog;
  }

  let shownPhase: ApprovalPhase | null = null;
  $effect(() => {
    const current = phase;
    const changed = shownPhase !== null && shownPhase !== current;
    shownPhase = current;
    if (!changed) return;
    void tick().then(() => {
      if (dialog && !dialog.contains(document.activeElement)) primary().focus();
    });
  });

  onMount(() => {
    const clock = setInterval(() => (now = Date.now()), 5000);
    return () => clearInterval(clock);
  });
</script>

<div class="approval-backdrop" role="presentation" onclick={onClose}></div>
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="approval-dialog"
  role="dialog"
  aria-modal="true"
  aria-labelledby={titleId}
  tabindex="-1"
  bind:this={dialog}
  use:dialogFocus={{ onClose, escapeFromOutside: true, recapture: primary, restoreFocus: false }}
  data-approval-dialog
  data-phase={phase}
>
  <div class="visual" aria-hidden="true">
    {#if phase === "approved"}
      <span class="halo"></span>
      <span class="check"><Icon name="check" size={44} weight={600} /></span>
    {:else}
      {#if phase === "asking" || phase === "waiting"}
        <span class="ring"></span>
        <span class="ring ring2"></span>
      {/if}
      <span class="core" class:quiet={phase === "not_approved" || phase === "error"}>
        <Icon name="tv" size={38} />
      </span>
    {/if}
  </div>

  <h2 id={titleId}>{title}</h2>
  {#if address}<p class="address mono">{address}</p>{/if}

  <div class="status" aria-live="polite">
    {#if phase === "asking"}
      <p>Asking {name}…</p>
    {:else if phase === "waiting"}
      {#if usb}
        <p class="instruction">
          Unplug the USB cable from {name} and plug it back in. Then tick
          <strong>Always allow from this computer</strong> and choose <strong>Allow</strong>.
        </p>
      {:else}
        <p class="instruction">
          On {name}, tick <strong>Always allow from this computer</strong>, then choose
          <strong>Allow</strong>.
        </p>
      {/if}
      <p class="note" data-wait-note>
        {#if minutesLeft !== null}
          Waiting up to {minutesLeft === 1 ? "1 more minute" : `${minutesLeft} more minutes`}.
        {/if}
        Closing this window keeps waiting; only Cancel request stops it.
      </p>
    {:else if phase === "approved"}
      <p>Connected.</p>
    {:else if phase === "not_approved"}
      <p>The wait ended before the TV allowed this computer. Nothing changed on it.</p>
    {:else if phase === "error"}
      <p class="error-text" role="alert">{error}</p>
    {/if}
  </div>

  {#if phase !== "approved"}
    <details class="help">
      <summary>Not seeing the prompt?</summary>
      <ul>
        <li>Wake the TV. The prompt can be hidden behind the screensaver.</li>
        <li>It is called {promptName}{#if !usb} (some TVs still say “USB” even over the network){/if}.</li>
        <li>
          Chose Cancel on the TV? The TV doesn't tell this computer, so press Cancel request here,
          then ask again.
        </li>
        <li>
          Still nothing? On the TV, open Developer options and choose Revoke USB debugging
          authorizations{#if wirelessDebugging} (or Wireless debugging → tap this computer → Forget){/if}.
          That un-trusts every computer, not just this one. Then ask again.
        </li>
      </ul>
    </details>
  {/if}

  <div class="actions">
    {#if phase === "asking" || phase === "waiting"}
      <button onclick={onClose} data-primary>Close</button>
      <button onclick={onCancel}>Cancel request</button>
    {:else if phase === "not_approved" || phase === "error"}
      <button onclick={onClose}>Close</button>
      <button class="primary" onclick={onRetry} data-primary>Try again</button>
    {/if}
  </div>
</div>

<style>
  .approval-backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: var(--scrim);
  }
  .approval-dialog {
    position: fixed;
    z-index: 41;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    box-sizing: border-box;
    width: min(28rem, calc(100vw - 32px));
    max-height: calc(100vh - 32px);
    overflow-y: auto;
    padding: 1.5rem 1.5rem 1.25rem;
    border: 1px solid var(--border-strong);
    border-radius: 14px;
    background: var(--bg-surface);
    box-shadow: var(--shadow-modal);
    text-align: center;
    overflow-wrap: anywhere;
  }
  .approval-dialog:focus {
    outline: none;
  }
  .approval-dialog:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .visual {
    position: relative;
    height: 150px;
    display: grid;
    place-items: center;
    margin-bottom: 0.5rem;
  }
  .ring {
    position: absolute;
    width: 120px;
    height: 120px;
    border-radius: 50%;
    border: 1.5px solid color-mix(in srgb, var(--accent) 55%, transparent);
    animation: approval-radar 2.4s ease-out infinite;
  }
  .ring2 {
    animation-delay: 1.2s;
  }
  .core {
    position: relative;
    width: 76px;
    height: 76px;
    border-radius: 22px;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    display: grid;
    place-items: center;
    color: var(--accent-strong);
    box-shadow: 0 0 36px color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .core.quiet {
    color: var(--fg-muted);
    box-shadow: none;
  }
  .halo {
    position: absolute;
    width: 128px;
    height: 128px;
    border-radius: 50%;
    background: var(--ok-surface);
  }
  .check {
    position: relative;
    width: 86px;
    height: 86px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--ok);
    color: var(--bg-page);
    box-shadow: 0 0 40px color-mix(in srgb, var(--ok) 40%, transparent);
  }
  @keyframes approval-radar {
    0% {
      transform: scale(0.6);
      opacity: 0.9;
    }
    100% {
      transform: scale(1.75);
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .ring {
      animation: none;
      opacity: 0.35;
      transform: scale(1.15);
    }
    .ring2 {
      display: none;
    }
  }
  h2 {
    margin: 0;
    font-size: 1.15rem;
  }
  .address {
    margin: 0.3rem 0 0;
    color: var(--fg-muted);
    font-size: 0.85rem;
  }
  .status {
    margin: 0.9rem 0 0;
  }
  .status p {
    margin: 0.4rem 0 0;
  }
  .instruction {
    font-size: 1rem;
    line-height: 1.45;
  }
  .note {
    color: var(--fg-muted);
    font-size: 0.85rem;
  }
  .error-text {
    color: var(--danger-text);
    font-size: 0.9rem;
  }
  .help {
    margin: 1rem 0 0;
    text-align: left;
    font-size: 0.88rem;
    color: var(--fg-secondary);
  }
  .help summary {
    cursor: pointer;
    text-align: center;
  }
  .help summary:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .help ul {
    margin: 0.5rem 0 0;
    padding-left: 1.2rem;
  }
  .help li {
    margin: 0.3rem 0;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 0.5rem;
    margin-top: 1.1rem;
  }
  .actions:empty {
    display: none;
  }
</style>
