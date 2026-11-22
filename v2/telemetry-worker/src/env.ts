export interface Env {
  DB: D1Database;
  ADMIN_TOKEN?: string;
  RAW_RETENTION_DAYS?: string;
  PER_INSTALL_DAILY_EVENTS?: string;
  GLOBAL_DAILY_EVENTS?: string;
  GLOBAL_DAILY_EVENT_BYTES?: string;
  STORAGE_BUDGET_BYTES?: string;
  STORAGE_TRIM_MAX_ROW_WRITES?: string;
  LOG_RETENTION_DAYS?: string;
  PER_INSTALL_DAILY_UPLOADS?: string;
  GLOBAL_DAILY_UPLOADS?: string;
  GLOBAL_DAILY_UPLOAD_BYTES?: string;
  PRIVACY_CONTACT_EMAIL?: string;
  // Email the owner one message per stored upload. Off unless all three are
  // set; see "Email notifications" in README.md.
  REPORT_MAIL?: SendEmail;
  REPORT_MAIL_FROM?: string;
  REPORT_MAIL_TO?: string;
  REPORT_MAIL_DAILY?: string;
}

export function intVar(v: string | undefined, fallback: number): number {
  const n = Number.parseInt(v ?? "", 10);
  return Number.isFinite(n) && n > 0 ? n : fallback;
}
