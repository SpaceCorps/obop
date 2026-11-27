import { useTheme } from "@obop/ui-native";
import { useEffect, useRef, useState } from "react";
import { TextInput, type TextInputProps } from "react-native";

/**
 * A text that is changed where it stands (D6): it looks like the text around it, with no frame;
 * a tap puts the cursor in it, and what was typed is kept when it loses the focus (a tap
 * elsewhere, the key Done of a line, the keyboard closed), or when the screen goes while it is
 * being typed. An empty field shows its placeholder ("Not given", "Empty" in grey, the default
 * name of a trip in black) while it is not being typed in; with the cursor in it, it is simply
 * empty. What was typed is black. The stored
 * value comes back in it when it changes elsewhere and the field is not being typed in.
 */
export function InlineText({
  value,
  placeholder,
  label,
  onCommit,
  onChange,
  keyboardType,
  look,
  maxLength,
  placeholderTone = look === "label" ? "text" : "muted",
  editable = true,
  testID,
}: {
  /** As stored; `null` shows the placeholder. */
  value: string | null;
  placeholder: string;
  /** What it is, for a screen reader: Name, From, To, Note. */
  label: string;
  /** Called with the text when it differs from `value`; the caller stores it. */
  onCommit?: (text: string) => void;
  /**
   * Called on every change: for a form whose Save may be pressed while the cursor is still in the
   * field (D7), not only when it loses the focus.
   */
  onChange?: (text: string) => void;
  keyboardType?: TextInputProps["keyboardType"];
  /**
   * `label` (the name in the current widget, D6; its default in black, as the name it stands for),
   * `km` (the big km of the widget of a trip added by hand, D7), `display` (a part of its big
   * duration: hours, minutes or seconds), `value` (at the right of a row), `note` (lines).
   */
  look: "label" | "km" | "display" | "value" | "note";
  maxLength: number;
  /**
   * The colour of the placeholder: grey (`muted`) for an empty field; black (`text`) for the
   * default name of a trip, which is its name until it is named. By default black for `label`.
   */
  placeholderTone?: "muted" | "text";
  /** `false`: read only, as the trip's screen until Edit is pressed; it looks the same. */
  editable?: boolean;
  testID?: string;
}) {
  const theme = useTheme();
  const [text, setText] = useState(value ?? "");
  const focused = useRef(false);
  // The same, for the screen: an empty field shows no placeholder while it is being typed in.
  const [editing, setEditing] = useState(false);
  // The latest of both for the commit when the screen goes.
  const latest = useRef({ text, value, onCommit });
  latest.current = { text, value, onCommit };

  useEffect(() => {
    if (!focused.current) setText(value ?? "");
  }, [value]);

  const commit = () => {
    const { text: typed, value: stored, onCommit: save } = latest.current;
    if (save && typed !== (stored ?? "")) save(typed);
  };

  // Typed and not yet kept when the screen goes (Back while the cursor is in it): keep it.
  useEffect(
    () => () => {
      if (focused.current) commit();
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  // The km and the parts of the duration are as wide as their text.
  const fit = look === "km" || look === "display";
  const font =
    look === "display"
      ? // No line height: Android gives it to the text typed and not to the placeholder, so the
        // typed digits stood lower than the grey "00".
        { fontSize: theme.fontSize["4xl"], fontWeight: theme.fontWeight.bold }
      : look === "label"
        ? { fontSize: theme.fontSize.md, fontWeight: theme.fontWeight.semibold }
        : look === "km"
          ? { fontSize: theme.fontSize["2xl"], fontWeight: theme.fontWeight.bold }
          : { fontSize: theme.fontSize.md, fontWeight: theme.fontWeight.regular };
  return (
    <TextInput
      value={text}
      onChangeText={(next) => {
        setText(next);
        onChange?.(next);
      }}
      keyboardType={keyboardType}
      editable={editable}
      onFocus={() => {
        focused.current = true;
        setEditing(true);
      }}
      onBlur={() => {
        focused.current = false;
        setEditing(false);
        commit();
        // Spaces only are nothing: the field is empty again, and its placeholder shows.
        if (latest.current.text.trim() === "") setText("");
      }}
      // The km ("0.0") and the parts of the duration ("00") keep their placeholder while they are
      // typed in: they are as wide as their text, and an empty field would have no width and
      // lose the focus.
      placeholder={editing && !fit ? undefined : placeholder}
      // An empty field is grey, what was typed is black; the default name of a trip is its name
      // until it is named, so it is not greyed where it stands for the trip's name.
      placeholderTextColor={placeholderTone === "text" ? theme.c.text : theme.c.textMuted}
      maxLength={maxLength}
      multiline={look === "note"}
      // A name and a place end with Done; a note takes new lines and is kept when it loses the focus.
      submitBehavior={look === "note" ? "newline" : "blurAndSubmit"}
      returnKeyType={look === "note" ? "default" : "done"}
      underlineColorAndroid="transparent"
      accessibilityLabel={label}
      testID={testID}
      style={{
        ...font,
        color: theme.c.text,
        textAlign: look === "value" ? "right" : "left",
        padding: 0,
        margin: 0,
        borderWidth: 0,
        // A value takes the rest of its row; a note the rest of its block, so a tap anywhere in it
        // puts the cursor there. The km and the parts of the duration are as wide as their text.
        ...(fit ? {} : { flex: 1 }),
        ...(look === "note" ? { textAlignVertical: "top" as const } : {}),
      }}
    />
  );
}
