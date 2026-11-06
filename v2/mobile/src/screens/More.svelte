<script module lang="ts">
  export type NoticeInline =
    | { kind: "text" | "code"; text: string }
    | { kind: "link"; text: string; href: string };
  export type NoticeBlock =
    | { kind: "heading"; level: number; inline: NoticeInline[] }
    | { kind: "paragraph"; inline: NoticeInline[] }
    | { kind: "list"; ordered: boolean; items: NoticeInline[][] }
    | { kind: "code"; text: string }
    | { kind: "table"; head: NoticeInline[][] | null; rows: NoticeInline[][][] };

  // Off applies at once even when saving it fails, but then reports come back
  // on at the next launch. Say so and offer a retry rather than a plain "Off".
  // Module state, so the warning survives leaving More and coming back.
  let telemetryOffUnsaved = $state(false);

  const NOTICE_INLINE = /`([^`]+)`|\[([^\]]+)\]\(([^)\s]+)\)|<(https?:\/\/[^>\s]+)>/g;

  function noticeInline(text: string): NoticeInline[] {
    const parts: NoticeInline[] = [];
    let last = 0;
    for (const m of text.matchAll(NOTICE_INLINE)) {
      const at = m.index ?? 0;
      if (at > last) parts.push({ kind: "text", text: text.slice(last, at) });
      if (m[1] !== undefined) {
        parts.push({ kind: "code", text: m[1] });
      } else {
        const label = (m[2] ?? m[4]).replaceAll("`", "");
        const href = m[3] ?? m[4];
        parts.push(/^https?:\/\//i.test(href) ? { kind: "link", text: label, href } : { kind: "text", text: label });
      }
      last = at + m[0].length;
    }
    if (last < text.length) parts.push({ kind: "text", text: text.slice(last) });
    return parts;
  }

  function tableCells(line: string): string[] {
    return line.trim().replace(/^\|/, "").replace(/\|$/, "").split("|").map((c) => c.trim());
  }

  /// The generated third-party notices are Markdown with a little HTML (a
  /// generator comment, <details>/<summary>). This only splits them into
  /// blocks of plain text; the template renders every block as text, so
  /// nothing in the document can become markup. Generator comments and
  /// maintainer instructions are dropped.
  export function parseNotices(markdown: string): NoticeBlock[] {
    const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
    const blocks: NoticeBlock[] = [];
    let paragraph: string[] = [];
    const flush = () => {
      const text = paragraph.join(" ").trim();
      paragraph = [];
      if (text && !/^Regenerate this document\b/i.test(text)) blocks.push({ kind: "paragraph", inline: noticeInline(text) });
    };
    for (let i = 0; i < lines.length; i++) {
      let line = lines[i];
      const fence = line.match(/^\s*(`{3,}|~{3,})/);
      if (fence) {
        flush();
        const code: string[] = [];
        while (++i < lines.length && !lines[i].trimStart().startsWith(fence[1])) code.push(lines[i]);
        blocks.push({ kind: "code", text: code.join("\n") });
        continue;
      }
      if (line.includes("<!--")) {
        line = line.replace(/<!--[\s\S]*?-->/g, "");
        const open = line.indexOf("<!--");
        if (open >= 0) {
          // A comment that runs onto later lines.
          let end = i + 1;
          while (end < lines.length && !lines[end].includes("-->")) end++;
          line = line.slice(0, open) + (end < lines.length ? lines[end].slice(lines[end].indexOf("-->") + 3) : "");
          i = end;
        }
      }
      const trimmed = line.trim();
      if (!trimmed) {
        flush();
        continue;
      }
      const summary = trimmed.match(/^<summary>(.*?)(?:<\/summary>)?$/i);
      if (summary) {
        flush();
        if (summary[1].trim()) blocks.push({ kind: "heading", level: 4, inline: noticeInline(summary[1].trim()) });
        continue;
      }
      if (/^<\/?[a-z][a-z0-9-]*(\s[^>]*)?>$/i.test(trimmed)) {
        flush();
        continue;
      }
      const heading = trimmed.match(/^(#{1,6})\s+(.*?)(?:\s+#+)?$/);
      if (heading) {
        flush();
        blocks.push({ kind: "heading", level: Math.min(heading[1].length, 4), inline: noticeInline(heading[2]) });
        continue;
      }
      if (trimmed.startsWith("|")) {
        flush();
        const rows: string[][] = [];
        for (; i < lines.length && lines[i].trim().startsWith("|"); i++) rows.push(tableCells(lines[i]));
        i--;
        const divider = rows.length > 1 && rows[1].every((c) => /^:?-{3,}:?$/.test(c));
        blocks.push({
          kind: "table",
          head: divider ? rows[0].map(noticeInline) : null,
          rows: rows.slice(divider ? 2 : 0).map((r) => r.map(noticeInline)),
        });
        continue;
      }
      const item = trimmed.match(/^([-*+]|\d+[.)])\s+(.*)$/);
      if (item) {
        flush();
        const ordered = /\d/.test(item[1]);
        const items: NoticeInline[][] = [];
        for (; i < lines.length; i++) {
          const next = lines[i].trim().match(/^([-*+]|\d+[.)])\s+(.*)$/);
          if (!next || /\d/.test(next[1]) !== ordered) break;
          items.push(noticeInline(next[2]));
        }
        i--;
        blocks.push({ kind: "list", ordered, items });
        continue;
      }
      paragraph.push(trimmed);
    }
    flush();
    return blocks;
  }
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { session } from "../lib/session.svelte";
  import { router } from "../lib/router.svelte";
  import { frontendLogText } from "../lib/log";
  import {
    clearUnknownDiagnostics,
    exportUnknownDiagnosticsSnapshot,
    getUnknownDiagnosticsSnapshot,
    unknownDiagnosticsError,
  } from "../lib/unknownDiagnostics";
  import { api } from "../lib/api";
  import { billing, FREE_RECOVERY_PROMISE } from "../lib/billing.svelte";
  import type { Screen } from "../lib/router.svelte";
  import type { LicenseInfo, RebootMode, RecoveryResult } from "../lib/types";
  import { MOBILE_NOTICE_TEXT, TOGGLE_LABEL, type TelemetrySettings } from "../../../shared/telemetry";
  import type {
    UnknownDiagnosticReason,
    UnknownDiagnosticReport,
  } from "../lib/unknownDiagnostics";
  import BottomTabs from "../components/BottomTabs.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import ErrorNotice from "../components/ErrorNotice.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import Toast from "../components/Toast.svelte";

  let { onDisconnect, navigate }: {
    onDisconnect: () => void;
    navigate: (screen: Screen) => void;
  } = $props();

  let licenseKey = $state("");
  let activating = $state(false);
  let licenseError = $state("");
  let license = $state<LicenseInfo | null>(null);

  let toast = $state("");
  let toastType = $state<"success" | "error" | "info">("info");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function showToast(msg: string, type: "success" | "error" | "info" = "info") {
    toast = msg;
    toastType = type;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 3500);
  }

  // Anonymous reports. null until the backend answers; a failed read leaves
  // the switch hidden rather than showing a state we don't know.
  let telemetry = $state<TelemetrySettings | null>(null);
  let telemetryBusy = $state(false);
  const PRIVACY_POLICY_URL = "https://atv-optimizer-telemetry.bryanroscoe.workers.dev/privacy";

  async function openPrivacyPolicy() {
    try {
      await api.openUrl(PRIVACY_POLICY_URL);
    } catch {
      showToast("Couldn't open the privacy policy. Please try again.", "error");
    }
  }

  async function copySupportId() {
    const id = telemetry?.support_id;
    if (!id) return;
    try {
      await navigator.clipboard.writeText(id);
      showToast("Support ID copied.", "success");
    } catch {
      showToast("Couldn't copy. You can select the Support ID below.", "error");
    }
  }

  async function loadTelemetry() {
    try {
      telemetry = await api.telemetrySettings();
    } catch {
      telemetry = null;
    }
  }

  async function setTelemetry(enabled: boolean) {
    telemetryBusy = true;
    try {
      telemetry = await api.setTelemetryEnabled(enabled);
      telemetryOffUnsaved = false;
    } catch {
      await loadTelemetry();
      telemetryOffUnsaved = !enabled && telemetry?.enabled === false;
      if (!telemetryOffUnsaved) showToast("Couldn't change the report setting.", "error");
    } finally {
      telemetryBusy = false;
    }
  }

  function toggleTelemetry(event: Event) {
    void setTelemetry((event.currentTarget as HTMLInputElement).checked);
  }

  onMount(async () => {
    void loadTelemetry();
    refreshDiagnosticsSummary();
    await session.loadEntitlement();
    await billing.load();
    await loadLicense();
    void billing.refresh();
  });

  // Where Pro comes from. The two sources are independent: a Play refund never
  // removes a license key, and vice versa.
  const proSource = $derived.by(() => {
    const sources = billing.status?.sources ?? [];
    if (sources.includes("play") && sources.includes("license")) return "Google Play purchase and license key";
    if (sources.includes("play")) return "Google Play purchase";
    if (sources.includes("license")) return "License key";
    return "";
  });

  const playConfirmed = $derived.by(() => {
    const play = billing.status?.play;
    if (play?.state !== "owned" || !play.cached) return "";
    const d = new Date(play.verified_at_ms);
    return Number.isNaN(d.getTime())
      ? ""
      : `Google Play couldn't be reached; last confirmed ${d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" })}.`;
  });

  // Details of the signed license behind Pro. Null on Free, and null on Pro if
  // the backend has no record (an older persisted activation, for instance) —
  // the card falls back to the plain thank-you line in that case.
  async function loadLicense() {
    try {
      license = await api.licenseInfo();
    } catch {
      license = null;
    }
  }

  // Pro activation goes through the backend's activate_license and updates the
  // shared entitlement.
  async function activate() {
    const key = licenseKey.trim();
    if (!key || activating) return;
    activating = true;
    licenseError = "";
    try {
      const ent = await session.activatePro(key);
      if (ent === "pro") {
        showToast("Pro unlocked — thank you!", "success");
        licenseKey = "";
        // The key is now a Pro source; without a fresh status the source line
        // and license details stay hidden until the next resume.
        await Promise.all([loadLicense(), billing.load()]);
      } else {
        licenseError = "That key wasn't accepted. Check and try again.";
        showToast(licenseError, "error");
      }
    } catch (e) {
      // The backend rejects a key with the verifier's user-facing reason
      // (malformed / bad signature / expired / unknown key) — show it verbatim
      // rather than a generic failure.
      licenseError = String(e).replace(/^.*Error:\s*/, "");
      showToast(licenseError, "error");
    } finally {
      activating = false;
    }
  }

  function formatDate(iso: string): string {
    const d = new Date(`${iso}T00:00:00`);
    return Number.isNaN(d.getTime())
      ? iso
      : d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
  }

  // Detail screens reachable from here, grouped by what they touch. Pro tools
  // show a lock for free users; the screen itself still handles the LOCKED
  // paywall on any write.
  type Tool = { screen: Screen; icon: string; title: string; desc: string; pro: boolean };
  const toolGroups: { label: string; tools: Tool[] }[] = [
    {
      label: "Your TV",
      tools: [
        { screen: "devices", icon: "devices_other", title: "Devices", desc: "Switch, rename, or add a TV", pro: false },
        { screen: "files", icon: "sync_alt", title: "Files", desc: "Browse the TV and copy files to this phone", pro: false },
        { screen: "backups", icon: "cloud_sync", title: "Backups", desc: "Back up and restore an app's APK", pro: false },
      ],
    },
    {
      label: "Tuning",
      tools: [
        { screen: "optimize", icon: "auto_fix_high", title: "Optimize", desc: "Review and turn off apps you don't use", pro: true },
        { screen: "launcher", icon: "home", title: "Launcher", desc: "Set a custom home screen", pro: true },
        { screen: "snapshots", icon: "photo_camera_back", title: "Snapshots", desc: "Record a setup and re-apply it later", pro: true },
      ],
    },
    {
      label: "Help",
      tools: [
        { screen: "catalog", icon: "search", title: "Device & package catalog", desc: "Browse source profiles and package observations", pro: false },
        { screen: "riskguide", icon: "help", title: "Safety tiers & actions", desc: "What each tier and action means", pro: false },
      ],
    },
  ];

  // Emergency recovery: `pm enable` every disabled package. This is the undo
  // for a debloat that went too far; it never uninstalls or installs anything.
  let recoveryConfirm = $state(false);
  let recovering = $state(false);
  let recovery = $state<RecoveryResult | null>(null);

  async function runRecovery() {
    recoveryConfirm = false;
    if (recovering || !session.serial) return;
    recovering = true;
    recovery = null;
    try {
      recovery = await api.panicRecovery(session.serial);
      session.invalidateAll();
      showToast(recovery.message || `Re-enabled ${recovery.restored.length} apps.`, recovery.failed.length ? "info" : "success");
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      recovering = false;
    }
  }

  // Advanced reboot targets. Recovery and bootloader are for troubleshooting;
  // both drop the ADB connection.
  let rebootTarget = $state<RebootMode | null>(null);
  let rebooting = $state(false);
  const rebootLabels: Record<RebootMode, string> = {
    normal: "Restart",
    recovery: "Recovery mode",
    bootloader: "Bootloader",
  };

  async function runReboot() {
    const mode = rebootTarget;
    rebootTarget = null;
    if (!mode || rebooting || !session.serial) return;
    rebooting = true;
    const generation = session.generation;
    try {
      const r = await api.rebootDevice(session.serial, mode);
      showToast(r.ok ? `${rebootLabels[mode]} command sent.` : r.message || "Reboot failed.", r.ok ? "success" : "error");
      if (r.ok && await session.finishReboot(generation)) router.reset("onboarding");
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      rebooting = false;
    }
  }

  let disconnectConfirm = $state(false);

  // Third-party notices are a few hundred KB, so the module is imported only
  // when the user opens the card.
  let showNotices = $state(false);
  let notices = $state<NoticeBlock[] | null>(null);
  let noticesError = $state("");
  let noticesLoading = $state(false);
  async function toggleNotices() {
    showNotices = !showNotices;
    if (!showNotices || notices) return;
    noticesLoading = true;
    noticesError = "";
    try {
      const mod = await import("../lib/notices.generated");
      notices = parseNotices(mod.THIRD_PARTY_NOTICES);
    } catch (e) {
      noticesError = `Couldn't load notices: ${String(e)}`;
    } finally {
      noticesLoading = false;
    }
  }

  async function openNoticeLink(event: MouseEvent, href: string) {
    event.preventDefault();
    try {
      await api.openUrl(href);
    } catch {
      showToast("Couldn't open the link. Please try again.", "error");
    }
  }

  // Debug log — the native tail plus the frontend call ring buffer, both
  // copyable for support.
  let showLog = $state(false);
  let nativeLog = $state("");
  // The frontend ring buffer isn't reactive, so it's read on Show, Refresh and
  // Copy rather than once.
  let frontendLog = $state("");
  let logLoading = $state(false);
  let logError = $state("");

  async function loadLog() {
    showLog = true;
    logLoading = true;
    logError = "";
    try {
      nativeLog = await api.readDebugLog();
    } catch (e) {
      logError = String(e);
    } finally {
      frontendLog = frontendLogText();
      logLoading = false;
    }
  }

  const combinedLog = $derived(
    `# Frontend calls\n${frontendLog}\n\n# Native log\n${nativeLog}`,
  );

  async function copyLog() {
    frontendLog = frontendLogText();
    try {
      await navigator.clipboard.writeText(combinedLog);
      showToast("Debug log copied.", "success");
    } catch {
      showToast("Couldn't copy to clipboard.", "error");
    }
  }

  let diagnosticsCount = $state(0);
  let diagnosticsStorageError = $state("");
  let diagnosticsPreview = $state<UnknownDiagnosticReport | null>(null);
  let diagnosticsJson = $state("");
  let diagnosticsCopyFailed = $state(false);
  let diagnosticsClearConfirm = $state(false);

  function refreshDiagnosticsSummary() {
    const snapshot = getUnknownDiagnosticsSnapshot();
    diagnosticsCount = snapshot.records.length;
    diagnosticsStorageError = unknownDiagnosticsError();
  }

  function takeDiagnosticsSnapshot() {
    diagnosticsPreview = getUnknownDiagnosticsSnapshot();
    diagnosticsJson = exportUnknownDiagnosticsSnapshot(diagnosticsPreview);
    diagnosticsCount = diagnosticsPreview.records.length;
    diagnosticsStorageError = unknownDiagnosticsError();
    diagnosticsCopyFailed = false;
  }

  function reasonLabel(reason: UnknownDiagnosticReason): string {
    switch (reason) {
      case "uncatalogued_package":
        return "Not in the reviewed app catalog";
      case "unknown_safety_classification":
        return "Removal safety is unknown";
      case "safety_lookup_unavailable":
        return "Safety lookup was unavailable";
      case "process_not_resolved":
        return "Process couldn't be matched to an installed app";
    }
  }

  function recordedAt(iso: string): string {
    const date = new Date(iso);
    return Number.isNaN(date.getTime()) ? iso : date.toLocaleString();
  }

  async function copyDiagnostics() {
    if (!diagnosticsPreview) return;
    try {
      await navigator.clipboard.writeText(diagnosticsJson);
      diagnosticsCopyFailed = false;
      showToast("Diagnostic JSON copied.", "success");
    } catch {
      diagnosticsCopyFailed = true;
      showToast("Couldn't copy. Select the JSON below instead.", "error");
    }
  }

  function clearDiagnostics() {
    diagnosticsClearConfirm = false;
    const result = clearUnknownDiagnostics();
    if (!result.ok) {
      diagnosticsStorageError = result.message ?? "Couldn't clear diagnostics stored on this device.";
      showToast(diagnosticsStorageError, "error");
      return;
    }
    diagnosticsCount = 0;
    diagnosticsPreview = null;
    diagnosticsJson = "";
    diagnosticsStorageError = "";
    diagnosticsCopyFailed = false;
    showToast("Local diagnostics cleared.", "success");
  }
</script>

{#snippet noticeText(parts: NoticeInline[])}{#each parts as part, i (i)}{#if part.kind === "code"}<code>{part.text}</code>{:else if part.kind === "link"}<a class="notice-link" href={part.href} rel="noopener noreferrer" onclick={(e) => openNoticeLink(e, part.href)}>{part.text}</a>{:else}{part.text}{/if}{/each}{/snippet}

<div class="screen">
  <div class="topline">
    <div class="header-left">
      <FindRemoteButton />
    </div>
    <h3 class="header-title">More</h3>
  </div>

  <div class="more-content">
    <!-- Licensing -->
    <div class="more-card">
      <span class="card-label">Licensing</span>
      <div class="license-status-row">
        <span class="license-title">Current plan:</span>
        <span class="license-badge" class:pro={session.isPro}>{session.isPro ? "Pro" : "Free"}</span>
      </div>

      {#if session.isPro && proSource}
        <p class="license-source">Source: {proSource}</p>
      {/if}

      {#if !session.isPro && billing.available}
        <p class="license-desc">Pro unlocks the debloat plan, snapshots, custom launchers and tweaks. One-time purchase through Google Play.</p>
        {#if billing.pending}
          <p class="license-desc" role="status">Purchase pending. Pro unlocks as soon as Google Play confirms the payment.</p>
        {/if}
        <div class="billing-actions">
          <button
            class="primary small-btn"
            disabled={!billing.status?.offer || billing.purchasing || billing.restoring || billing.pending}
            onclick={() => billing.purchase()}
          >
            {#if billing.purchasing}
              Waiting for Google Play…
            {:else if billing.status?.offer}
              Buy Pro · {billing.status.offer.formatted_price}
            {:else}
              Price unavailable
            {/if}
          </button>
          <button class="ghost small-btn" disabled={billing.purchasing || billing.restoring} onclick={() => billing.restore()}>
            {billing.restoring ? "Checking…" : "Restore purchases"}
          </button>
        </div>
      {:else if !session.isPro && !billing.licenseKeyEntry}
        <p class="license-desc">{billing.unavailableReason}</p>
      {/if}
      {#if billing.message && billing.tone === "error"}
        <ErrorNotice
          error={billing.message}
          guidance={billing.message}
          title="Google Play"
          action="Google Play purchase"
          support={null}
          onDismiss={() => (billing.message = "")}
        />
      {:else if billing.message}
        <p class="license-desc" role="status">{billing.message}</p>
      {/if}
      {#if playConfirmed}
        <p class="license-desc">{playConfirmed}</p>
      {/if}
      {#if !session.isPro}
        <p class="license-desc">{FREE_RECOVERY_PROMISE}</p>
      {/if}
      {#if billing.status?.ack_pending}
        <p class="license-desc">Finishing the purchase with Google Play. This retries automatically.</p>
      {/if}

      {#if !session.isPro && billing.licenseKeyEntry}
        <p class="license-desc">Have a license key? Activate Pro with it here.</p>
        <div class="license-input-row">
          <input
            type="text"
            bind:value={licenseKey}
            placeholder="ATVOPT-…"
            class="license-input mono"
            autocapitalize="characters"
            autocorrect="off"
            spellcheck="false"
            onkeydown={(e) => e.key === "Enter" && activate()}
          />
          <button class="primary small-btn" disabled={!licenseKey || activating} onclick={activate}>
            {activating ? "…" : "Activate"}
          </button>
        </div>
        {#if licenseError}
          <p class="license-error">{licenseError}</p>
        {/if}
      {:else if session.isPro}
        <p class="license-desc success-color">Thank you for supporting ATV Optimizer Pro!</p>
        {#if license && billing.status?.sources?.includes("license") !== false}
          <dl class="license-details">
            <dt>Licensed to</dt>
            <dd class="mono">{license.licensee}</dd>
            <dt>Term</dt>
            <dd>{license.expires ? `Expires ${formatDate(license.expires)}` : "Perpetual"}</dd>
          </dl>
        {/if}
      {/if}
    </div>

    {#each toolGroups as group (group.label)}
      <div class="more-card">
        <span class="card-label">{group.label}</span>
        {#each group.tools as t (t.screen)}
          <button class="setting-row" onclick={() => navigate(t.screen)}>
            <span class="msr tool-icon">{t.icon}</span>
            <div class="setting-info">
              <span class="setting-title">{t.title}</span>
            </div>
            {#if t.pro && !session.isPro}
              <span class="msr lock-icon">lock</span>
            {:else}
              <span class="msr chevron">chevron_right</span>
            {/if}
          </button>
        {/each}
      </div>
    {/each}

    <!-- Recovery -->
    <div class="more-card">
      <span class="card-label">Recovery</span>
      <p class="card-desc">
        Re-enables every app that is currently disabled on the TV, including ones this app
        didn't touch. Use it if something stopped working after a debloat. It never uninstalls
        or installs anything.
      </p>
      <button class="ghost-btn" disabled={recovering || !session.connectedDevice} onclick={() => (recoveryConfirm = true)}>
        {#if recovering}
          <span class="pdot blink"></span>Re-enabling apps…
        {:else}
          <span class="msr">medical_services</span>Emergency recovery
        {/if}
      </button>
      {#if recovery}
        <div class="recovery-result">
          <span class="mono">{recovery.restored.length} re-enabled · {recovery.failed.length} failed</span>
          {#if recovery.failed.length}
            <ul class="recovery-failed mono">
              {#each recovery.failed as f (f.package)}
                <li>{f.package}: {f.error}</li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Reboot -->
    <div class="more-card">
      <span class="card-label">Reboot</span>
      <p class="card-desc">Every option restarts the TV and drops this connection.</p>
      <div class="reboot-row">
        {#each ["normal", "recovery", "bootloader"] as mode (mode)}
          <button class="ghost-btn" disabled={rebooting || !session.connectedDevice} onclick={() => (rebootTarget = mode as RebootMode)}>
            <span class="msr">{mode === "normal" ? "restart_alt" : mode === "recovery" ? "build" : "developer_board"}</span>{rebootLabels[mode as RebootMode]}
          </button>
        {/each}
      </div>
    </div>

    <div class="more-card">
      <span class="card-label">Unknown app diagnostics</span>
      <p class="card-desc">
        Keeps a small local record of apps we haven't reviewed and processes we couldn't match to
        an app. Nothing is sent automatically.
      </p>
      {#if diagnosticsCount === 0}
        <p class="diagnostics-empty">Nothing recorded yet.</p>
      {:else}
        <p class="diagnostics-count">
          <strong>{diagnosticsCount}</strong> distinct {diagnosticsCount === 1 ? "record" : "records"}.
          Repeated refreshes are grouped as observations. Only the most recent records are kept.
        </p>
      {/if}
      {#if diagnosticsStorageError}
        <p class="log-error mono">{diagnosticsStorageError}</p>
      {/if}
      <div class="log-toolbar">
        <button class="ghost-btn compact" disabled={diagnosticsCount === 0} onclick={takeDiagnosticsSnapshot}>
          <span class="msr">description</span>Review records
        </button>
        <button class="ghost-btn compact" disabled={diagnosticsCount === 0 && !diagnosticsStorageError} onclick={() => (diagnosticsClearConfirm = true)}>
          <span class="msr">delete</span>Clear records
        </button>
      </div>
      {#if diagnosticsPreview}
        <div class="diagnostics-review">
          <p class="card-desc">
            Includes app or process identifiers, observation times and version information. Review
            the contents before sharing. No network addresses, device serials, license keys or full
            debug logs are included. Copy diagnostic JSON puts text on your clipboard; it does not save a file.
          </p>
          <div class="log-toolbar">
            <button class="ghost-btn compact" onclick={takeDiagnosticsSnapshot}>
              <span class="msr">refresh</span>Refresh preview
            </button>
            <button class="ghost-btn compact" onclick={copyDiagnostics}>
              <span class="msr">content_copy</span>Copy diagnostic JSON
            </button>
          </div>
          {#each diagnosticsPreview.records as record (`${record.kind}:${record.token}:${record.first_seen}`)}
            <div class="diagnostic-record">
              <div class="diagnostic-record-title">
                <strong>{record.kind === "installed_package" ? "App not reviewed" : "Unmatched process"}</strong>
                <span class="mono">×{record.count}</span>
              </div>
              <span class="mono diagnostic-token">{record.token}</span>
              <span class="diagnostic-reason">{reasonLabel(record.reason)}</span>
              <dl class="diagnostic-meta">
                <dt>First recorded</dt><dd>{recordedAt(record.first_seen)}</dd>
                <dt>Last recorded</dt><dd>{recordedAt(record.last_seen)}</dd>
                <dt>App version</dt><dd class="mono">{record.app_version}</dd>
                <dt>Registry version</dt><dd class="mono">{record.registry_version ?? "Not available"}</dd>
                {#if record.device_family}
                  <dt>Device family</dt><dd>{record.device_family.replace("_", " ")}</dd>
                {/if}
                {#if record.device_os}
                  <dt>Device OS</dt><dd>{record.device_os}</dd>
                {/if}
              </dl>
            </div>
          {/each}
          {#if diagnosticsPreview.truncated}
            <p class="diagnostics-empty">Older records were removed to keep this report small.</p>
          {/if}
          {#if diagnosticsCopyFailed}
            <p class="log-error">Couldn't copy. You can select and copy the text below.</p>
          {/if}
          <pre class="log-view mono diagnostics-json">{diagnosticsJson}</pre>
        </div>
      {/if}
    </div>

    <!-- Anonymous reports -->
    <div class="more-card" data-testid="telemetry-card">
      <span class="card-label">Privacy</span>
      <button class="ghost-btn" onclick={openPrivacyPolicy}>Privacy policy<span class="msr">open_in_new</span></button>
      {#if telemetry}
        <label class="telemetry-toggle">
          <span class="tt-copy">
            <span class="tt-title">{TOGGLE_LABEL}</span>
            <span class="card-desc">
              {#if !telemetry.available}
                This build doesn't send reports.
              {:else if telemetry.enabled}
                On. Turning it off stops sending immediately and discards anything not yet sent.
              {:else}
                Off. Nothing is sent.
              {/if}
            </span>
          </span>
          <input
            type="checkbox"
            role="switch"
            checked={telemetry.available && telemetry.enabled}
            disabled={!telemetry.available || telemetryBusy}
            onchange={toggleTelemetry}
            aria-label={TOGGLE_LABEL}
          />
        </label>
        {#if telemetryOffUnsaved && telemetry.enabled === false}
          <div class="telemetry-unsaved" role="alert">
            <p class="card-desc">
              Reports are off for now, but this choice couldn't be saved. They may turn back on
              the next time the app starts.
            </p>
            <button class="ghost-btn compact" disabled={telemetryBusy} onclick={() => setTelemetry(false)}>
              <span class="msr">refresh</span>Try saving again
            </button>
          </div>
        {/if}
        <details class="help-disclosure">
          <summary>What's sent</summary>
          <p class="card-desc">{MOBILE_NOTICE_TEXT}</p>
          <p class="card-desc">
            Reports go to a server the app's developer runs; raw reports are kept for 90 days.
            Turning reports back on starts a new random install ID.
          </p>
        </details>
        {#if telemetry.support_id}
          <p class="card-desc">Support ID: <code class="support-id">{telemetry.support_id}</code></p>
          <button class="ghost-btn" onclick={copySupportId}>Copy Support ID</button>
          <p class="card-desc">Include this ID in a privacy or deletion request. Save it before turning reports off; turning them back on creates a new ID.</p>
        {/if}
      {/if}
    </div>

    <!-- Debug log -->
    <div class="more-card">
      <span class="card-label">Debug log</span>
      <p class="card-desc">Copy puts log text on your clipboard; it does not save a file.</p>
      <details class="help-disclosure"><summary>About debug logs</summary><p class="card-desc">Recent app activity and the device transport log — useful for support.</p></details>
      {#if !showLog}
        <button class="ghost-btn" onclick={loadLog}>
          <span class="msr">description</span>Show debug log
        </button>
      {:else}
        <div class="log-toolbar">
          <button class="ghost-btn compact" onclick={loadLog} disabled={logLoading}>
            <span class="msr" class:blink={logLoading}>refresh</span>Refresh
          </button>
          <button class="ghost-btn compact" onclick={copyLog}>
            <span class="msr">content_copy</span>Copy
          </button>
        </div>
        {#if logError}
          <p class="log-error mono">{logError}</p>
        {/if}
        <pre class="log-view mono">{combinedLog}</pre>
      {/if}
    </div>

    <!-- About -->
    <div class="more-card">
      <span class="card-label">About</span>
      <button class="ghost-btn" onclick={toggleNotices}>
        <span class="msr">{showNotices ? "expand_less" : "expand_more"}</span>
        {showNotices ? "Hide third-party notices" : "Third-party notices"}
      </button>
      {#if showNotices}
        {#if noticesLoading}
          <p class="card-desc">Loading…</p>
        {:else if noticesError}
          <p class="log-error mono">{noticesError}</p>
        {:else if notices}
          <div class="notices" data-testid="third-party-notices">
            {#each notices as block, i (i)}
              {#if block.kind === "heading"}
                {#if block.level === 1}
                  <h4 class="notice-h notice-h1">{@render noticeText(block.inline)}</h4>
                {:else if block.level === 2}
                  <h5 class="notice-h notice-h2">{@render noticeText(block.inline)}</h5>
                {:else}
                  <h6 class="notice-h notice-h3">{@render noticeText(block.inline)}</h6>
                {/if}
              {:else if block.kind === "paragraph"}
                <p class="notice-p">{@render noticeText(block.inline)}</p>
              {:else if block.kind === "list"}
                {#if block.ordered}
                  <ol class="notice-list">{#each block.items as item, j (j)}<li>{@render noticeText(item)}</li>{/each}</ol>
                {:else}
                  <ul class="notice-list">{#each block.items as item, j (j)}<li>{@render noticeText(item)}</li>{/each}</ul>
                {/if}
              {:else if block.kind === "code"}
                <pre class="notice-code mono">{block.text}</pre>
              {:else if block.kind === "table"}
                <table class="notice-table">
                  {#if block.head}
                    <thead><tr>{#each block.head as cell, j (j)}<th>{@render noticeText(cell)}</th>{/each}</tr></thead>
                  {/if}
                  <tbody>
                    {#each block.rows as row, j (j)}
                      <tr>{#each row as cell, k (k)}<td>{@render noticeText(cell)}</td>{/each}</tr>
                    {/each}
                  </tbody>
                </table>
              {/if}
            {/each}
          </div>
        {/if}
      {/if}
    </div>

    <!-- Disconnect -->
    <button class="ghost danger-btn" onclick={() => (disconnectConfirm = true)}>
      <span class="msr">power_settings_new</span>Disconnect from TV
    </button>
  </div>

  <ConfirmDialog
    open={recoveryConfirm}
    icon="medical_services"
    title="Re-enable every disabled app?"
    message="This turns back on all apps currently disabled on {session.deviceLabel}, including bloat you disabled on purpose. You can run Optimize again afterwards."
    confirmLabel="Re-enable all"
    onConfirm={runRecovery}
    onCancel={() => (recoveryConfirm = false)}
  />

  <ConfirmDialog
    open={rebootTarget !== null}
    icon="restart_alt"
    danger
    title={rebootTarget ? `${rebootLabels[rebootTarget]}?` : ""}
    message={rebootTarget === "normal"
      ? "The TV restarts normally. Reconnect once it's back."
      : rebootTarget === "recovery"
        ? "The TV boots into its recovery menu. You'll need the TV remote to leave it."
        : "The TV boots into the bootloader. Only do this if you know how to get back out."}
    confirmLabel="Reboot"
    onConfirm={runReboot}
    onCancel={() => (rebootTarget = null)}
  />

  <ConfirmDialog
    open={diagnosticsClearConfirm}
    icon="delete"
    danger
    title="Clear local diagnostics?"
    message="Delete the diagnostics stored in this app? Copies you saved or shared will remain. This doesn't change your TV, license, saved TVs or debug log."
    confirmLabel="Clear records"
    onConfirm={clearDiagnostics}
    onCancel={() => (diagnosticsClearConfirm = false)}
  />

  <ConfirmDialog
    open={disconnectConfirm}
    icon="power_settings_new"
    title="Disconnect from {session.deviceLabel}?"
    message="The app stops talking to this TV until you connect again. Nothing on the TV changes."
    confirmLabel="Disconnect"
    onConfirm={() => { disconnectConfirm = false; onDisconnect(); }}
    onCancel={() => (disconnectConfirm = false)}
  />

  <Toast message={toast} type={toastType} />

  <div class="spacer"></div>
  <BottomTabs active="more" {navigate} />
</div>

<style>
  .support-id { overflow-wrap: anywhere; user-select: text; }
  .topline {
    justify-content: flex-start;
  }
  .header-left {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .header-title {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .more-content {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .more-card {
    padding: 16px;
    border-radius: 18px;
    background: var(--surface);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .card-label {
    font-family: var(--mono);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.12em;
    color: var(--dim);
    text-transform: uppercase;
  }
  .telemetry-toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    cursor: pointer;
  }
  .tt-copy {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .tt-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
  }
  .telemetry-toggle input {
    width: 22px;
    height: 22px;
    flex: none;
    accent-color: var(--accent);
  }
  .card-desc {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.45;
    margin: 0;
  }

  .notices {
    max-height: 50vh;
    overflow: auto;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 12px;
    font-size: 12px;
    line-height: 1.5;
    color: var(--text-soft);
    overflow-wrap: anywhere;
  }
  .notice-h {
    margin: 16px 0 6px;
    color: var(--text);
    font-weight: 700;
    line-height: 1.3;
  }
  .notice-h:first-child {
    margin-top: 0;
  }
  .notice-h1 {
    font-size: 16px;
  }
  .notice-h2 {
    font-size: 14px;
  }
  .notice-h3 {
    font-size: 13px;
  }
  .notice-p {
    margin: 0 0 8px;
  }
  .notice-list {
    margin: 0 0 8px;
    padding-left: 20px;
  }
  .notice-code {
    margin: 0 0 10px;
    padding: 8px;
    border-radius: 8px;
    background: var(--surface);
    font-size: 10px;
    line-height: 1.45;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .notice-table {
    width: 100%;
    margin: 0 0 10px;
    border-collapse: collapse;
    font-size: 11px;
  }
  .notice-table th,
  .notice-table td {
    padding: 4px 6px;
    border-bottom: 1px solid var(--line);
    text-align: left;
    vertical-align: top;
  }
  .notice-table th {
    color: var(--text);
  }
  .notices code {
    font-family: var(--mono);
    font-size: 0.92em;
  }
  .notice-link {
    color: var(--accent);
    text-decoration: underline;
  }
  .telemetry-unsaved {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 10px;
    border: 1px solid color-mix(in srgb, var(--danger) 40%, transparent);
    background: color-mix(in srgb, var(--danger) 8%, transparent);
  }
  .telemetry-unsaved .card-desc {
    color: var(--text-soft);
  }
  .recovery-result {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12px;
    color: var(--muted);
  }
  .recovery-failed {
    margin: 0;
    padding-left: 16px;
    font-size: 11px;
    color: var(--danger);
    word-break: break-all;
  }
  /* One column on phones, all three in a row once they fit; never two and
     a straggler. */
  .reboot-row {
    display: grid;
    grid-template-columns: 1fr;
    gap: 8px;
  }
  @media (min-width: 36rem) {
    .reboot-row {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
  }

  .license-status-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .license-title {
    font-size: 14px;
    font-weight: 600;
  }
  .license-badge {
    font-size: 11px;
    font-weight: 700;
    color: var(--muted);
    background: rgba(255, 255, 255, 0.06);
    padding: 4px 9px;
    border-radius: 7px;
    text-transform: uppercase;
  }
  .license-badge.pro {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .license-desc {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.4;
    margin: 0;
  }
  .success-color {
    color: var(--teal);
  }
  .license-error {
    font-size: 12px;
    color: var(--danger);
    line-height: 1.4;
    margin: 0;
  }
  .license-details {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 12px;
    margin: 0;
    font-size: 12px;
  }
  .license-details dt {
    color: var(--dim);
  }
  .license-details dd {
    margin: 0;
    color: var(--text-soft);
    word-break: break-all;
  }
  .license-source {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--muted);
  }
  .billing-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 10px;
  }
  .license-input-row {
    display: flex;
    gap: 8px;
  }
  .license-input {
    flex: 1;
    min-height: 40px;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: 9px;
    padding: 0 10px;
    color: var(--text);
    font-size: 13px;
  }
  .license-input:focus {
    outline: 2px solid var(--accent);
    border-color: transparent;
  }
  .small-btn {
    min-height: 40px;
    padding: 0 16px;
    width: auto;
    font-size: 13px;
    border-radius: 9px;
  }

  .ghost-btn {
    flex: 1;
    min-height: 44px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 11px;
    background: var(--surface-2);
    color: var(--text-soft);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }
  .ghost-btn:active {
    background: var(--surface);
  }
  .ghost-btn:disabled {
    opacity: 0.6;
  }
  .ghost-btn .msr {
    font-size: 16px;
  }
  .ghost-btn.compact {
    flex: none;
    padding: 0 14px;
  }

  .setting-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    width: 100%;
    background: transparent;
    border: none;
    padding: 4px 0;
    cursor: pointer;
    text-align: left;
    color: var(--text);
    font-family: var(--sans);
  }
  .setting-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }
  .setting-title {
    font-size: 14px;
    font-weight: 600;
  }
  .tool-icon {
    font-size: 22px;
    color: var(--text-soft);
    flex: none;
  }
  .lock-icon {
    font-size: 18px;
    color: var(--dim);
    flex: none;
  }
  .chevron {
    font-size: 20px;
    color: var(--dim);
    flex: none;
  }

  .log-toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .log-view {
    max-height: 260px;
    overflow: auto;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 12px;
    font-size: 10px;
    line-height: 1.5;
    color: var(--text-soft);
    white-space: pre-wrap;
    word-break: break-word;
    margin: 0;
  }
  .log-error {
    font-size: 11px;
    color: var(--danger);
    margin: 0;
  }
  .diagnostics-empty,
  .diagnostics-count {
    margin: 0;
    color: var(--muted);
    font-size: 12px;
    line-height: 1.45;
  }
  .diagnostics-review {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .diagnostic-record {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 12px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--canvas);
  }
  .diagnostic-record-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 13px;
  }
  .diagnostic-token {
    color: var(--text-soft);
    font-size: 11px;
    overflow-wrap: anywhere;
  }
  .diagnostic-reason {
    color: var(--muted);
    font-size: 12px;
  }
  .diagnostic-meta {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 3px 10px;
    margin: 4px 0 0;
    font-size: 10px;
    color: var(--dim);
  }
  .diagnostic-meta dd {
    margin: 0;
    color: var(--text-soft);
    text-align: right;
  }
  .diagnostics-json {
    user-select: text;
  }

  .danger-btn {
    min-height: 50px;
    gap: 8px;
    border-color: rgba(251, 107, 95, 0.25);
    background: rgba(251, 107, 95, 0.08);
    color: var(--danger);
  }
  .danger-btn:active {
    background: rgba(251, 107, 95, 0.05);
  }
  .danger-btn .msr {
    font-size: 20px;
  }
</style>
