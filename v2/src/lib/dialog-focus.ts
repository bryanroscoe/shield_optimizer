import { tick } from "svelte";

export interface DialogFocusOptions {
  /// Escape (with focus inside the dialog) calls this.
  onClose: () => void;
  /// Where focus lands when the dialog opens. Defaults to the dialog itself.
  initial?: () => HTMLElement | null | undefined;
  /// Also answer Escape when focus has slipped out of the dialog (to the page
  /// body after its focused button went away). Off where the page already has
  /// its own window-level Escape for this dialog.
  escapeFromOutside?: boolean;
  /// Where Tab sends focus that has slipped out. Defaults to `initial`.
  recapture?: () => HTMLElement | null | undefined;
  /// Hand focus back to the opener on close. Off where the caller places it.
  restoreFocus?: boolean;
}

const FOCUSABLE =
  "button, [href], input, select, textarea, summary, [tabindex]:not([tabindex='-1'])";

function focusables(node: HTMLElement): HTMLElement[] {
  return [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (el) => !el.hasAttribute("disabled") && el.offsetParent !== null,
  );
}

/// Modal focus for a `role="dialog" aria-modal="true"` element: move focus in
/// on open, keep Tab inside, close on Escape and hand focus back to whatever
/// opened it on close.
export function dialogFocus(node: HTMLElement, options: DialogFocusOptions) {
  let opts = options;
  const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  if (!node.hasAttribute("tabindex")) node.tabIndex = -1;

  void tick().then(() => {
    if (!node.isConnected || node.contains(document.activeElement)) return;
    (opts.initial?.() ?? node).focus();
  });

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      // This listener is native on the dialog, so it runs before any Svelte
      // `onkeydown` inside it (those are delegated to the document root) and
      // before `svelte:window` handlers: a child that handles Escape itself
      // (a popover, a menu, an editor that clears on Escape) never sees it.
      // Such a child marks itself, or an ancestor inside the dialog, with
      // `data-own-escape`; native listeners that call preventDefault first
      // (like $lib/tip) are honoured too.
      if (e.defaultPrevented) return;
      if (e.target instanceof Element && e.target.closest("[data-own-escape]")?.closest("[aria-modal='true']") === node) {
        return;
      }
      e.preventDefault();
      e.stopPropagation();
      opts.onClose();
      return;
    }
    if (e.key !== "Tab") return;
    const items = focusables(node);
    if (items.length === 0) {
      e.preventDefault();
      node.focus();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;
    if (e.shiftKey && (active === first || active === node)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first.focus();
    }
  }

  function onWindowKeydown(e: KeyboardEvent) {
    if (!node.isConnected || node.contains(document.activeElement)) return;
    // Another modal on top (or a later one) owns the keyboard.
    const modals = document.querySelectorAll("[aria-modal='true']");
    if (modals[modals.length - 1] !== node) return;
    if (e.key === "Tab") {
      e.preventDefault();
      (opts.recapture?.() ?? opts.initial?.() ?? node).focus();
    } else if (e.key === "Escape" && opts.escapeFromOutside) {
      e.preventDefault();
      opts.onClose();
    }
  }

  node.addEventListener("keydown", onKeydown);
  window.addEventListener("keydown", onWindowKeydown);

  return {
    update(next: DialogFocusOptions) {
      opts = next;
    },
    destroy() {
      node.removeEventListener("keydown", onKeydown);
      window.removeEventListener("keydown", onWindowKeydown);
      const active = document.activeElement;
      const focusWasHere = !active || active === document.body || node.contains(active);
      if (opts.restoreFocus !== false && focusWasHere && opener?.isConnected) opener.focus();
    },
  };
}
