import { useCallback, useState } from "react";
import type { LayoutChangeEvent } from "react-native";

/**
 * A block at least as high as it is wide (the Note of a trip and of Manual add, D6 and D7): its
 * width is measured at each layout and is its least height; a longer note makes it grow and the
 * screen scroll. `onLayout` and `minHeight` go on the block.
 */
export function useSquare(): { onLayout: (e: LayoutChangeEvent) => void; minHeight: number } {
  const [width, setWidth] = useState(0);
  const onLayout = useCallback((e: LayoutChangeEvent) => {
    const measured = Math.round(e.nativeEvent.layout.width);
    setWidth((was) => (was === measured ? was : measured));
  }, []);
  return { onLayout, minHeight: width };
}
