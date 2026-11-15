import { useRef, useState } from "react";
import type { FieldMeta, Row } from "../api.ts";
import { fromEdit, toEdit, type Edit } from "../edit.ts";
import { FieldInput } from "../fields.tsx";
import { rowCurrency } from "../format.ts";

/**
 * A cell turned into an input: Enter saves, Escape cancels, a select or switch saves when changed.
 * Tab and Shift+Tab save and move to the next or previous editable cell of the row (`advance`), as
 * in a spreadsheet; leaving the cell any other way cancels.
 */
export function InlineEdit(props: {
  field: FieldMeta;
  row: Row;
  update: (id: string, patch: Record<string, unknown>) => Promise<string | null>;
  done: () => void;
  /** Saved: go on to the next (1) or previous (-1) editable cell of the row. */
  advance?: (dir: 1 | -1) => void;
  optionState?: (option: string) => { disabled: boolean; title?: string };
}) {
  const { field: f, row } = props;
  const currency = rowCurrency(row);
  const [value, setValue] = useState<Edit>(() => toEdit(f, row[f.name], currency));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // Set once the cell is done (saved, moved on): a blur that follows must not end the next cell.
  const finished = useRef(false);

  async function commit(v: Edit, then: () => void = props.done) {
    const same = toEdit(f, row[f.name], currency) === v;
    const r = fromEdit(f, v, currency, same);
    if ("error" in r) return setError(r.error);
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
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) props.done();
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
