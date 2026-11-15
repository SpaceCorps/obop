import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { Fragment, type ReactElement } from "react";
import { describe, expect, test, vi } from "vite-plus/test";
import type { Progress } from "../core/progress.ts";

// The two components are stand-ins that only name themselves: nothing is rendered here (there is no
// renderer in this app). `SignalNote` has no state and no hook, so it is called as a function and
// the elements it returns are read.
vi.mock("@obop/ui-native", () => ({
  Banner: function Banner() {
    return null;
  },
  Text: function Text() {
    return null;
  },
}));

const { Banner, Text } = await import("@obop/ui-native");
const { SignalNote } = await import("./SignalNote.tsx");

const measuredWith = (signal: Progress["signal"]): Progress => ({
  points: 7,
  metres: 0,
  lastFixMs: 1,
  signal,
});

/** The two elements the note makes: the banner and the muted line under it. */
function drawn(measured: Progress | null) {
  const root = SignalNote({ measured }) as ReactElement<{ children: ReactElement[] }> | null;
  if (root === null) return null;
  expect(root.type).toBe(Fragment);
  const [banner, line, ...rest] = root.props.children as [ReactElement, ReactElement];
  expect(rest).toEqual([]);
  return { banner, line };
}

describe("the note about a distance of 0 m, as the Recording screen draws it", () => {
  test("nothing before the stored positions have been read, and nothing when there is nothing to say", () => {
    expect(SignalNote({ measured: null })).toBeNull();
    expect(SignalNote({ measured: measuredWith(null) })).toBeNull();
  });

  test("weak: a warning banner with fixed words, then a muted line with the counts", () => {
    const note = drawn(measuredWith({ kind: "weak", dropped: 7, stored: 9, accuracyM: 150 }))!;
    expect(note.banner.type).toBe(Banner);
    expect(note.banner.props).toEqual({
      tone: "warning",
      title: "The location signal is too weak to measure a distance",
      message:
        "Positions that are too imprecise are left out of the distance. If you are indoors, move to a window or go outside.",
      testID: "weak-signal",
    });
    expect(note.line.type).toBe(Text);
    expect(note.line.props).toEqual({
      tone: "muted",
      testID: "weak-signal-detail",
      children: "7 of 9 positions were left out (the last one was only known to within 150 m)",
    });
  });

  test("not fitting: the banner and the line carry the other kind's ids and words", () => {
    const note = drawn(measuredWith({ kind: "jumpy", dropped: 8, stored: 9 }))!;
    expect(note.banner.type).toBe(Banner);
    expect(note.banner.props).toMatchObject({
      tone: "warning",
      title: "The positions do not fit together",
      testID: "jumpy-signal",
    });
    expect(note.line.props).toEqual({
      tone: "muted",
      testID: "jumpy-signal-detail",
      children: "8 of 9 positions were left out",
    });
  });

  test("the banner is given no count and no action: its text stays while the kind stays", () => {
    const first = drawn(measuredWith({ kind: "weak", dropped: 3, stored: 4, accuracyM: null }))!;
    const later = drawn(measuredWith({ kind: "weak", dropped: 40, stored: 41, accuracyM: 230 }))!;
    expect(later.banner.props).toEqual(first.banner.props);
    expect(Object.keys(first.banner.props as object).sort()).toEqual([
      "message",
      "testID",
      "title",
      "tone",
    ]);
    // The counts moved to the line, which differs.
    expect(later.line.props).not.toEqual(first.line.props);
  });
});

/**
 * Text, not rendering: the place of the note in `Recording.tsx` is also read from the source of the
 * file. The same is checked on the elements the screen makes, and what the screen measures from the
 * stored positions is run with fake ports, in `screens/Recording.test.tsx`; this is the second look
 * at the file itself: the note is drawn once, with what the screen measured, outside any condition,
 * after the banners and before the clock.
 */
describe("the place of the note in the Recording screen (the text of the file)", () => {
  const source = readFileSync(
    fileURLToPath(new URL("./screens/Recording.tsx", import.meta.url)),
    "utf8",
  );
  const lines = source.split("\n").map((line) => line.trim());
  const element = "<SignalNote measured={measured} />";
  const at = lines.indexOf(element);

  test("it is drawn once, with `measured`", () => {
    expect(lines.filter((line) => line.includes("<SignalNote"))).toEqual([element]);
    expect(source).toContain("const [measured, setMeasured] = useState<Progress | null>(null);");
  });

  test("it follows the last banner and comes before the clock, outside any condition", () => {
    const previous = lines.slice(0, at).filter((line) => !line.startsWith("{/*"));
    const next = lines.slice(at + 1).filter((line) => line !== "");
    expect(previous.at(-1)).toBe(") : null}");
    expect(lines.findIndex((line) => line.includes('t("recording.full.message")'))).toBeLessThan(
      at,
    );
    expect(next[0]).toBe('<Text variant="display" accessibilityRole="timer" testID="elapsed">');
  });
});
