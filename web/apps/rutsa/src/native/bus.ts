/**
 * A signal that the trips have changed: the location task stored fixes, or an action changed a trip.
 * The screens that are open read again when it fires. It carries no data, so nothing about a route
 * passes through it.
 *
 * From `mileage-app` (`src/native/bus.ts`).
 */
const listeners = new Set<() => void>();

export function emitChange(): void {
  for (const listener of listeners) listener();
}

/** Calls `listener` on every change; the answer removes it again. */
export function onChange(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
