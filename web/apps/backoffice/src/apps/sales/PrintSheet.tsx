// The one thing on paper: a printable document (a quotation, a receipt) rendered straight under
// <body>, outside the side panel, the phone sheet or a dialog. Under print media every other child of
// <body> is taken out of the layout (display: none, not visibility: hidden), so the document starts
// at the top of the first page, uses the full width, is printed once, and is dark on white whatever
// the screen theme. On screen it is not shown.
import type { ReactNode } from "react";
import { createPortal } from "react-dom";
import "./print.css";

export function PrintSheet(p: { children: ReactNode; className?: string }) {
  if (typeof document === "undefined") return null;
  return createPortal(
    <div className={`print-sheet ${p.className ?? ""}`.trim()} aria-hidden="true">
      {p.children}
    </div>,
    document.body,
  );
}
