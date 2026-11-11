<script lang="ts">
  import Preview from "./EntitlementPreview.svelte";
  import { initialPreview, reducePreview, type Event } from "./entitlementPreview";
  let { recovered }: { recovered: () => void } = $props();
  let state = $state(initialPreview());
  export function dispatch(event: Event) { state = reducePreview(state, event); }
</script>
<Preview {state} onBuy={() => dispatch({ type: "begin", request: state.request + 1, operation: "buy" })} onRestore={() => dispatch({ type: "begin", request: state.request + 1, operation: "restore" })} onRecovery={recovered} />
