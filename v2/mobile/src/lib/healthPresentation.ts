import type { RamInfo, StorageInfo } from './types';
const finite = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value);
// Temperature presentation is shared with the desktop Health tile.
export { metricNumber, temperatureContext } from '../../../shared/health';

export function memoryUsage(ram: Partial<RamInfo> | null | undefined) {
  const total = ram?.total_mb;
  if (!finite(total) || total <= 0) return null;
  const free = ram?.free_mb;
  const used = ram?.used_mb;
  if (free != null && (!finite(free) || free < 0 || free > total)) return null;
  if (used != null && (!finite(used) || used < 0 || used > total)) return null;
  const amount = used ?? (free != null ? total - free : null);
  // Android reports Used and Free RAM with separate cache/swap accounting.
  if (amount == null) return null;
  return { used: amount, total, free: free ?? null, percent: Math.round(amount / total * 100) };
}

export function storageUsage(storage: Partial<StorageInfo> | null | undefined) {
  const percent = storage?.used_percent;
  return finite(percent) && percent >= 0 && percent <= 100 ? percent : null;
}
