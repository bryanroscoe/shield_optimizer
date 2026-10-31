export interface Env {
  DB: D1Database;
  ADMIN_TOKEN?: string;
  RAW_RETENTION_DAYS?: string;
  PER_INSTALL_DAILY_EVENTS?: string;
  GLOBAL_DAILY_EVENTS?: string;
}

export function intVar(v: string | undefined, fallback: number): number {
  const n = Number.parseInt(v ?? "", 10);
  return Number.isFinite(n) && n > 0 ? n : fallback;
}
