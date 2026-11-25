import { useTheme } from "@obop/ui-native";
import { useEffect, useRef, useState } from "react";
import { TextInput } from "react-native";

/**
 * A text that is changed where it stands (D6): it looks like the text around it, with no frame;
 * a tap puts the cursor in it, and what was typed is kept when it loses the focus (a tap
 * elsewhere, the key Done of a line, the keyboard closed), or when the screen goes while it is
 * being typed. An empty field shows its placeholder in grey ("Not given", "Empty", the default
 * name) while it is not being typed in; with the cursor in it, it is simply empty. The stored
 * value comes back in it when it changes elsewhere and the field is not being typed in.
 */
export function InlineText({
  value,
  placeholder,
  label,
  onCommit,
  look,
  maxLength,
  restoreWhenEmpty = false,
  testID,
}: {
  /** As stored; `null` shows the placeholder. */
  value: string | null;
  placeholder: string;
  /** What it is, for a screen reader: Name, From, To, Note. */
  label: string;
  /** Called with the text when it differs from `value`; the caller stores it. */
  onCommit: (text: string) => void;
  /** `title` (the name, as the title of the screen), `value` (at the right of a row), `note` (lines). */
  look: "title" | "value" | "note";
  maxLength: number;
  /**
   * An empty field shows `value` again when it loses the focus: for the name, whose `value` is the
   * default while the trip has none, so clearing it brings the default back at once.
   */
  restoreWhenEmpty?: boolean;
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
    if (typed !== (stored ?? "")) save(typed);
  };

  // Typed and not yet kept when the screen goes (Back while the cursor is in it): keep it.
  useEffect(
    () => () => {
      if (focused.current) commit();
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  const font =
    look === "title"
      ? { fontSize: theme.fontSize["2xl"], fontWeight: theme.fontWeight.bold }
      : { fontSize: theme.fontSize.md, fontWeight: theme.fontWeight.regular };
  return (
    <TextInput
      value={text}
      onChangeText={setText}
      onFocus={() => {
        focused.current = true;
        setEditing(true);
      }}
      onBlur={() => {
        focused.current = false;
        setEditing(false);
        commit();
        if (restoreWhenEmpty && latest.current.text.trim() === "")
          setText(latest.current.value ?? "");
      }}
      placeholder={editing ? undefined : placeholder}
      placeholderTextColor={theme.c.textMuted}
      maxLength={maxLength}
      multiline={look !== "value"}
      // A name and a place end with Done; a note takes new lines and is kept when it loses the focus.
      submitBehavior={look === "note" ? "newline" : "blurAndSubmit"}
      returnKeyType={look === "note" ? "default" : "done"}
      underlineColorAndroid="transparent"
      accessibilityLabel={label}
      testID={testID}
      style={{
        ...font,
        color: look === "value" ? theme.c.textMuted : theme.c.text,
        textAlign: look === "value" ? "right" : "left",
        padding: 0,
        margin: 0,
        borderWidth: 0,
        // A value takes the rest of its row; a note the rest of its block, so a tap anywhere in it
        // puts the cursor there.
        ...(look === "title" ? {} : { flex: 1 }),
        ...(look === "note" ? { textAlignVertical: "top" as const } : {}),
      }}
    />
  );
}
