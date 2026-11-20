import { useEffect, useRef } from "react";

/**
 * A refusal kept on a dialog or a sheet, announced as an alert. The keyboard focus goes to it when
 * it appears or its words change, so that the person reads it at once: on a phone the bottom sheet
 * scrolls it into view, where a line under the visible part of the sheet would say nothing.
 * A field's own message (aria-invalid) keeps its own rule: the focus goes to the field.
 */
export function Refusal({ message }: { message: string }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => ref.current?.focus(), [message]);
  return (
    <div ref={ref} tabIndex={-1} className="confirm-error" role="alert">
      {message}
    </div>
  );
}
