// Work that begins for one session (a read of the metadata and the settings after sign-in) may
// finish after the person signed out. What it read belongs to the old session and must not be
// applied to the sign-in screen that follows (K25).

export type SessionCounter = {
  /** The session now: keep it when work starts. */
  current: () => number;
  /** A sign-out: every piece of work begun before is stale. */
  next: () => void;
  /** Whether the session `at` is still the one in force. */
  live: (at: number) => boolean;
};

export function sessionCounter(): SessionCounter {
  let n = 0;
  return {
    current: () => n,
    next: () => {
      n += 1;
    },
    live: (at) => at === n,
  };
}

/** Variables and attributes the theme of a signed-in instance puts on the page root. */
const THEME_ATTRIBUTES = ["theme", "density", "radius"] as const;

/**
 * Take off the page root what the theme of the instance put there, so the sign-in screen after a
 * sign-out looks as one after a reload (K25). The brand's accent is put back by the caller.
 */
export function clearInstanceTheme(root: HTMLElement): void {
  const props: string[] = [];
  for (let i = 0; i < root.style.length; i++) props.push(root.style.item(i));
  for (const p of props) {
    if (p.startsWith("--accent") || p.startsWith("--on-accent") || p === "--radius") {
      root.style.removeProperty(p);
    }
  }
  for (const a of THEME_ATTRIBUTES) delete root.dataset[a];
}
