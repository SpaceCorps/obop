// @vitest-environment jsdom
// R16: a dialog whose first field focuses itself (`autoFocus`, as ActionDialog's Reject reason does)
// must still hand focus back to the button that opened it, and the field keeps focus while it is open.
import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it } from "vite-plus/test";
import { Dialog } from "@obop/ui";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | null = null;
afterEach(() => {
  act(() => root?.unmount());
  root = null;
  document.body.innerHTML = "";
});

function Page() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button onClick={() => setOpen(true)}>Reject</button>
      {open && (
        <Dialog open title="Reject" onClose={() => setOpen(false)}>
          <textarea aria-label="Reason" autoFocus />
          <button onClick={() => setOpen(false)}>Cancel</button>
        </Dialog>
      )}
    </>
  );
}

it("gives focus back to the opener after a dialog whose field took focus itself", async () => {
  const host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() => root!.render(<Page />));
  const opener = [...document.querySelectorAll("button")].find((b) => b.textContent === "Reject")!;
  opener.focus();
  act(() => opener.click());
  expect(document.activeElement?.tagName).toBe("TEXTAREA");
  const cancel = [...document.querySelectorAll("button")].find((b) => b.textContent === "Cancel")!;
  act(() => cancel.click());
  await act(() => new Promise((done) => setTimeout(done, 50)));
  expect(document.activeElement).toBe(opener);
});
