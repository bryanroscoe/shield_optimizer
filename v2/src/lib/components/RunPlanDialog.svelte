<script lang="ts">
  import Icon from "$lib/components/Icon.svelte";
  import { dialogFocus } from "$lib/dialog-focus";
  import type { RunConfirmSection } from "$lib/run-confirmation";

  // The Optimize/Restore confirmation. A native alert grows instead of
  // scrolling and reads as one paragraph, so the plan is listed here: one
  // section per safety tier, one line per app. Consequences stay visible;
  // routine reasons fold under each app.
  let {
    title,
    confirmLabel,
    sections,
    uninstalls,
    restoreMode,
    supportWarning = null,
    onConfirm,
    onCancel,
  }: {
    title: string;
    confirmLabel: string;
    sections: RunConfirmSection[];
    uninstalls: number;
    restoreMode: boolean;
    supportWarning?: string | null;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  const titleId = $props.id();
  let cancelButton: HTMLButtonElement;
</script>

<div class="run-backdrop" role="presentation" onclick={onCancel}></div>
<div
  class="run-dialog"
  role="dialog"
  aria-modal="true"
  aria-labelledby={titleId}
  tabindex="-1"
  use:dialogFocus={{ onClose: onCancel, initial: () => cancelButton, restoreFocus: false }}
  data-run-plan-dialog
>
  <h2 id={titleId}>{title}</h2>
  <div class="run-body">
    {#if supportWarning}
      <p class="support-warning" data-support-warning>
        <Icon name="warning" size={16} />
        <span>{supportWarning}</span>
      </p>
    {/if}
    {#each sections as section (section.kind)}
      <section class="run-section" data-run-section={section.kind} aria-labelledby={`${titleId}-${section.kind}`}>
        <h3 id={`${titleId}-${section.kind}`}>
          {#if section.cls}<span class={section.cls}>{section.label}</span>{:else}{section.label}{/if}
          <span class="count">{section.apps.length}</span>
        </h3>
        <ul>
          {#each section.apps as app (app.package)}
            <li data-run-app={app.package}>
              <div class="app-line">
                <span class="verb">{app.verb}</span>
                <strong class="name">{app.name}</strong>
              </div>
              {#if section.showReason && app.reason}
                <p class="reason">{app.reason}</p>
              {/if}
              {#if app.outcome}
                <p class="outcome" class:no-way-back={app.noWayBack} data-run-outcome>{app.outcome}</p>
              {/if}
              <details class="app-more">
                <summary aria-label={`Details for ${app.name}`}>Details</summary>
                <p class="mono pkg">{app.package}</p>
                {#if !section.showReason && app.reason}<p>{app.reason}</p>{/if}
                {#if app.outcomeDetail}<p>{app.outcomeDetail}</p>{/if}
              </details>
            </li>
          {/each}
        </ul>
      </section>
    {/each}
    {#if !restoreMode}
      <ul class="rules">
        <li>You can re-enable disabled apps later with Restore mode.</li>
        {#if uninstalls > 0}
          <li data-uninstall-rule>
            Uninstall removes an app for this TV's user. Restore can't bring it back.
          </li>
        {/if}
      </ul>
    {/if}
  </div>
  <div class="actions">
    <button type="button" bind:this={cancelButton} onclick={onCancel}>Cancel</button>
    <button type="button" class="primary" onclick={onConfirm}>{confirmLabel}</button>
  </div>
</div>

<style>
  .run-backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: var(--scrim);
  }
  .run-dialog {
    position: fixed;
    z-index: 41;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    box-sizing: border-box;
    width: min(36rem, calc(100vw - 32px));
    max-height: calc(100vh - 32px);
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
    padding: 1.25rem;
    border: 1px solid var(--border-strong);
    border-radius: 14px;
    background: var(--bg-surface);
    box-shadow: var(--shadow-modal);
    text-align: left;
  }
  .run-dialog:focus {
    outline: none;
  }
  h2 {
    margin: 0;
    font-size: 1.1rem;
  }
  .run-body {
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    overflow-wrap: anywhere;
  }
  .support-warning {
    display: flex;
    gap: 0.5rem;
    align-items: flex-start;
    margin: 0;
    padding: 0.6rem 0.75rem;
    border-radius: 8px;
    border: 1px solid var(--danger-border);
    background: var(--danger-surface);
    color: var(--danger-surface-text);
    font-size: 0.9rem;
    line-height: 1.45;
  }
  .run-section h3 {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin: 0 0 0.45rem;
    font-size: 0.92rem;
    font-weight: 600;
  }
  .count {
    color: var(--fg-muted);
    font-weight: 500;
    font-size: 0.85rem;
  }
  .run-section ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
  }
  .run-section li {
    padding-left: 0.7rem;
    border-left: 2px solid var(--border-strong);
  }
  .app-line {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    align-items: baseline;
    line-height: 1.4;
  }
  .verb {
    color: var(--fg-secondary);
    font-size: 0.85rem;
  }
  .name {
    font-weight: 600;
  }
  .reason,
  .outcome {
    margin: 0.15rem 0 0;
    font-size: 0.85rem;
    line-height: 1.45;
    color: var(--fg-secondary);
  }
  .outcome.no-way-back {
    color: var(--warn);
    font-weight: 550;
  }
  .app-more {
    margin-top: 0.15rem;
    font-size: 0.82rem;
    color: var(--fg-secondary);
  }
  .app-more summary {
    width: fit-content;
    cursor: pointer;
    line-height: 1.6;
  }
  .app-more summary:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 3px;
    border-radius: 2px;
  }
  .app-more p {
    margin: 0.3rem 0;
    line-height: 1.45;
  }
  .pkg {
    font-family: var(--mono);
    color: var(--fg-muted);
  }
  .rules {
    margin: 0;
    padding-left: 1.2rem;
    font-size: 0.88rem;
    line-height: 1.5;
    color: var(--fg-secondary);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.5rem;
  }
</style>
