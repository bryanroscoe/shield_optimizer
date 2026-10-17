import type { HealthReport, RamInfo, StorageInfo } from './types';
const finite = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value);
export const metricNumber = (value: number) => String(Math.round(value * 10) / 10);

export function memoryUsage(ram: Partial<RamInfo> | null | undefined) {
  const total = ram?.total_mb;
  if (!finite(total) || total <= 0) return null;
  const free = ram?.free_mb;
  const used = ram?.used_mb;
  if (free != null && (!finite(free) || free < 0 || free > total)) return null;
  if (used != null && (!finite(used) || used < 0 || used > total)) return null;
  const amount = used ?? (free != null ? total - free : null);
  if (amount == null || (free != null && amount + free > total)) return null;
  return { used: amount, total, free: free ?? null, percent: Math.round(amount / total * 100) };
}

export function storageUsage(storage: Partial<StorageInfo> | null | undefined) {
  const percent = storage?.used_percent;
  return finite(percent) && percent >= 0 && percent <= 100 ? percent : null;
}

const statusNames = ['No throttling reported', 'Light throttling', 'Moderate throttling', 'Severe throttling', 'Critical thermal status', 'Emergency thermal status', 'Shutdown thermal status'];
const statusHelp = [
  'No thermal throttling is reported in this snapshot; this is not a temperature safety rating.',
  'The device reports light thermal limiting.',
  'The device reports thermal limiting. Reduce heavy workloads and recheck.',
  'Thermal limits can noticeably reduce performance. Pause heavy workloads and recheck.',
  'The device reports strong thermal limiting. Pause heavy workloads and let it cool.',
  'The device reports an emergency condition. Stop heavy workloads and follow its warnings.',
  'The device reports a shutdown condition. Stop heavy workloads and follow its warnings.',
];
const validStatus = (value: unknown): value is number => finite(value) && Number.isInteger(value) && value >= 0 && value <= 6;

export function temperatureContext(report: Partial<HealthReport> | null | undefined) {
  const value = finite(report?.temperature_c) ? report.temperature_c : null;
  const candidate = report?.temperature_reading;
  // Never attach another value's sensor/threshold context to a legacy scalar.
  const reading = candidate && value != null && candidate.celsius === value ? candidate : null;
  const deviceStatus = report?.thermal_status;
  const sensorStatus = validStatus(reading?.status) ? reading.status : null;
  const status = validStatus(deviceStatus) ? deviceStatus : sensorStatus;
  const rawName = reading?.sensor_name;
  const indexed = rawName?.match(/^(CPU|GPU) temperatures\[(\d+)\]$/);
  const name = indexed ? `${indexed[1]} reading ${Number(indexed[2]) + 1}` : rawName;
  const kind = reading?.sensor_type;
  const sensor = indexed ? name! : name && kind && name.toLowerCase() !== kind.toLowerCase() ? `${kind} · ${name}` : name || kind || 'Sensor not reported';
  const source = reading?.source === 'thermalservice' ? 'Android thermal sensors'
    : reading?.source === 'hardware_properties' ? 'CPU/GPU hardware report' : null;
  const throttle = finite(reading?.throttling_c) && reading.throttling_c > 0 ? reading.throttling_c : null;
  const shutdown = finite(reading?.shutdown_c) && reading.shutdown_c > 0 ? reading.shutdown_c : null;
  let comparison: string | null = null;
  if (value != null && throttle != null) {
    const difference = value - throttle;
    comparison = difference === 0 ? 'At reported throttle point'
      : `${metricNumber(Math.abs(difference))}°C ${difference > 0 ? 'above' : 'below'} reported throttle point`;
  }
  return {
    value, sensor, source, throttle, shutdown, comparison,
    deviceStatus: validStatus(deviceStatus) ? deviceStatus : null, sensorStatus,
    sensorStatusLabel: sensorStatus == null ? null : statusNames[sensorStatus],
    status, statusScope: validStatus(deviceStatus) ? 'Device status' : 'Selected sensor status',
    statusLabel: status == null ? 'Thermal status not reported' : statusNames[status],
    statusHelp: status == null ? (throttle != null ? 'No severity level was supplied; the limits below belong to this reported sensor.' : 'A temperature alone does not establish overheating. No device-specific severity was supplied.') : statusHelp[status],
  };
}
