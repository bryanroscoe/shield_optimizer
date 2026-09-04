// Tiny pub/sub so api.ts can report a lost TV connection without importing the
// session store (which imports api.ts — a cycle). The backend prefixes every
// error that evicted the live connection with CONNECTION_LOST_PREFIX.

const LOST_PATTERNS = [
  /Connection to the TV was lost/i,
  /Not connected to a device\./i,
];

type Listener = () => void;
const listeners = new Set<Listener>();

export function isConnectionLostError(message: string): boolean {
  return LOST_PATTERNS.some((p) => p.test(message));
}

export function onConnectionLost(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function emitConnectionLost(): void {
  for (const listener of listeners) listener();
}
