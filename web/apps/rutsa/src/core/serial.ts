/**
 * Runs jobs one after another: a job starts when the one before it has ended, also when that one
 * failed. The actions on a recording go through one (`native/actions.ts`): the check of a recording
 * (C5) and a Start that goes on by itself after a return from the settings (D2) both run when the
 * app comes back to the front, and the check must not stop the updates the Start has just started.
 */
export function createSerial(): <T>(job: () => Promise<T>) => Promise<T> {
  let tail: Promise<unknown> = Promise.resolve();
  return <T>(job: () => Promise<T>): Promise<T> => {
    const result = tail.then(job, job);
    tail = result.catch(() => undefined);
    return result;
  };
}
