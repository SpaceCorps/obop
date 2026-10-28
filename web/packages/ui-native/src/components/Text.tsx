import type { ReactNode } from "react";
import { Text as RNText, type TextProps as RNTextProps } from "react-native";
import { useTheme } from "../theme.tsx";

export type TextVariant = "display" | "title" | "heading" | "body" | "label" | "caption";
export type TextTone = "default" | "muted" | "danger" | "accent" | "onAccent";

export type TextProps = Omit<RNTextProps, "style" | "children"> & {
  variant?: TextVariant;
  tone?: TextTone;
  /** A colour of the palette that the tone does not cover (the text on a button, a banner). */
  color?: string;
  children: ReactNode;
};

/** Text that scales with the system font size, up to twice (the 200 percent of the checklist). */
export function Text({
  variant = "body",
  tone = "default",
  color: override,
  children,
  ...rest
}: TextProps) {
  const t = useTheme();
  const size = {
    display: t.fontSize["4xl"],
    title: t.fontSize["2xl"],
    heading: t.fontSize.lg,
    body: t.fontSize.md,
    label: t.fontSize.md,
    caption: t.fontSize.sm,
  }[variant];
  const weight =
    variant === "display" || variant === "title"
      ? t.fontWeight.bold
      : variant === "heading" || variant === "label"
        ? t.fontWeight.semibold
        : t.fontWeight.regular;
  const toneColor = {
    default: t.c.text,
    muted: t.c.textMuted,
    danger: t.c.danger.ink,
    accent: t.c.accentInk,
    onAccent: t.c.onAccent,
  }[tone];
  return (
    <RNText
      accessibilityRole={variant === "title" || variant === "heading" ? "header" : undefined}
      allowFontScaling
      maxFontSizeMultiplier={2}
      style={{
        color: override ?? toneColor,
        fontSize: size,
        fontWeight: weight,
        lineHeight: Math.round(
          size * (variant === "display" ? t.lineHeight.tight : t.lineHeight.snug),
        ),
      }}
      {...rest}
    >
      {children}
    </RNText>
  );
}
