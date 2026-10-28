/**
 * A change in the outbox happened (a point was stored by the task, a trip was stopped, a pass
 * ended). The screens listen and read again. The task and the screens share one JavaScript
 * runtime while the app is alive, so a plain listener list is enough.
 */
const listeners = new Set<() => void>();

export function emitChange(): void {
  for (const l of listeners) l();
}

export function onChange(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
