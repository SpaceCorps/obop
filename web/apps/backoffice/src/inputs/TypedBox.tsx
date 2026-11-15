// Draws the reading form of a value ("1 h 30", "12,450.00") over a box that holds its typing form
// ("1:30", "12450.00") while the box does not have the focus (common.ts `useTypedText`). Divs, not
// spans: the money box styles every span inside it as its currency sign.
import type { ReactNode } from "react";

export function TypedBox({ shown, children }: { shown: string | null; children: ReactNode }) {
  return (
    <div className={`typedbox${shown ? " showing" : ""}`}>
      {children}
      {shown && (
        <div className="typedshow" aria-hidden="true">
          {shown}
        </div>
      )}
    </div>
  );
}
