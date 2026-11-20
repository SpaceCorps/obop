import { useWorded, type Wording } from "@obop/ui";
import { useRef, useState } from "react";
import type { FieldMeta, Row } from "../api.ts";
import { fromEdit, sameAsStored, toEdit, type Edit } from "../edit.ts";
import { FieldInput } from "../fields.tsx";
import { rowCurrency } from "../format.ts";

/**
 * A cell turned into an input: Enter saves, Escape cancels, a select or switch saves when changed.
 * Tab and Shift+Tab save and move to the next or previous editable cell of the row (`advance`), as
 * in a spreadsheet. Leaving the cell (a click elsewhere) saves what was changed too (#87); only
 * Escape cancels. A value the server refuses keeps the cell open with the reason.
 */
export function InlineEdit(props: {
  field: FieldMeta;
  row: Row;
  update: (id: string, patch: Record<string, unknown>) => Promise<Wording | null>;
  done: () => void;
  /** Saved: go on to the next (1) or previous (-1) editable cell of the row. */
  advance?: (dir: 1 | -1) => void;
  optionState?: (option: string) => { disabled: boolean; title?: string };
}) {
  const { field: f, row } = props;
  const currency = rowCurrency(row);
  const [value, setValue] = useState<Edit>(() => toEdit(f, row[f.name], currency));
  const [error, setError] = useWorded();
  const [busy, setBusy] = useState(false);
  // Set once the cell is done (saved, moved on): a blur that follows must not end the next cell.
  const finished = useRef(false);

  async function commit(v: Edit, then: () => void = props.done) {
    // An unset switch shown as No is a change when saved: it writes an explicit false (#98).
    const same = sameAsStored(f, row[f.name], v, currency);
    const r = fromEdit(f, v, currency, same);
    if ("error" in r) return setError(() => r.error);
    const finish = () => {
      finished.current = true;
      then();
    };
    if (same) return finish();
    setBusy(true);
    const err = await props.update(row.id, { [f.name]: r.value });
    setBusy(false);
    if (err) setError(err);
    else finish();
  }

  return (
    <div
      className="inline-edit"
      onClick={(e) => e.stopPropagation()}
      onBlur={(e) => {
        if (finished.current || busy) return;
        if (e.currentTarget.contains(e.relatedTarget as Node | null)) return;
        // Only what was typed is saved on the way out: a look at an unset switch writes nothing.
        if (value === toEdit(f, row[f.name], currency)) props.done();
        else void commit(value);
      }}
    >
      <FieldInput
        field={f}
        value={value}
        autoFocus
        compact
        currency={currency}
        optionState={props.optionState}
        invalid={!!error}
        readOnly={busy}
        unchanged={toEdit(f, row[f.name], currency) === value}
        describedBy={error ? `ie-${row.id}-${f.name}` : undefined}
        onChange={(v) => {
          setError(null);
          setValue(v);
          if (f.kind === "select" || f.kind === "bool") void commit(v);
        }}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            finished.current = true;
            props.done();
          } else if (e.key === "Tab" && props.advance) {
            e.preventDefault();
            e.stopPropagation();
            const dir = e.shiftKey ? -1 : 1;
            void commit(value, () => props.advance!(dir));
          } else if (
            e.key === "Enter" &&
            !(e.target instanceof HTMLTextAreaElement && e.shiftKey)
          ) {
            e.preventDefault();
            e.stopPropagation();
            void commit(value);
          }
        }}
      />
      {error && (
        <span id={`ie-${row.id}-${f.name}`} role="alert" className="ie-error">
          {error}
        </span>
      )}
    </div>
  );
}
