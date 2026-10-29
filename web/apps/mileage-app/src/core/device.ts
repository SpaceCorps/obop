/** The part of React Native's `Platform` that names the device (injected, so this runs in `vp test`). */
export interface PlatformInfo {
  OS: string;
  constants?: Record<string, unknown> | null;
}

const text = (value: unknown): string => (typeof value === "string" ? value.trim() : "");

/**
 * The name the instance lists for this phone (Customize, Security, Devices). Android gives the
 * maker and the model; iOS gives no model without a native module (and the app has none for it),
 * only whether it is a phone or a tablet.
 */
export function deviceName(platform: PlatformInfo): string {
  const c = platform.constants ?? {};
  if (platform.OS === "android") {
    return [text(c.Brand), text(c.Model)].filter((x) => x !== "").join(" ") || "Android phone";
  }
  if (platform.OS === "ios") {
    const idiom = text(c.interfaceIdiom);
    if (idiom === "pad") return "iPad";
    if (idiom === "phone") return "iPhone";
    return "iOS device";
  }
  return "Phone";
}
