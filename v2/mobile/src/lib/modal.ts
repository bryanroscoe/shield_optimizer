// One document lock shared by nested sheets/dialogs. Each action owns only its
// own stack entry, so removing an underlying sheet cannot unlock a child modal.
type Options = { onClose: () => void };
type Entry = { node: HTMLElement; options: Options; returnFocus: HTMLElement | null; lastFocus: HTMLElement | null };
const stack: Entry[] = [];
let unlock: (() => void) | undefined;
let documentReturnFocus: HTMLElement | null = null;
const focusable = 'button:not(:disabled), a[href], input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex]:not([tabindex="-1"])';

function controls(node: HTMLElement): HTMLElement[] {
  return Array.from(node.querySelectorAll<HTMLElement>(focusable))
    .filter((element) => element.getClientRects().length > 0 && !element.closest('[inert]'));
}
function focusControl(element: HTMLElement, entry: Entry) {
  element.focus({ preventScroll: true });
  // Keep the document lock intact while revealing focus inside a long sheet.
  // Walking only modal ancestors avoids scrollIntoView moving the page behind
  // the overlay (and works without newer WebView container:"nearest" support).
  for (let parent = element.parentElement; parent && entry.node.contains(parent); parent = parent.parentElement) {
    if (!/^(auto|scroll)$/.test(getComputedStyle(parent).overflowY) || parent.scrollHeight <= parent.clientHeight) continue;
    const target = element.getBoundingClientRect();
    const bounds = parent.getBoundingClientRect();
    const top = bounds.top + parent.clientTop;
    const bottom = top + parent.clientHeight;
    if (target.top < top) parent.scrollTop += target.top - top;
    else if (target.bottom > bottom) parent.scrollTop += target.bottom - bottom;
  }
}
function focusInside(entry: Entry) {
  focusControl(controls(entry.node)[0] ?? entry.node.querySelector<HTMLElement>('[role="dialog"], [role="menu"]') ?? entry.node, entry);
}
function holdDocument() {
  const x = window.scrollX;
  const y = window.scrollY;
  const saved: Array<() => void> = [];
  const set = (element: HTMLElement, property: string, value: string) => {
    const before = element.style.getPropertyValue(property);
    const priority = element.style.getPropertyPriority(property);
    saved.push(() => {
      if (before) element.style.setProperty(property, before, priority);
      else element.style.removeProperty(property);
    });
    element.style.setProperty(property, value);
  };
  // A fixed body also covers mobile WebViews where overflow:hidden alone does
  // not stop touch scrolling. Do not disable touch or pinch zoom globally.
  set(document.documentElement, 'overflow', 'hidden');
  set(document.documentElement, 'overscroll-behavior', 'none');
  set(document.body, 'position', 'fixed');
  set(document.body, 'top', `${-y}px`);
  set(document.body, 'left', `${-x}px`);
  set(document.body, 'width', '100%');
  set(document.body, 'overflow', 'hidden');
  return () => {
    saved.reverse().forEach((restore) => restore());
    window.scrollTo({ left: x, top: y, behavior: 'instant' });
  };
}

export function modal(node: HTMLElement, options: Options) {
  const previous = stack.at(-1);
  const active = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  // A triggering action may briefly disable its button while awaiting a safety
  // read, causing the browser to drop focus before its confirmation mounts.
  const returnFocus = previous && (!active || !previous.node.contains(active))
    ? previous.lastFocus : active;
  const entry: Entry = { node, options, returnFocus, lastFocus: null };
  if (!stack.length) {
    documentReturnFocus = entry.returnFocus;
    unlock = holdDocument();
  }
  stack.push(entry);
  const initialFocus = requestAnimationFrame(() => {
    if (stack.at(-1) === entry && node.isConnected) focusInside(entry);
  });
  const keydown = (event: KeyboardEvent) => {
    if (stack.at(-1) !== entry) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopImmediatePropagation();
      entry.options.onClose();
    } else if (event.key === 'Tab') {
      const items = controls(node);
      const first = items[0];
      const last = items.at(-1);
      if (!first) {
        event.preventDefault();
        focusInside(entry);
      } else if (event.shiftKey && (document.activeElement === first || !node.contains(document.activeElement))) {
        event.preventDefault();
        if (last) focusControl(last, entry);
      } else if (!event.shiftKey && (document.activeElement === last || !node.contains(document.activeElement))) {
        event.preventDefault();
        focusControl(first, entry);
      }
    }
  };
  const focusin = (event: FocusEvent) => {
    if (stack.at(-1) === entry && event.target instanceof HTMLElement && node.contains(event.target)) entry.lastFocus = event.target;
  };
  document.addEventListener('focusin', focusin);
  document.addEventListener('keydown', keydown, true);
  return {
    update(next: Options) { entry.options = next; },
    destroy() {
      cancelAnimationFrame(initialFocus);
      document.removeEventListener('keydown', keydown, true);
      document.removeEventListener('focusin', focusin);
      const wasTop = stack.at(-1) === entry;
      const index = stack.indexOf(entry);
      if (index < 0) return;
      stack.splice(index, 1);
      const fallbackFocus = documentReturnFocus;
      if (!stack.length) {
        documentReturnFocus = null;
        unlock?.();
        unlock = undefined;
      }
      if (!wasTop) return;
      // Wait for Svelte to remove the closing overlay; never focus behind a
      // newly opened modal or a disconnected launcher after screen navigation.
      queueMicrotask(() => {
        const top = stack.at(-1);
        if (entry.returnFocus?.isConnected && (!top || top.node.contains(entry.returnFocus))) {
          if (top) focusControl(entry.returnFocus, top);
          else entry.returnFocus.focus({ preventScroll: true });
        } else if (top && top.node.isConnected) focusInside(top);
        else if (!top && fallbackFocus?.isConnected) fallbackFocus.focus({ preventScroll: true });
      });
    },
  };
}
