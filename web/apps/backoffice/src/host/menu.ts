// What the account menu gains inside the phone host app: one entry that opens the app's own
// screen about itself (the server it is on, its version, Change server, Open in browser).
import { t, type MenuItem } from "@obop/ui";
import { askHost, hostAnswers } from "./state.ts";

/**
 * The entries for the account menu: none in a browser, and none inside a host that has not said
 * that it answers `host.settings`. The app's bar over the page leads to the same screen without
 * the page, so an entry that fails says nothing.
 */
export function hostMenuItems(): MenuItem[] {
  if (!hostAnswers("host.settings")) return [];
  return [
    {
      label: t("This app"),
      icon: "info",
      onSelect: () => void askHost("host.settings", {}).catch(() => {}),
    },
  ];
}
