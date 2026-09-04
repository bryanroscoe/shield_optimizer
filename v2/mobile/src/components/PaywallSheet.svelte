<script lang="ts">
  // The one Pro upsell surface (design §6.3). Screens open this when a Pro
  // command throws `LOCKED:<feature>` instead of showing a raw error. "Enter
  // license key" routes to More, where the real activate_license flow lives.
  import type { Screen } from "../lib/router.svelte";

  let {
    open,
    navigate,
    onClose,
  }: {
    open: boolean;
    navigate: (screen: Screen) => void;
    onClose: () => void;
  } = $props();

  const features: { icon: string; title: string; desc: string }[] = [
    {
      icon: "auto_fix_high",
      title: "Curated debloat & one-tap optimize",
      desc: "Safely disable or uninstall known TV bloat.",
    },
    {
      icon: "photo_camera_back",
      title: "Snapshots",
      desc: "Record which packages are disabled, plus the launcher and tracked settings — then re-apply that set to this TV later.",
    },
    {
      icon: "home",
      title: "Launcher takeover",
      desc: "Set a custom launcher and disable the stock one.",
    },
    {
      icon: "tune",
      title: "Write tweaks — CEC, frame rate, DNS",
      desc: "Tune low-level system settings.",
    },
  ];
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="paywall-overlay" onclick={onClose}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="paywall-card" onclick={(e) => e.stopPropagation()}>
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <span class="paywall-close msr" onclick={onClose}>close</span>
      <div class="paywall-header">
        <span class="logo"><span class="msr">bolt</span></span>
        <h2>Unlock ATV Optimizer Pro</h2>
        <p class="paywall-lede">One payment · every future update.</p>
      </div>
      <div class="paywall-features">
        {#each features as f (f.title)}
          <div class="feature-row">
            <span class="msr teal-color">{f.icon}</span>
            <div class="feature-info">
              <span class="feature-title">{f.title}</span>
              <span class="feature-desc">{f.desc}</span>
            </div>
          </div>
        {/each}
      </div>
      <button class="primary" onclick={() => { onClose(); navigate("more"); }}>
        Enter license key
      </button>
      <button class="ghost paywall-later" onclick={onClose}>Maybe later</button>
      <p class="paywall-hint">Already have a key? Enter it in More.</p>
    </div>
  </div>
{/if}

<style>
  .paywall-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.85);
    display: grid;
    place-items: center;
    padding: 24px;
    z-index: 400;
  }
  .paywall-card {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 26px;
    width: 100%;
    max-width: 340px;
    padding: 24px;
    position: relative;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.8);
    display: flex;
    flex-direction: column;
    align-items: center;
    box-sizing: border-box;
    gap: 4px;
    animation: paywallIn 0.2s ease-out;
  }
  .paywall-close {
    position: absolute;
    top: 18px;
    right: 18px;
    font-size: 22px;
    color: var(--muted);
    cursor: pointer;
  }
  .paywall-header {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 8px;
    margin-bottom: 20px;
  }
  .paywall-header .logo {
    width: 48px;
    height: 48px;
    border-radius: 14px;
    background: var(--accent);
    display: grid;
    place-items: center;
  }
  .paywall-header .logo .msr {
    font-size: 28px;
    color: var(--accent-ink);
  }
  .paywall-header h2 {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
  }
  .paywall-lede {
    margin: 0;
    font-size: 13px;
    color: var(--muted);
  }
  .paywall-features {
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: 100%;
    margin-bottom: 20px;
  }
  .feature-row {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  .feature-row .msr {
    font-size: 20px;
    flex-shrink: 0;
  }
  .teal-color {
    color: var(--teal);
  }
  .feature-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .feature-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
  }
  .feature-desc {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.4;
  }
  .paywall-later {
    width: 100%;
    min-height: 50px;
    margin-top: 10px;
  }
  .paywall-hint {
    margin: 10px 0 0;
    font-size: 11px;
    color: var(--muted);
    text-align: center;
  }
  @keyframes paywallIn {
    from {
      transform: scale(0.94);
      opacity: 0;
    }
    to {
      transform: scale(1);
      opacity: 1;
    }
  }
</style>
