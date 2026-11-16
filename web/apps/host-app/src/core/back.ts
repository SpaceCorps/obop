// What Android's Back button does while the back office is shown.

export type BackAction =
  /** "This app" is open: close it. */
  | "close_settings"
  /** The page has history: go back in it (routes are fragment addresses, so this walks the routes). */
  | "web_back"
  /** Nothing to go back to: the default, the app leaves. */
  | "leave";

export interface BackInput {
  readonly settingsOpen: boolean;
  /** The error screen lies over the page: its history is not walked from under it. */
  readonly errorShown: boolean;
  /** From the WebView's last navigation state. */
  readonly canGoBack: boolean;
}

/**
 * Back at the root of the history is not made a second route to "This app": a page that keeps
 * adding history entries keeps Back from ever reaching the root, and catching Back there switches
 * off Android's predictive back-to-home. A dialog open in the page is not closed first.
 */
export function backAction(input: BackInput): BackAction {
  if (input.settingsOpen) return "close_settings";
  if (input.errorShown) return "leave";
  return input.canGoBack ? "web_back" : "leave";
}
