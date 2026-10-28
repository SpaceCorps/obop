import { useState } from "react";
import { TextInput, View, type TextInputProps } from "react-native";
import { useTheme } from "../theme.tsx";
import { Text } from "./Text.tsx";

export type FieldProps = {
  label: string;
  value: string;
  onChangeText: (text: string) => void;
  placeholder?: string;
  /** Help under the field. */
  helper?: string;
  /** An error replaces the help, is announced and shown in words, not in colour alone. */
  error?: string;
  editable?: boolean;
  multiline?: boolean;
  secureTextEntry?: boolean;
  keyboardType?: TextInputProps["keyboardType"];
  autoCapitalize?: TextInputProps["autoCapitalize"];
  autoCorrect?: boolean;
  maxLength?: number;
  testID?: string;
};

export function Field({
  label,
  value,
  onChangeText,
  placeholder,
  helper,
  error,
  editable = true,
  multiline = false,
  secureTextEntry,
  keyboardType,
  autoCapitalize,
  autoCorrect,
  maxLength,
  testID,
}: FieldProps) {
  const t = useTheme();
  const [focused, setFocused] = useState(false);
  const note = error ?? helper;
  return (
    <View style={{ gap: t.space[1] }}>
      <Text variant="label">{label}</Text>
      <TextInput
        accessibilityLabel={label}
        accessibilityHint={note}
        accessibilityState={{ disabled: !editable }}
        value={value}
        onChangeText={onChangeText}
        placeholder={placeholder}
        placeholderTextColor={t.c.textSubtle}
        editable={editable}
        multiline={multiline}
        secureTextEntry={secureTextEntry}
        keyboardType={keyboardType}
        autoCapitalize={autoCapitalize}
        autoCorrect={autoCorrect}
        maxLength={maxLength}
        allowFontScaling
        maxFontSizeMultiplier={2}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        testID={testID}
        style={{
          minHeight: multiline ? t.control.height * 2 : t.control.height,
          paddingHorizontal: t.space[3],
          paddingVertical: t.space[2],
          fontSize: t.fontSize.md,
          color: t.c.text,
          backgroundColor: editable ? t.c.surface : t.c.surface3,
          borderRadius: t.radius.md,
          borderWidth: focused || error ? 2 : 1,
          borderColor: error ? t.c.danger.fill : focused ? t.c.accentFill : t.c.borderControl,
          textAlignVertical: multiline ? "top" : "center",
        }}
      />
      {note ? (
        <Text
          variant="caption"
          tone={error ? "danger" : "muted"}
          accessibilityRole={error ? "alert" : undefined}
          accessibilityLiveRegion={error ? "polite" : undefined}
        >
          {error ? `Error: ${error}` : note}
        </Text>
      ) : null}
    </View>
  );
}
