// One plain-text email to the owner per stored upload, so a bug report
// reaches him without anyone polling the admin API. Sent through Cloudflare
// Email Routing's send_email binding, after the response, and never allowed
// to fail or slow the upload.
//
// The email carries metadata and the note, both already redacted, and never
// the log text: the full upload stays behind the admin token. At most
// REPORT_MAIL_DAILY (50) emails per UTC day, counted in rate_daily under
// `report-mail:*`, so a flood of uploads can't become a flood of email.

import { EmailMessage } from "cloudflare:email";
import type { Env } from "./env";
import { intVar } from "./env";
import { admit, utcDay } from "./ingest";
import { clamp } from "./redact";
import { shortId } from "./support";
import type { CleanUpload } from "./uploads";

export const MAIL_COUNTER_KEY = "report-mail:*";
export const DEFAULT_MAIL_DAILY = 50;
const NOTE_IN_MAIL = 2000;
const ADDRESS = /^[A-Za-z0-9._%+-]{1,64}@[A-Za-z0-9.-]{1,253}\.[A-Za-z]{2,24}$/;

export interface MailUpload {
  id: number;
  received_at: string;
  upload: CleanUpload;
}

// Header values come from validated fields and Worker vars, but a header is
// only safe if no CR or LF reaches it: every control character becomes a
// space, and anything outside printable ASCII is sent as an RFC 2047 word.
export function headerValue(value: string): string {
  const flat = value.replace(/[\u0000-\u001f\u007f-\u009f\u2028\u2029]/g, " ").replace(/ {2,}/g, " ").trim();
  if (/^[\x20-\x7e]*$/.test(flat)) return flat;
  return `=?UTF-8?B?${base64(new TextEncoder().encode(flat))}?=`;
}

function base64(bytes: Uint8Array): string {
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin);
}

const kb = (n: number) => (n < 1024 ? `${n} B` : `${(n / 1024).toFixed(1)} KB`);

export function mailSubject(m: MailUpload): string {
  const u = m.upload;
  const what = u.kind === "bug" ? "Bug report" : "Logs";
  return `[ATV Optimizer] ${what} #${m.id} from ${u.app} ${u.app_version} (${shortId(u.install_id)})`;
}

export function mailBody(m: MailUpload): string {
  const u = m.upload;
  const reports = u.reports_on === null ? "unknown" : u.reports_on ? "on" : "off";
  const lines = [
    u.kind === "bug" ? `New bug report #${m.id}.` : `New Send logs upload #${m.id}.`,
    "",
    `Kind:        ${u.kind}`,
    `App:         ${u.app} ${u.app_version}`,
    `OS:          ${u.os}`,
    `Support ID:  ${shortId(u.install_id)}`,
    `Received:    ${m.received_at}`,
    `Reports:     ${reports}`,
    `Text:        ${u.body ? kb(new TextEncoder().encode(u.body).length) : "none"}`,
    "",
  ];
  if (u.note) {
    const chars = Array.from(u.note).length;
    lines.push("Description (redacted):", "", clamp(u.note, NOTE_IN_MAIL));
    if (chars > NOTE_IN_MAIL) lines.push("", `[... ${chars - NOTE_IN_MAIL} more characters in the full upload]`);
  } else {
    lines.push("No description.");
  }
  lines.push(
    "",
    "Full upload (admin token):",
    `  GET /v1/admin/uploads/${m.id}`,
    `  scripts/support-lookup.sh --upload ${m.id}`,
    `  scripts/support-lookup.sh ${shortId(u.install_id)}`,
  );
  return `${lines.join("\n")}\n`;
}

// RFC 5322 by hand: CRLF line ends, base64 body in 76-column lines, so UTF-8
// and long note lines pass any relay unchanged.
export function buildReportEmail(m: MailUpload, from: string, to: string, now: Date): string {
  const domain = from.slice(from.lastIndexOf("@") + 1);
  const nonce = crypto.randomUUID();
  const body = base64(new TextEncoder().encode(mailBody(m).replace(/\r?\n/g, "\r\n")));
  const headers = [
    `From: ATV Optimizer reports <${headerValue(from)}>`,
    `To: <${headerValue(to)}>`,
    `Subject: ${headerValue(mailSubject(m))}`,
    `Date: ${now.toUTCString().replace("GMT", "+0000")}`,
    `Message-ID: <upload-${m.id}.${nonce}@${headerValue(domain)}>`,
    "MIME-Version: 1.0",
    "Content-Type: text/plain; charset=utf-8",
    "Content-Transfer-Encoding: base64",
  ];
  return `${headers.join("\r\n")}\r\n\r\n${body.replace(/.{1,76}/g, "$&\r\n")}`;
}

export function mailConfig(env: Env): { binding: SendEmail; from: string; to: string } | null {
  const from = env.REPORT_MAIL_FROM?.trim() ?? "";
  const to = env.REPORT_MAIL_TO?.trim() ?? "";
  if (!env.REPORT_MAIL || !ADDRESS.test(from) || !ADDRESS.test(to)) return null;
  return { binding: env.REPORT_MAIL, from, to };
}

// Never throws. Nothing is logged: the error could carry request data.
export async function notifyUpload(env: Env, m: MailUpload, now: Date): Promise<void> {
  try {
    const config = mailConfig(env);
    if (!config) return;
    const cap = intVar(env.REPORT_MAIL_DAILY, DEFAULT_MAIL_DAILY);
    if (!(await admit(env, utcDay(now), [{ key: MAIL_COUNTER_KEY, n: 1, cap }]))) return;
    const raw = buildReportEmail(m, config.from, config.to, now);
    await config.binding.send(new EmailMessage(config.from, config.to, raw));
  } catch {
    // An email that didn't go is not worth failing anything over; the upload
    // is stored and the admin list shows it.
  }
}
