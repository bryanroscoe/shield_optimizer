<script lang="ts">
  // The app's brand mark — the same glyph as the launcher icon (a TV outline on
  // two feet with three vertical faders inside), so the in-app logo matches the icon.
  let { size = 30 }: { size?: number } = $props();
  const gradientId = $props.id();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 120 120"
  role="img"
  aria-label="ATV Optimizer"
  style="display:block;border-radius:{size * 0.23}px"
>
  <defs>
    <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#DCFF6E" />
      <stop offset="1" stop-color="#B9E640" />
    </linearGradient>
  </defs>
  <rect width="120" height="120" rx="28" fill="url(#{gradientId})" />
  <g fill="#0E1512">
    <path d="M34 83 L26 96 L33 96 L41 83 Z" />
    <path d="M86 83 L94 96 L87 96 L79 83 Z" />
  </g>
  <rect x="21.5" y="30.5" width="77" height="51" rx="10.5" fill="none" stroke="#0E1512" stroke-width="7" />
  <g stroke="#0E1512" stroke-width="3" stroke-linecap="round">
    <path d="M43 40V72" />
    <path d="M60 40V72" />
    <path d="M77 40V72" />
  </g>
  <g fill="#0E1512">
    <rect x="36.5" y="59" width="13" height="10" rx="5" />
    <rect x="53.5" y="42" width="13" height="10" rx="5" />
    <rect x="70.5" y="51" width="13" height="10" rx="5" />
  </g>
</svg>
