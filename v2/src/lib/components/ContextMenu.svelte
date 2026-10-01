<script lang="ts">
  import { onMount, tick } from "svelte";
  import {
    closeContextMenu,
    installContextMenuGuard,
    subscribeContextMenu,
    type MenuItem,
    type OpenMenu,
  } from "$lib/contextmenu";

  // Raw: the identity check in `place` compares against the object handed in.
  let menu = $state.raw<OpenMenu | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);
  let left = $state(0);
  let top = $state(0);
  let toast = $state("");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  onMount(() => {
    const uninstall = installContextMenuGuard();
    const unsubscribe = subscribeContextMenu((m) => {
      const closing = menu && !m;
      const previous = menu;
      menu = m;
      if (m) void place(m);
      else if (closing) restoreFocus(previous);
    });
    return () => {
      uninstall();
      unsubscribe();
      clearTimeout(toastTimer);
    };
  });

  function restoreFocus(m: OpenMenu | null) {
    const el = m?.returnFocus;
    if (el?.isConnected) el.focus({ preventScroll: true });
  }

  /// Open at the pointer, then pull back inside the window once its size is
  /// known. Two pixels off the pointer so the release of a right-press (which
  /// is when WebKitGTK and WebView2 fire the event) never lands on an item.
  async function place(m: OpenMenu) {
    left = m.x + 2;
    top = m.y + 2;
    await tick();
    if (!menuEl || menu !== m) return;
    const r = menuEl.getBoundingClientRect();
    const pad = 8;
    left = Math.max(pad, Math.min(left, window.innerWidth - r.width - pad));
    top = Math.max(pad, Math.min(top, window.innerHeight - r.height - pad));
    items()[0]?.focus();
  }

  function items(): HTMLButtonElement[] {
    return menuEl
      ? Array.from(menuEl.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not([aria-disabled="true"])'))
      : [];
  }

  function move(delta: number | "first" | "last") {
    const list = items();
    if (list.length === 0) return;
    const at = list.indexOf(document.activeElement as HTMLButtonElement);
    const next =
      delta === "first" ? 0
      : delta === "last" ? list.length - 1
      : (at + delta + list.length) % list.length;
    list[next].focus();
  }

  function onKeydown(e: KeyboardEvent) {
    switch (e.key) {
      case "Escape":
        e.preventDefault();
        e.stopPropagation();
        closeContextMenu();
        break;
      case "ArrowDown":
        e.preventDefault();
        move(1);
        break;
      case "ArrowUp":
        e.preventDefault();
        move(-1);
        break;
      case "Home":
        e.preventDefault();
        move("first");
        break;
      case "End":
        e.preventDefault();
        move("last");
        break;
      case "Tab":
        // Focus stays in the menu while it is open.
        e.preventDefault();
        move(e.shiftKey ? -1 : 1);
        break;
    }
  }

  async function activate(item: MenuItem) {
    if (item.disabled) return;
    closeContextMenu();
    try {
      const said = await item.run();
      if (typeof said === "string" && said) flash(said);
    } catch (err) {
      flash(`Couldn't ${item.label.toLowerCase()}: ${err}`);
    }
  }

  function flash(message: string) {
    toast = message;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 1800);
  }

  function onPointerDownOutside(e: PointerEvent) {
    if (menu && menuEl && !menuEl.contains(e.target as Node)) closeContextMenu();
  }

  function dismiss() {
    if (menu) closeContextMenu();
  }
</script>

<svelte:window
  onpointerdowncapture={onPointerDownOutside}
  onblur={dismiss}
  onresize={dismiss}
/>
<svelte:document onscrollcapture={dismiss} />

{#if menu}
  <div
    class="app-context-menu"
    role="menu"
    tabindex="-1"
    aria-label="Actions"
    bind:this={menuEl}
    style:left={`${left}px`}
    style:top={`${top}px`}
    onkeydown={onKeydown}
    oncontextmenu={(e) => e.preventDefault()}
  >
    {#each menu.items as item}
      <button
        type="button"
        role="menuitem"
        tabindex="-1"
        class:danger={item.danger}
        aria-disabled={item.disabled ? "true" : undefined}
        onclick={() => activate(item)}
      >{item.label}</button>
    {/each}
  </div>
{/if}

{#if toast}
  <div class="app-context-toast" role="status">{toast}</div>
{/if}

<style>
  .app-context-menu {
    position: fixed;
    z-index: 1000;
    min-width: 180px;
    max-width: 320px;
    padding: 4px;
    display: flex;
    flex-direction: column;
    background: var(--bg-surface);
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    box-shadow: var(--shadow-modal);
    outline: none;
  }

  .app-context-menu button {
    all: unset;
    box-sizing: border-box;
    padding: 6px 10px;
    border-radius: 5px;
    font-size: 13px;
    line-height: 1.3;
    color: var(--fg-primary);
    cursor: default;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .app-context-menu button:hover:not([aria-disabled="true"]),
  .app-context-menu button:focus-visible,
  .app-context-menu button:focus {
    background: var(--bg-button-hover);
  }

  .app-context-menu button.danger {
    color: var(--danger-text);
  }

  .app-context-menu button[aria-disabled="true"] {
    color: var(--fg-muted);
    opacity: 0.6;
  }

  .app-context-toast {
    position: fixed;
    z-index: 1001;
    left: 50%;
    bottom: 24px;
    transform: translateX(-50%);
    padding: 6px 12px;
    border-radius: 6px;
    background: var(--bg-surface-2);
    border: 1px solid var(--border-strong);
    color: var(--fg-primary);
    font-size: 13px;
    box-shadow: var(--shadow-modal);
    pointer-events: none;
  }
</style>
