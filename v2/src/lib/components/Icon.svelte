<script lang="ts">
  import type { IconName } from "$lib/icons";

  // A Material Symbols glyph. Always decorative: the ligature name is real text
  // content, so without aria-hidden a screen reader announces "settings" and
  // an icon inside a button joins its accessible name — which would also break
  // the screenshot capture, since that drives buttons by exact name. Anything
  // icon-only therefore needs its own aria-label or title on the button.
  let {
    name,
    size = 18,
    fill = false,
    weight = 400,
  }: {
    name: IconName;
    size?: number;
    fill?: boolean;
    /// 400 for most things; 500-600 when a glyph sits next to bold text.
    weight?: 300 | 400 | 500 | 600 | 700;
  } = $props();
</script>

<span
  class="msr"
  class:fill
  style="font-size: {size}px; --icon-weight: {weight};"
  aria-hidden="true">{name}</span
>

<style>
  .msr {
    font-family: "Material Symbols Rounded";
    font-weight: normal;
    font-style: normal;
    line-height: 1;
    letter-spacing: normal;
    text-transform: none;
    display: inline-block;
    white-space: nowrap;
    word-wrap: normal;
    direction: ltr;
    -webkit-font-feature-settings: "liga";
    font-feature-settings: "liga";
    -webkit-font-smoothing: antialiased;
    flex-shrink: 0;
    font-variation-settings:
      "FILL" 0,
      "wght" var(--icon-weight, 400),
      "GRAD" 0,
      "opsz" 24;
  }
  .msr.fill {
    font-variation-settings:
      "FILL" 1,
      "wght" var(--icon-weight, 400),
      "GRAD" 0,
      "opsz" 24;
  }
</style>
