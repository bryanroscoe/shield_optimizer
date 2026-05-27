<script lang="ts">
  import { page } from "$app/stores";
  let { children } = $props();
</script>

<div class="app">
  <header>
    <div class="brand">
      <span class="logo-dot"></span>
      <span class="title">Shield Optimizer</span>
      <span class="version">v2</span>
    </div>
    <nav>
      <a href="/" class:active={$page.url.pathname === "/"}>Devices</a>
      <a href="/snapshots" class:active={$page.url.pathname.startsWith("/snapshots")}>
        Snapshots
      </a>
    </nav>
  </header>
  <main>
    {@render children?.()}
  </main>
  <footer>
    <span class="muted">v1 (PowerShell) is still supported. See repo README.</span>
  </footer>
</div>

<style>
  :global(html, body) {
    margin: 0;
    padding: 0;
    height: 100vh;
    font-family:
      -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Oxygen, Ubuntu,
      Cantarell, sans-serif;
    background: #0e1116;
    color: #e6edf3;
  }
  :global(*) {
    box-sizing: border-box;
  }
  :global(a) {
    color: #58a6ff;
    text-decoration: none;
  }
  :global(a:hover) {
    text-decoration: underline;
  }
  :global(button) {
    background: #21262d;
    color: #e6edf3;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 0.4rem 0.9rem;
    cursor: pointer;
    font-size: 0.9rem;
    font-family: inherit;
  }
  :global(button:hover) {
    background: #30363d;
  }
  :global(button.primary) {
    background: #1f6feb;
    border-color: #1f6feb;
  }
  :global(button.primary:hover) {
    background: #388bfd;
  }
  :global(button.danger) {
    background: #da3633;
    border-color: #da3633;
  }
  :global(button.danger:hover) {
    background: #f85149;
  }
  :global(input, select) {
    background: #0d1117;
    color: #e6edf3;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 0.45rem 0.7rem;
    font-size: 0.9rem;
    font-family: inherit;
  }
  :global(.muted) {
    color: #7d8590;
  }
  :global(.risk-safe) { color: #3fb950; }
  :global(.risk-medium) { color: #d29922; }
  :global(.risk-high) { color: #f85149; }
  :global(.risk-advanced) { color: #a371f7; }

  .app {
    display: grid;
    grid-template-rows: auto 1fr auto;
    min-height: 100vh;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.8rem 1.5rem;
    border-bottom: 1px solid #30363d;
    background: #161b22;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    font-weight: 600;
  }
  .logo-dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #58a6ff;
    box-shadow: 0 0 8px #58a6ff80;
  }
  .title {
    font-size: 1.05rem;
  }
  .version {
    color: #7d8590;
    font-weight: 400;
    font-size: 0.85rem;
  }
  nav {
    display: flex;
    gap: 1.2rem;
  }
  nav a {
    color: #c9d1d9;
    font-size: 0.92rem;
    padding: 0.3rem 0.5rem;
    border-radius: 4px;
  }
  nav a.active {
    color: #58a6ff;
    background: #1f2937;
  }
  main {
    padding: 1.5rem;
    max-width: 1100px;
    width: 100%;
    margin: 0 auto;
  }
  footer {
    padding: 0.8rem 1.5rem;
    border-top: 1px solid #30363d;
    font-size: 0.82rem;
    text-align: center;
  }
</style>
