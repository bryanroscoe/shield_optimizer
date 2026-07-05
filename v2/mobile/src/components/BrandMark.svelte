<script lang="ts">
  // The app's brand mark — the same glyph as the launcher icon (a TV/monitor on
  // a stand with three vertical faders), so the in-app logo matches the icon.
  let { size = 30 }: { size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 120 120"
  role="img"
  aria-label="ATV Optimizer"
  style="display:block;border-radius:{size * 0.23}px"
>
  <rect width="120" height="120" rx="28" fill="#C9F24E" />
  <g fill="#0B0D10">
    <rect x="18" y="22" width="84" height="58" rx="14" />
    <path d="M34 80 L26 91 L33 91 L41 80 Z" />
    <path d="M86 80 L94 91 L87 91 L79 80 Z" />
  </g>
  <rect x="27" y="30" width="66" height="42" rx="8" fill="#C9F24E" />
  <g fill="#0B0D10">
    <rect x="42.75" y="34" width="2.5" height="34" rx="1.25" />
    <rect x="58.75" y="34" width="2.5" height="34" rx="1.25" />
    <rect x="74.75" y="34" width="2.5" height="34" rx="1.25" />
    <rect x="37" y="48" width="14" height="9" rx="4.5" />
    <rect x="53" y="37" width="14" height="9" rx="4.5" />
    <rect x="69" y="55" width="14" height="9" rx="4.5" />
  </g>
</svg>
