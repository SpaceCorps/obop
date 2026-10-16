import { useState } from "react";
import type { FieldMeta, Row } from "../api.ts";
import { fromEdit, toEdit, type Edit } from "../edit.ts";
import { FieldInput } from "../fields.tsx";

/** A cell turned into an input: Enter saves, Escape cancels, a select or switch saves when changed. */
export function InlineEdit(props: {
  field: FieldMeta;
  row: Row;
  update: (id: string, patch: Record<string, unknown>) => Promise<string | null>;
  done: () => void;
  optionState?: (option: string) => { disabled: boolean; title?: string };
}) {
  const { field: f, row } = props;
  const [value, setValue] = useState<Edit>(() => toEdit(f, row[f.name]));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function commit(v: Edit) {
    const r = fromEdit(f, v);
    if ("error" in r) return setError(r.error);
    if (toEdit(f, row[f.name]) === v) return props.done();
    setBusy(true);
    const err = await props.update(row.id, { [f.name]: r.value });
    setBusy(false);
    if (err) setError(err);
    else props.done();
  }

  return (
    <div
      className="inline-edit"
      onClick={(e) => e.stopPropagation()}
      onBlur={(e) => {
        if (!busy && !e.currentTarget.contains(e.relatedTarget as Node | null)) props.done();
      }}
    >
      <FieldInput
        field={f}
        value={value}
        autoFocus
        compact
        optionState={props.optionState}
        invalid={!!error}
        readOnly={busy}
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
