/**
 * The modifier key of shortcuts as a person reads it: the Command sign on Apple platforms, Ctrl
 * everywhere else (B13: a Windows laptop was shown "⌘K"). The shortcut handlers already accept
 * either key (`e.metaKey || e.ctrlKey`); this is only what is printed.
 */
export function isApple(platform: string = currentPlatform()): boolean {
  return /mac|iphone|ipad|ipod/i.test(platform);
}

function currentPlatform(): string {
  if (typeof navigator === "undefined") return "";
  const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
  return nav.userAgentData?.platform || nav.platform || nav.userAgent || "";
}

/** "⌘" on Apple platforms, "Ctrl" elsewhere. */
export function modKey(platform?: string): string {
  return isApple(platform) ? "⌘" : "Ctrl";
}

/** A shortcut with the modifier: "⌘K" on Apple platforms, "Ctrl+K" elsewhere. */
export function shortcut(key: string, platform?: string): string {
  return isApple(platform) ? `⌘${key}` : `Ctrl+${key}`;
}
