import { useEffect, useState } from "react";
import { THEME, themeChoiceOf, type ThemeChoice } from "../core/settings.ts";
import { getSettings } from "../native/store.ts";

/**
 * The look the driver chose (in Profile, stage 2; nothing chooses it in stage 1): one value for the
 * whole app, read from the settings once the trips are open and kept there when it changes. Every
 * screen that uses `useThemeChoice` follows a change at once; the theme of `App.tsx` too.
 */

let current: ThemeChoice = "system";
let read = false;
const listeners = new Set<(choice: ThemeChoice) => void>();

function tell(choice: ThemeChoice) {
  current = choice;
  for (const listener of listeners) listener(choice);
}

/** The look chosen, and the way to choose another (kept in the settings). */
export function useThemeChoice(): [ThemeChoice, (choice: ThemeChoice) => void] {
  const [choice, setChoice] = useState(current);
  useEffect(() => {
    listeners.add(setChoice);
    if (!read) {
      read = true;
      getSettings()
        .then((settings) => settings.get(THEME))
        .then(
          (value) => tell(themeChoiceOf(value)),
          () => undefined,
        );
    }
    return () => {
      listeners.delete(setChoice);
    };
  }, []);
  const choose = (next: ThemeChoice) => {
    tell(next);
    getSettings()
      .then((settings) => settings.set(THEME, next))
      .catch(() => undefined);
  };
  return [choice, choose];
}
