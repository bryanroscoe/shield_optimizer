<script lang="ts">
  let {
    message,
    type = "info",
  }: { message: string; type?: "success" | "error" | "info" } = $props();
</script>

{#if message}
  <div class="toast" class:error={type === "error"} class:success={type === "success"}>
    <span class="msr">
      {type === "success" ? "check_circle" : type === "error" ? "error" : "info"}
    </span>
    <span>{message}</span>
  </div>
{/if}

<style>
  .toast {
    position: fixed;
    bottom: calc(env(safe-area-inset-bottom) + 96px);
    left: 24px;
    right: 24px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 12px 16px;
    color: var(--text);
    font-size: 13px;
    display: flex;
    align-items: center;
    gap: 10px;
    box-shadow: 0 8px 20px rgba(0, 0, 0, 0.5);
    z-index: 300;
    animation: toastUp 0.3s ease-out;
  }
  .toast .msr {
    font-size: 20px;
    color: var(--muted);
    flex: none;
  }
  .toast.success {
    border-color: var(--teal);
    background: color-mix(in srgb, var(--teal) 8%, var(--surface-2));
  }
  .toast.success .msr {
    color: var(--teal);
  }
  .toast.error {
    border-color: var(--danger);
    background: color-mix(in srgb, var(--danger) 8%, var(--surface-2));
  }
  .toast.error .msr {
    color: var(--danger);
  }
  @keyframes toastUp {
    from {
      transform: translateY(20px);
      opacity: 0;
    }
    to {
      transform: translateY(0);
      opacity: 1;
    }
  }
</style>
