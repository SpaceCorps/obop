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
  return onReturn(appState, run);
}

/**
 * Runs `run` each time the app comes back to the front, not now: the screen of the permissions
 * checks again when the driver comes back from the settings (US-18 AC5). Answers the function that
 * stops listening.
 */
export function onReturn(appState: AppStateLike, run: () => void): () => void {
  const subscription = appState.addEventListener("change", (state) => {
    if (state === "active") run();
  });
  return () => subscription.remove();
}

/**
 * Runs `run` when the app comes back to the front after the driver was away (in the settings, in
 * another app): a move to the background, then back to `active`. On Android a system dialog of the
 * app itself (a permission) sends the app to the background too; while `isAsking()` says such a
 * dialog is open, that move does not count, so the answer to the dialog is not taken for a return
 * from the settings. Answers the function that stops listening.
 */
export function onReturnFromAway(
  appState: AppStateLike,
  isAsking: () => boolean,
  run: () => void,
): () => void {
  let away = false;
  const subscription = appState.addEventListener("change", (state) => {
    if (state === "background") {
      if (!isAsking()) away = true;
    } else if (state === "active" && away) {
      away = false;
      run();
    }
  });
  return () => subscription.remove();
}
