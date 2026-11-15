// The few places where the components differ between iOS and Android. The system is passed in, so
// that these run in `vp test` and the web Storybook (where it is "web") needs no special case.

/**
 * Whether a screen moves for the keyboard itself. Android resizes the window when the keyboard
 * opens (the app's default `adjustResize`), so a field and the button under it stay in view; iOS
 * leaves the window as it is and the keyboard covers whatever is at the bottom, so the screen pads
 * itself (KeyboardAvoidingView).
 */
export function screenAvoidsKeyboard(os: string): boolean {
  return os === "ios";
}

/**
 * What a screen reader is told when a banner appears, or `null` when the system does it by itself.
 * Android speaks a live region (\`accessibilityLiveRegion\` on the banner) when it changes. iOS has
 * no such property, so a warning or a problem is announced; a note or a success is only read when
 * the person reaches it.
 */
export function bannerAnnouncement(os: string, tone: string, spoken: string): string | null {
  return os === "ios" && (tone === "danger" || tone === "warning") ? spoken : null;
}
