import { useTheme } from "@obop/ui-native";
import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import { Keyboard, type HostInstance } from "react-native";

/**
 * A block that reaches down to the footer of its screen (the Note of a trip, D6 and D7): its least
 * height is the room between its top and the footer's frame, less the padding at the bottom of the
 * screen, measured on the screen after each render and layout and when the keyboard comes or goes
 * (it makes the window of Android smaller). `footerRef` goes on the content of the footer,
 * `blockRef` and `onLayout` on the block, `minHeight` in its style.
 */
export function useFillToFooter(): {
  footerRef: RefObject<HostInstance | null>;
  blockRef: RefObject<HostInstance | null>;
  onLayout: () => void;
  minHeight: number;
} {
  const theme = useTheme();
  const footerRef = useRef<HostInstance>(null);
  const blockRef = useRef<HostInstance>(null);
  const [minHeight, setMinHeight] = useState(0);

  const onLayout = useCallback(() => {
    const block = blockRef.current;
    const footer = footerRef.current;
    if (!block || !footer) return;
    block.measureInWindow((_x, top) => {
      footer.measureInWindow((_fx, footerContentTop, _width, footerHeight) => {
        // Hidden (another screen over it), it measures nothing.
        if (footerHeight <= 0) return;
        // The footer's frame starts its padding and its top border above its content.
        setMinHeight(Math.max(0, Math.floor(footerContentTop - 1 - 2 * theme.space[4] - top)));
      });
    });
  }, [theme]);

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

  return { footerRef, blockRef, onLayout, minHeight };
}
