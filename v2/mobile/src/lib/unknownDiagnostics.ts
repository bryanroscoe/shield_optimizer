export type UnknownDiagnosticKind =
  | "installed_package"
  | "unresolved_process";

export type UnknownDiagnosticReason =
  | "uncatalogued_package"
  | "unknown_safety_classification"
  | "safety_lookup_unavailable"
  | "process_not_resolved";

export type DiagnosticDeviceFamily =
  | "shield"
  | "google_tv"
  | "android_tv"
  | "unknown";

export interface UnknownDiagnosticInput {
  kind: UnknownDiagnosticKind;
  token: string;
  reason: UnknownDiagnosticReason;
  appVersion: string;
  registryVersion?: string | null;
  deviceFamily?: DiagnosticDeviceFamily | null;
  deviceOs?: string | null;
}

export interface UnknownDiagnosticRecord {
  kind: UnknownDiagnosticKind;
  token: string;
  reason: UnknownDiagnosticReason;
  app_version: string;
  registry_version: string | null;
  device_family: DiagnosticDeviceFamily | null;
  device_os: string | null;
  first_seen: string;
  last_seen: string;
  count: number;
}

export interface UnknownDiagnosticReport {
  schema_version: 1;
  generated_at: string;
  truncated: boolean;
  records: UnknownDiagnosticRecord[];
}

export interface DiagnosticsResult {
  ok: boolean;
  message?: string;
}

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

interface StoredDiagnostics {
  schema_version: 1;
  truncated: boolean;
  records: UnknownDiagnosticRecord[];
}

interface CollectorOptions {
  maxRecords?: number;
  maxBytes?: number;
  maxCount?: number;
  now?: () => Date;
  schedule?: (task: () => void) => void;
}

const STORAGE_KEY = "atv.unknownDiagnostics.v1";
export const UNKNOWN_DIAGNOSTICS_MAX_RECORDS = 100;
export const UNKNOWN_DIAGNOSTICS_MAX_BYTES = 64 * 1024;
export const UNKNOWN_DIAGNOSTICS_MAX_COUNT = 9_999;
const SAVE_ERROR = "Couldn't save diagnostics on this device.";
const READ_ERROR = "Couldn't load diagnostics stored on this device.";
const CLEAR_ERROR = "Couldn't clear diagnostics stored on this device.";

const kinds = new Set<UnknownDiagnosticKind>([
  "installed_package",
  "unresolved_process",
]);
const reasons = new Set<UnknownDiagnosticReason>([
  "uncatalogued_package",
  "unknown_safety_classification",
  "safety_lookup_unavailable",
  "process_not_resolved",
]);
const families = new Set<DiagnosticDeviceFamily>([
  "shield",
  "google_tv",
  "android_tv",
  "unknown",
]);

function byteLength(value: string): number {
  return new TextEncoder().encode(value).byteLength;
}

function validVersion(value: unknown): value is string {
  return (
    typeof value === "string" &&
    value.length > 0 &&
    value.length <= 32 &&
    /^[0-9A-Za-z][0-9A-Za-z.+_-]*$/.test(value)
  );
}

function validDeviceOs(value: unknown): value is string {
  return (
    typeof value === "string" &&
    value.length > 0 &&
    value.length <= 32 &&
    /^[0-9A-Za-z][0-9A-Za-z._ -]*$/.test(value)
  );
}

function looksLikeAddress(value: string): boolean {
  if (/^\d{1,3}(?:\.\d{1,3}){3}(?::\d+)?$/.test(value)) return true;
  if (/^(?:[0-9a-f]{2}:){5}[0-9a-f]{2}$/i.test(value)) return true;
  if ((value.match(/:/g) ?? []).length > 1) return true;
  return /^[A-Za-z0-9.-]+:\d+$/.test(value);
}

export function isValidDiagnosticToken(
  kind: UnknownDiagnosticKind,
  value: unknown,
): value is string {
  if (typeof value !== "string" || value.length === 0 || value.length > 255) {
    return false;
  }
  if (
    /[\u0000-\u001f\u007f\s/\\]/.test(value) ||
    value.includes("://") ||
    looksLikeAddress(value)
  ) {
    return false;
  }
  if (kind === "installed_package") {
    return /^[A-Za-z][A-Za-z0-9_]*(?:\.[A-Za-z][A-Za-z0-9_]*)+$/.test(value);
  }
  return /^[A-Za-z0-9_][A-Za-z0-9_.:-]*$/.test(value);
}

function isObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function reasonMatchesKind(
  kind: UnknownDiagnosticKind,
  reason: UnknownDiagnosticReason,
): boolean {
  return kind === "unresolved_process"
    ? reason === "process_not_resolved"
    : reason !== "process_not_resolved";
}

function normalizeRecord(value: unknown): UnknownDiagnosticRecord | null {
  if (!isObject(value)) return null;
  if (!kinds.has(value.kind as UnknownDiagnosticKind)) return null;
  const kind = value.kind as UnknownDiagnosticKind;
  if (!isValidDiagnosticToken(kind, value.token)) return null;
  if (!reasons.has(value.reason as UnknownDiagnosticReason)) return null;
  if (!reasonMatchesKind(kind, value.reason as UnknownDiagnosticReason)) return null;
  if (!validVersion(value.app_version)) return null;
  if (value.registry_version !== null && !validVersion(value.registry_version)) {
    return null;
  }
  if (
    value.device_family !== null &&
    !families.has(value.device_family as DiagnosticDeviceFamily)
  ) {
    return null;
  }
  if (value.device_os !== null && !validDeviceOs(value.device_os)) return null;
  if (
    typeof value.first_seen !== "string" ||
    !Number.isFinite(Date.parse(value.first_seen)) ||
    typeof value.last_seen !== "string" ||
    !Number.isFinite(Date.parse(value.last_seen)) ||
    typeof value.count !== "number" ||
    !Number.isInteger(value.count) ||
    value.count < 1 ||
    value.count > UNKNOWN_DIAGNOSTICS_MAX_COUNT
  ) {
    return null;
  }
  return {
    kind,
    token: value.token,
    reason: value.reason as UnknownDiagnosticReason,
    app_version: value.app_version,
    registry_version: value.registry_version,
    device_family: value.device_family as DiagnosticDeviceFamily | null,
    device_os: value.device_os,
    first_seen: new Date(value.first_seen).toISOString(),
    last_seen: new Date(value.last_seen).toISOString(),
    count: value.count,
  };
}

function emptyStored(): StoredDiagnostics {
  return { schema_version: 1, truncated: false, records: [] };
}

export class UnknownDiagnostics {
  private readonly storage: StorageLike;
  private readonly maxRecords: number;
  private readonly maxBytes: number;
  private readonly maxCount: number;
  private readonly now: () => Date;
  private readonly scheduleTask: (task: () => void) => void;
  private epoch = 0;
  private lastErrorValue = "";

  constructor(storage: StorageLike, options: CollectorOptions = {}) {
    this.storage = storage;
    this.maxRecords = options.maxRecords ?? UNKNOWN_DIAGNOSTICS_MAX_RECORDS;
    this.maxBytes = options.maxBytes ?? UNKNOWN_DIAGNOSTICS_MAX_BYTES;
    this.maxCount = options.maxCount ?? UNKNOWN_DIAGNOSTICS_MAX_COUNT;
    this.now = options.now ?? (() => new Date());
    this.scheduleTask = options.schedule ?? ((task) => void setTimeout(task, 0));
  }

  get lastError(): string {
    return this.lastErrorValue;
  }

  private read(): StoredDiagnostics {
    try {
      const raw = this.storage.getItem(STORAGE_KEY);
      if (!raw) return emptyStored();
      const parsed: unknown = JSON.parse(raw);
      if (
        !isObject(parsed) ||
        parsed.schema_version !== 1 ||
        typeof parsed.truncated !== "boolean" ||
        !Array.isArray(parsed.records)
      ) {
        this.lastErrorValue = READ_ERROR;
        return emptyStored();
      }
      const records = parsed.records
        .map(normalizeRecord)
        .filter((record): record is UnknownDiagnosticRecord => record !== null)
        .slice(-this.maxRecords);
      if (records.length !== parsed.records.length) this.lastErrorValue = READ_ERROR;
      return {
        schema_version: 1,
        truncated: parsed.truncated || records.length !== parsed.records.length,
        records,
      };
    } catch {
      this.lastErrorValue = READ_ERROR;
      return emptyStored();
    }
  }

  private persist(data: StoredDiagnostics): DiagnosticsResult {
    while (data.records.length > this.maxRecords) {
      data.records.shift();
      data.truncated = true;
    }
    let serialized = JSON.stringify(data);
    while (data.records.length > 0 && byteLength(serialized) > this.maxBytes) {
      data.records.shift();
      data.truncated = true;
      serialized = JSON.stringify(data);
    }
    if (byteLength(serialized) > this.maxBytes) {
      this.lastErrorValue = SAVE_ERROR;
      return { ok: false, message: SAVE_ERROR };
    }
    try {
      this.storage.setItem(STORAGE_KEY, serialized);
      this.lastErrorValue = "";
      return { ok: true };
    } catch {
      this.lastErrorValue = SAVE_ERROR;
      return { ok: false, message: SAVE_ERROR };
    }
  }

  observe(input: UnknownDiagnosticInput): DiagnosticsResult {
    return this.observeMany([input]);
  }

  observeMany(inputs: UnknownDiagnosticInput[]): DiagnosticsResult {
    const data = this.read();
    const now = this.now().toISOString();
    let accepted = 0;
    for (const input of inputs) {
      if (
        !kinds.has(input.kind) ||
        !reasons.has(input.reason) ||
        !reasonMatchesKind(input.kind, input.reason) ||
        !isValidDiagnosticToken(input.kind, input.token) ||
        !validVersion(input.appVersion) ||
        (input.registryVersion != null && !validVersion(input.registryVersion)) ||
        (input.deviceFamily != null && !families.has(input.deviceFamily)) ||
        (input.deviceOs != null && !validDeviceOs(input.deviceOs))
      ) {
        continue;
      }
      ++accepted;
      const registryVersion = input.registryVersion ?? null;
      const deviceFamily = input.deviceFamily ?? null;
      const deviceOs = input.deviceOs ?? null;
      const index = data.records.findIndex(
        (record) =>
          record.kind === input.kind &&
          record.token === input.token &&
          record.reason === input.reason &&
          record.app_version === input.appVersion &&
          record.registry_version === registryVersion &&
          record.device_family === deviceFamily &&
          record.device_os === deviceOs,
      );
      if (index >= 0) {
        const existing = data.records.splice(index, 1)[0];
        data.records.push({
          ...existing,
          last_seen: now,
          count: Math.min(this.maxCount, existing.count + 1),
        });
      } else {
        data.records.push({
          kind: input.kind,
          token: input.token,
          reason: input.reason,
          app_version: input.appVersion,
          registry_version: registryVersion,
          device_family: deviceFamily,
          device_os: deviceOs,
          first_seen: now,
          last_seen: now,
          count: 1,
        });
      }
    }
    if (accepted === 0) return { ok: false };
    return this.persist(data);
  }

  schedule(
    input: UnknownDiagnosticInput,
    isCurrent: () => boolean = () => true,
  ): void {
    this.scheduleMany([input], isCurrent);
  }

  scheduleMany(
    inputs: UnknownDiagnosticInput[],
    isCurrent: () => boolean = () => true,
  ): void {
    const scheduledEpoch = this.epoch;
    this.scheduleTask(() => {
      if (scheduledEpoch !== this.epoch || !isCurrent()) return;
      this.observeMany(inputs);
    });
  }

  snapshot(): UnknownDiagnosticReport {
    const data = this.read();
    return {
      schema_version: 1,
      generated_at: this.now().toISOString(),
      truncated: data.truncated,
      records: data.records.map((record) => ({ ...record })),
    };
  }

  exportSnapshot(snapshot: UnknownDiagnosticReport): string {
    return JSON.stringify(snapshot, null, 2);
  }

  clear(): DiagnosticsResult {
    ++this.epoch;
    try {
      this.storage.removeItem(STORAGE_KEY);
      if (this.storage.getItem(STORAGE_KEY) !== null) throw new Error();
      this.lastErrorValue = "";
      return { ok: true };
    } catch {
      this.lastErrorValue = CLEAR_ERROR;
      return { ok: false, message: CLEAR_ERROR };
    }
  }
}

let singleton: UnknownDiagnostics | null = null;

function diagnostics(): UnknownDiagnostics | null {
  if (singleton) return singleton;
  try {
    singleton = new UnknownDiagnostics(window.localStorage);
    return singleton;
  } catch {
    return null;
  }
}

export function recordUnknownDiagnostic(
  input: UnknownDiagnosticInput,
  isCurrent?: () => boolean,
): void {
  diagnostics()?.schedule(input, isCurrent);
}

export function recordUnknownDiagnostics(
  inputs: UnknownDiagnosticInput[],
  isCurrent?: () => boolean,
): void {
  diagnostics()?.scheduleMany(inputs, isCurrent);
}

export function getUnknownDiagnosticsSnapshot(): UnknownDiagnosticReport {
  return diagnostics()?.snapshot() ?? {
    schema_version: 1,
    generated_at: new Date().toISOString(),
    truncated: false,
    records: [],
  };
}

export function exportUnknownDiagnosticsSnapshot(
  snapshot: UnknownDiagnosticReport,
): string {
  return diagnostics()?.exportSnapshot(snapshot) ?? JSON.stringify(snapshot, null, 2);
}

export function clearUnknownDiagnostics(): DiagnosticsResult {
  return diagnostics()?.clear() ?? { ok: false, message: CLEAR_ERROR };
}

export function unknownDiagnosticsError(): string {
  return diagnostics()?.lastError ?? READ_ERROR;
}
