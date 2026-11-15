// What every field input shares: the attributes FieldInput computes once (id, focus, ARIA, keys)
// and the hook that lets an input show a value one way and take it typed another way.
import { useState, type KeyboardEvent } from "react";
import type { FieldMeta } from "../api.ts";
import type { Edit } from "../edit.ts";

/** The attributes FieldInput hands to the control it draws. */
export type CommonAttrs = {
  id?: string;
  autoFocus?: boolean;
  "aria-invalid"?: boolean;
  "aria-describedby"?: string;
  "aria-required"?: boolean;
  onKeyDown?: (e: KeyboardEvent) => void;
};

/** The props of every input in this folder. */
export type BaseInputProps = {
  field: FieldMeta;
  /** What the form holds for the field (edit.ts `toEdit`): text, or a boolean for a switch. */
  value: Edit;
  onChange: (v: Edit) => void;
  readOnly?: boolean;
  common: CommonAttrs;
  /** A cell of a table (lines, inline edit): tighter, no hints under the box. */
  compact?: boolean;
};

/**
 * Text that reads one way and is typed another. The box always holds the typing form ("1:30",
 * "12450.00"); while it does not have the focus, `shown` is the reading form ("1 h 30",
 * "12,450.00") that <TypedBox> draws over it. The box's own value never changes as it takes the
 * focus, so a selection, a select-all or an automated fill behaves as in any other box. While it
 * has the focus it holds exactly what the person types; `emit` turns that into what the form keeps
 * (the stored unit, or the text itself when it does not read, so the form's own check names the
 * problem).
 */
export function useTypedText(
  value: Edit,
  show: (v: string) => string,
  edit: (v: string) => string,
  emit: (typed: string) => string,
  onChange: (v: Edit) => void,
) {
  const [focused, setFocused] = useState(false);
  const [text, setText] = useState("");
  const typing = edit(String(value));
  const reading = show(String(value));
  return {
    focused,
    text: focused ? text : typing,
    /** The reading form to draw over the box, or null when it reads as it is typed. */
    shown: !focused && reading !== typing ? reading : null,
    onFocus: () => {
      setText(typing);
      setFocused(true);
    },
    onBlur: () => setFocused(false),
    onChange: (typed: string) => {
      setText(typed);
      onChange(emit(typed));
    },
    /** Replace the text, as a pick from a popover does. */
    set: (typed: string) => {
      setText(typed);
      onChange(emit(typed));
    },
  };
}

/** Whether `s` is a whole number as the form holds it. */
export const isInt = (s: string) => /^-?\d+$/.test(s.trim());
