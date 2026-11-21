/** What the helper needs of React Native's `AppState`. */
export interface AppStateLike {
  addEventListener(type: "change", listener: (state: string) => void): { remove(): void };
}

/**
 * Runs `run` now and again each time the app comes back to the front (`active`): the check of a
 * recording after Android ended the app's process (C5), and the new check of the location access
 * when the driver comes back from the settings (D2). A failure inside `run` is the caller's to
 * handle; this only schedules. Answers the function that stops listening.
 *
 * After `refreshOnShow` of `mileage-app` (`src/ui/foreground.ts`).
 */
export function onForeground(appState: AppStateLike, run: () => void): () => void {
  run();
  const subscription = appState.addEventListener("change", (state) => {
    if (state === "active") run();
  });
  return () => subscription.remove();
}
