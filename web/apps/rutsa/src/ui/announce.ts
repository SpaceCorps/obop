import { AccessibilityInfo } from "react-native";

/**
 * Says something to a screen reader without moving its focus: a state that changes while the
 * driver is looking elsewhere (recording started, stopped, stopped by itself).
 *
 * From `mileage-app` (`src/ui/announce.ts`).
 */
export function announce(text: string): void {
  AccessibilityInfo.announceForAccessibility(text);
}
