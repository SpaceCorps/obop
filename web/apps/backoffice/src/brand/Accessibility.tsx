import { useEffect, useRef, useState, type ReactNode } from "react";
import { Badge, Button, Checkbox, IconButton, Kbd, Notice, Table } from "@obop/ui";
import { Block, Specimen } from "./kit.tsx";

/** Shows the rendered size of whatever it wraps, measured, not assumed. */
function Measured({ label, children }: { label: string; children: ReactNode }) {
  const ref = useRef<HTMLSpanElement>(null);
  const [size, setSize] = useState("");
  useEffect(() => {
    const el = ref.current?.firstElementChild as HTMLElement | null;
    if (!el) return;
    const read = () => {
      const r = el.getBoundingClientRect();
      setSize(`${Math.round(r.width)} × ${Math.round(r.height)}px`);
    };
    read();
    const ro = new ResizeObserver(read);
    ro.observe(el);
    // density changes the size without resizing the element's box observer in some browsers
    const mo = new MutationObserver(read);
    mo.observe(document.documentElement, { attributes: true });
    return () => {
      ro.disconnect();
      mo.disconnect();
    };
  }, []);
  return (
    <div className="measured">
      <span ref={ref}>{children}</span>
      <span className="state-label">{label}</span>
      <code className="tabular">{size}</code>
    </div>
  );
}

export function Accessibility() {
  const reduced =
    typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  const forced = typeof matchMedia === "function" && matchMedia("(forced-colors: active)").matches;
  return (
    <>
      <Block
        id="a11y"
        title="Accessibility"
        lead="obop meets WCAG 2.2 level AA, and accessibility is built into the primitives, not added to screens afterwards. The European Accessibility Act applies to consumer-facing storefronts, checkouts and portals, so the defaults are conformant."
      >
        <div className="spec-row">
          <Specimen
            title="Target size"
            note="WCAG 2.5.8: 24 × 24px at least, including compact rows. Comfortable is 36px; touch density is 44 to 48px."
          >
            <div className="measure-grid">
              <Measured label="Button">
                <Button>Save</Button>
              </Measured>
              <Measured label="Small button">
                <Button size="sm">Save</Button>
              </Measured>
              <Measured label="Icon button">
                <IconButton icon="edit" label="Edit" tooltip={false} />
              </Measured>
              <Measured label="Checkbox row">
                <Checkbox label="Select" />
              </Measured>
            </div>
            <p className="hint">
              Switch to Compact in Theme and the sizes above update: nothing goes under 24px.
            </p>
          </Specimen>

          <Specimen
            title="Contrast"
            note="Measured in the Colour section and checked for every accent a person picks."
          >
            <Table dense caption="Contrast minimums">
              <thead>
                <tr>
                  <th scope="col">What</th>
                  <th scope="col" className="num">
                    Minimum
                  </th>
                </tr>
              </thead>
              <tbody>
                {[
                  ["Body and label text", "4.5:1"],
                  ["Large text (24px, or 19px bold)", "3:1"],
                  ["Outlines of inputs, icons that carry meaning", "3:1"],
                  ["Focus ring against the surface", "3:1"],
                  ["Accent fill, text on it, accent links", "4.5:1 / 3:1, auto-corrected"],
                ].map(([a, b]) => (
                  <tr key={a}>
                    <td>{a}</td>
                    <td className="num tabular">{b}</td>
                  </tr>
                ))}
              </tbody>
            </Table>
          </Specimen>

          <Specimen
            title="Focus"
            note="Every control shows a 2px accent outline with a 2px offset when reached by keyboard. It is never removed."
          >
            <div className="state-row">
              <Button data-state="focus">Focused button</Button>
              <IconButton icon="search" label="Search" data-state="focus" tooltip={false} />
              <Checkbox label="Focused" data-state="focus" />
            </div>
            <p className="hint">
              Sticky bars never cover a focused control: <code>scroll-padding</code> comes from
              tokens (WCAG 2.4.11).
            </p>
          </Specimen>
        </div>
      </Block>

      <Block
        id="a11y-keys"
        title="Keyboard map"
        lead="Every path completes without a pointer. Single-key shortcuts are avoided so they cannot clash with speech input (WCAG 2.1.4)."
      >
        <div className="table-wrap">
          <table className="table">
            <caption className="sr-only">Keyboard shortcuts</caption>
            <thead>
              <tr>
                <th scope="col">Where</th>
                <th scope="col">Keys</th>
                <th scope="col">Does</th>
              </tr>
            </thead>
            <tbody>
              {(
                [
                  [
                    "Everywhere",
                    ["Ctrl or ⌘", "K"],
                    "Open the command palette: apps, records and actions in one box.",
                  ],
                  ["Everywhere", ["Esc"], "Close the topmost menu, dialog, panel or search."],
                  [
                    "Tabs and segmented",
                    ["←", "→", "Home", "End"],
                    "Move between options; selection follows focus.",
                  ],
                  [
                    "Menus",
                    ["↓", "↑", "Enter", "Esc"],
                    "Move, choose, close and return focus to the trigger.",
                  ],
                  ["Search box", ["Alt", "↓"], "Open the filter, group and favourites panel."],
                  ["Search box", ["Backspace"], "Remove the last filter when the box is empty."],
                  [
                    "List",
                    ["↑", "↓", "←", "→"],
                    "Move between cells. Home and End jump to the row ends.",
                  ],
                  [
                    "List",
                    ["Enter", "Space", "F2"],
                    "Open the record, select the row, edit the cell.",
                  ],
                  ["Kanban", ["↑", "↓", "←", "→"], "Move between cards and columns."],
                  [
                    "Kanban",
                    ["Shift", "←", "→"],
                    "Move the focused card to the next or previous column.",
                  ],
                  ["Record panel", ["Ctrl or ⌘", "S"], "Save the record."],
                  ["Record panel", ["Esc"], "Close it (asks first if there are unsaved changes)."],
                ] as [string, string[], string][]
              ).map(([where, keys, does], i) => (
                <tr key={i}>
                  <th scope="row">{where}</th>
                  <td>
                    <span className="keys">
                      {keys.map((k) => (
                        <Kbd key={k}>{k}</Kbd>
                      ))}
                    </span>
                  </td>
                  <td>{does}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <Notice tone="info" title="Every drag has a twin">
          Dragging a card has the Move to select and Shift plus arrows; reordering apps has Move up
          and Move down. No function depends on dragging (WCAG 2.5.7).
        </Notice>
      </Block>

      <Block id="a11y-more" title="Motion, contrast modes and assistive technology">
        <div className="spec-row">
          <Specimen title="Reduced motion">
            <p>
              Respected through <code>prefers-reduced-motion</code>: durations collapse to near
              zero, shimmer and spin stop, and a static ring stands in for the spinner. State
              changes still happen instantly.
            </p>
            <p>
              <Badge tone={reduced ? "success" : "neutral"}>
                {reduced
                  ? "Reduced motion is on in this browser"
                  : "Reduced motion is off in this browser"}
              </Badge>
            </p>
          </Specimen>
          <Specimen title="Forced colours and zoom">
            <p>
              Panels, badges and chips gain a visible border in forced-colours mode; icons use
              <code> currentColor</code>. Layouts reflow down to 320px wide at 400% zoom without
              horizontal scrolling of the page, and survive the WCAG text-spacing override.
            </p>
            <p>
              <Badge tone={forced ? "success" : "neutral"}>
                {forced ? "Forced colours are on" : "Forced colours are off"}
              </Badge>
            </p>
          </Specimen>
          <Specimen title="Screen readers">
            <ul className="bb-list">
              <li>
                One <code>h1</code> per page; headings never skip a level.
              </li>
              <li>
                Toasts are a polite <code>role="status"</code> region; only errors use{" "}
                <code>role="alert"</code>.
              </li>
              <li>
                Icon buttons need a label; decorative icons are <code>aria-hidden</code>.
              </li>
              <li>
                Tables have captions; sortable headers state <code>aria-sort</code>.
              </li>
              <li>Records open in a labelled complementary region and return focus on close.</li>
            </ul>
          </Specimen>
        </div>
        <h3 className="bb-sub">Before a component ships</h3>
        <ul className="bb-checklist">
          {[
            "Reachable and operable with the keyboard alone, in a logical order.",
            "Name, role and state exposed (native element first, ARIA second).",
            "Visible focus; 3:1 contrast for the outline and 4.5:1 for text, in light and dark.",
            "Target at least 24 × 24px in compact density.",
            "Works at 320px wide, 400% zoom and with forced colours.",
            "Nothing depends on colour, drag or hover alone.",
            "No unlabeled controls in the accessibility smoke test of the e2e suite.",
          ].map((t) => (
            <li key={t}>{t}</li>
          ))}
        </ul>
      </Block>
    </>
  );
}
