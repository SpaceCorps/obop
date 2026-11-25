import { useInsets, useTheme } from "@obop/ui-native";
import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import { Keyboard, useWindowDimensions, type HostInstance } from "react-native";

/**
 * A block that reaches down to the bottom of its screen (the Note of a trip, D6 and D7): its least
 * height is the room between its top and the bottom of the screen (above the bar of the system),
 * less the padding at the bottom of the screen, measured on the screen after each render and
 * layout and when the keyboard comes or goes (it makes the window of Android smaller). The
 * screens with these blocks have no footer: their buttons are in the header. `blockRef` and
 * `onLayout` go on the block, `minHeight` in its style; `reserve` is the room kept under it for
 * what follows it (Delete trip while a trip is edited).
 */
export function useFillToBottom(reserve = 0): {
  blockRef: RefObject<HostInstance | null>;
  onLayout: () => void;
  minHeight: number;
} {
  const theme = useTheme();
  const insets = useInsets();
  const window = useWindowDimensions();
  const blockRef = useRef<HostInstance>(null);
  const [minHeight, setMinHeight] = useState(0);

  const onLayout = useCallback(() => {
    const block = blockRef.current;
    if (!block) return;
    block.measureInWindow((_x, top, width) => {
      // Hidden (another screen over it), it measures nothing.
      if (width <= 0) return;
      const bottom = window.height - insets.bottom - theme.space[4] - reserve;
      setMinHeight(Math.max(0, Math.floor(bottom - top)));
    });
  }, [theme, insets.bottom, window.height, reserve]);

  useEffect(() => {
    onLayout();
  });
  useEffect(() => {
    const shown = Keyboard.addListener("keyboardDidShow", onLayout);
    const hidden = Keyboard.addListener("keyboardDidHide", onLayout);
    return () => {
      shown.remove();
      hidden.remove();
    };
  }, [onLayout]);

  return { blockRef, onLayout, minHeight };
}
