/** What the helper needs of React Native's `AppState`. */
export interface AppStateLike {
  addEventListener(type: "change", listener: (state: string) => void): { remove(): void };
}

/**
 * Runs `refresh` now and again each time the app comes back to the foreground (`active`), so a
 * screen that shows the server's view (Home: the state of the last trip, L27) does not keep an old
 * copy until the Trips list is opened. A failed refresh is the caller's to swallow; this only
 * schedules. Returns the function that stops listening.
 */
export function refreshOnShow(appState: AppStateLike, refresh: () => void): () => void {
  refresh();
  const subscription = appState.addEventListener("change", (state) => {
    if (state === "active") refresh();
  });
  return () => subscription.remove();
}
