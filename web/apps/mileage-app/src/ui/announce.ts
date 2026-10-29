import { AccessibilityInfo } from "react-native";

/**
 * Says something to a screen reader without moving its focus: a state that changes while the
 * driver is looking elsewhere (recording started, stopped, stopped by the system). A live region
 * only speaks when it changes inside a screen that is already read; a new screen or a result needs this.
 */
export function announce(text: string): void {
  AccessibilityInfo.announceForAccessibility(text);
}
