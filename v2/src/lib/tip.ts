let nextId = 0;
let holder: HTMLElement | null = null;
let bubble: HTMLElement | null = null;
let owner: HTMLElement | null = null;

function descriptions(): HTMLElement {
  if (!holder?.isConnected) {
    holder = document.createElement("div");
    holder.hidden = true;
    holder.dataset.tipDescriptions = "";
    document.body.append(holder);
  }
  return holder;
}

const GAP = 6;
const MARGIN = 8;

/// One floating bubble for the whole page, positioned against the viewport,
/// so a badge inside a clipping container (the App List name row hides its
/// overflow) still shows its whole explanation.
function show(node: HTMLElement, text: string) {
  if (!bubble?.isConnected) {
    bubble = document.createElement("div");
    bubble.className = "tip-bubble";
    bubble.setAttribute("aria-hidden", "true");
    document.body.append(bubble);
    // Follow the badge: focusing one off screen scrolls it into view.
    const follow = () => owner && bubble && show(owner, bubble.textContent ?? "");
    window.addEventListener("scroll", follow, true);
    window.addEventListener("resize", follow);
  }
  owner = node;
  bubble.textContent = text;
  bubble.hidden = false;
  const r = node.getBoundingClientRect();
  const vw = document.documentElement.clientWidth;
  const { offsetWidth: w, offsetHeight: h } = bubble;
  const above = r.top - GAP - h;
  bubble.style.top = `${above >= MARGIN ? above : r.bottom + GAP}px`;
  bubble.style.left = `${Math.min(Math.max(MARGIN, r.left + r.width / 2 - w / 2), Math.max(MARGIN, vw - MARGIN - w))}px`;
}

function hide(node: HTMLElement) {
  if (owner !== node || !bubble) return;
  bubble.hidden = true;
  owner = null;
}

/// A short explanation on a badge or label that is not a control. A hover
/// tooltip alone leaves keyboard and touch users without it, so this makes
/// the element focusable: the explanation opens on hover, keyboard focus,
/// click and tap, Escape or moving away dismisses it, and the same words are
/// the element's accessible description.
export function tip(node: HTMLElement, text: string) {
  let current = text;
  const description = document.createElement("span");
  description.id = `tip-${++nextId}`;
  description.textContent = current;
  descriptions().append(description);

  const open = () => show(node, current);
  const close = () => {
    if (document.activeElement !== node) hide(node);
  };
  const blur = () => hide(node);
  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && document.activeElement === node) {
      e.preventDefault();
      e.stopPropagation();
      node.blur();
    }
  }
  node.dataset.tipFocus = "";
  node.tabIndex = 0;
  node.setAttribute("aria-describedby", description.id);
  node.addEventListener("pointerenter", open);
  node.addEventListener("pointerleave", close);
  node.addEventListener("focus", open);
  node.addEventListener("blur", blur);
  node.addEventListener("keydown", onKeydown);

  return {
    update(next: string) {
      current = next;
      description.textContent = next;
      if (owner === node) show(node, next);
    },
    destroy() {
      hide(node);
      node.removeEventListener("pointerenter", open);
      node.removeEventListener("pointerleave", close);
      node.removeEventListener("focus", open);
      node.removeEventListener("blur", blur);
      node.removeEventListener("keydown", onKeydown);
      description.remove();
    },
  };
}
