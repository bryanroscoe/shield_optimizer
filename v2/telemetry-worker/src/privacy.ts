import { intVar, type Env } from "./env";
import { DEFAULT_RAW_RETENTION_DAYS } from "./ingest";

export function privacyPolicy(env: Env, head = false): Response {
  const contact = env.PRIVACY_CONTACT_EMAIL?.trim();
  if (!contact || !/^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$/.test(contact)) {
    return new Response(head ? null : "Privacy policy is not yet published.", {
      status: 503,
      headers: { "content-type": "text/plain; charset=utf-8", "cache-control": "no-store" },
    });
  }
  const days = intVar(env.RAW_RETENTION_DAYS, DEFAULT_RAW_RETENTION_DAYS);
  const html = `<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>ATV Optimizer Privacy Policy</title>
<style>
:root{color-scheme:light dark}body{margin:0;font:17px/1.65 system-ui,sans-serif}
main{max-width:740px;margin:auto;padding:40px 24px 72px}h1{font-size:2rem;line-height:1.2}
h2{font-size:1.25rem;margin-top:2rem}a{overflow-wrap:anywhere}li{margin:.4rem 0}
</style></head><body><main>
<h1>ATV Optimizer Privacy Policy</h1>
<p>Effective November 12, 2026. ATV Optimizer is operated by Bryan Roscoe.
This policy describes the Android phone and tablet app, package <code>com.atvoptimizer.mobile</code>.</p>
<h2>Your phone and TV</h2>
<p>The app connects to a TV you authorize over your local network. Saved TVs, their addresses,
names and hardware identifiers, debugging authorization keys, installed-app lists, settings,
snapshots, screenshots and downloaded files are stored on your phone or exchanged with your TV.
They are not uploaded to the developer. The app does not require a user account.</p>
<p>Developer options and debugging give access to your TV. Only authorize devices you trust.
Legacy network debugging may be unencrypted; use a trusted network. You can revoke debugging
authorization in your TV's Developer options. Removing a saved TV does not revoke its debugging keys.</p>
<h2>Optional device and error reports</h2>
<p>Reports are on by default with an in-app notice. Turn them off in <strong>More → Privacy</strong>.
Turning them off stops automatic reporting, discards unsent reports and stops support-list requests.
Turning them back on creates a new random installation identifier.</p>
<p>Reports contain a random installation ID, app version, phone operating-system version,
timestamp, TV manufacturer, brand, model, device codename, product and Android version, support
status, action category and whether it worked, and short error codes and redacted error text.
Errors may include crash diagnostics. Reports do not contain TV serial numbers, hardware IDs,
network addresses, TV names, installed-app lists, file contents or purchase records.
The Android app does not upload its debug logs.</p>
<p>Reports go over HTTPS to the developer's service hosted by Cloudflare. The application backend
does not store your request IP address or location. Cloudflare necessarily processes network
information to deliver the service. Device-support updates are downloaded from this service
while reporting is enabled. We do not sell your data or use it for advertising.</p>
<h2>Purchases</h2>
<p>Google Play processes the optional one-time Pro purchase under Google's terms and privacy
policy. The app does not receive your card or bank details. It verifies and stores the signed
purchase record, including purchase token, product, purchase time and an order ID when provided,
on your phone. The developer's reporting service does not receive that record. Google handles
payment records and purchase history; uninstalling the app does not delete Google's records.</p>
<h2>Retention and deletion</h2>
<p>Raw reports are retained for ${days} days and removed by a daily cleanup. Aggregated device and
error statistics are retained longer. Some device statistics retain the random installation ID;
they are not all automatically anonymized when raw reports expire.</p>
<p>For a privacy question or to request deletion of reports associated with your installation,
email <a href="mailto:${contact}">${contact}</a>. Include the <strong>Support ID</strong> shown in
<strong>More → Privacy</strong>. Copy it before turning reports off or clearing app data.
If you previously reset the ID, include any older IDs you saved. Without an ID we may be unable
to identify your reports. Deleting reports does not remove statistics that can no longer be
associated with your installation.</p>
<p>Local data remains until you remove it in the app, clear app storage or uninstall. Android
backup or device-transfer settings can affect retained copies; uninstalling is not a promise
that every backup is erased. TV changes remain on the TV until you undo them. Re-enabling apps
and emergency recovery are free; undoing settings, Home-app or snapshot changes uses Pro features.</p>
<h2>Links and support</h2>
<p>Links you choose to open use your browser and the destination's privacy practices. Information
you choose to include in a GitHub issue is public. Do not put private information in a public issue.
Privacy questions and requests should go to the email above.</p>
<h2>Audience and updates</h2>
<p>The app is intended for adults managing their own TVs and is not directed to children.
We may update this policy when the app or its data practices change; the effective date above
identifies this version. Contact the email above about rights available in your jurisdiction.</p>
</main></body></html>`;
  return new Response(head ? null : html, {
    headers: {
      "content-type": "text/html; charset=utf-8",
      "cache-control": "public, max-age=300",
      "content-security-policy": "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'",
      "x-content-type-options": "nosniff",
      "referrer-policy": "no-referrer",
    },
  });
}
