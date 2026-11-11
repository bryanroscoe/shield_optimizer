let nextId = 0;
let holder: HTMLElement | null = null;

function descriptions(): HTMLElement {
  if (!holder?.isConnected) {
    holder = document.createElement("div");
    holder.hidden = true;
    holder.dataset.tipDescriptions = "";
    document.body.append(holder);
  }
  return holder;
}

/// A short explanation on a badge or label that is not a control. `data-tip`
/// alone shows only on hover, so this makes the element focusable: the tip
/// then also opens on keyboard focus, click and tap (app.css,
/// `[data-tip-focus]`), Escape dismisses it, and the same words are the
/// element's accessible description.
export function tip(node: HTMLElement, text: string) {
  const description = document.createElement("span");
  description.id = `tip-${++nextId}`;
  descriptions().append(description);

  function apply(value: string) {
    node.dataset.tip = value;
    description.textContent = value;
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && document.activeElement === node) {
      e.preventDefault();
      e.stopPropagation();
      node.blur();
    }
  }

  apply(text);
  node.dataset.tipFocus = "";
  node.tabIndex = 0;
  node.setAttribute("aria-describedby", description.id);
  node.addEventListener("keydown", onKeydown);

  return {
    update: apply,
    destroy() {
      node.removeEventListener("keydown", onKeydown);
      description.remove();
    },
  };
}
