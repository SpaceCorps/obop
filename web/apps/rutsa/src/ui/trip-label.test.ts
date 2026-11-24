import { describe, expect, test } from "vite-plus/test";
import { tripName, tripTitle, typedName } from "./trip-label.ts";

const none = { name: null, from: null, to: null };

describe("what a trip is called on a line", () => {
  test("its name first", () => {
    expect(tripTitle({ name: "Meeting", from: "Stockholm", to: "Uppsala" })).toBe("Meeting");
  });

  test("without a name, where it went", () => {
    expect(tripTitle({ ...none, from: "Stockholm", to: "Uppsala" })).toBe("Stockholm → Uppsala");
  });

  test("only one of the places: that one", () => {
    expect(tripTitle({ ...none, from: "Stockholm" })).toBe("Stockholm");
    expect(tripTitle({ ...none, to: "Uppsala" })).toBe("Uppsala");
  });

  test("nothing typed: no title, the screen shows the times", () => {
    expect(tripTitle(none)).toBeNull();
  });

  test("a name or a place of spaces only counts as nothing", () => {
    expect(tripTitle({ name: "  ", from: " ", to: "Uppsala" })).toBe("Uppsala");
  });
});

describe("the name of a trip on its screen", () => {
  const untitled = (number: number | null) => `Untitled trip ${number ?? "?"}`;

  test("its name when it has one", () => {
    expect(tripName({ name: "Meeting", number: 12 }, untitled)).toBe("Meeting");
  });

  test("without one, the default of its number; places are not its name", () => {
    expect(tripName({ name: null, number: 12 }, untitled)).toBe("Untitled trip 12");
    expect(tripName({ name: "   ", number: 3 }, untitled)).toBe("Untitled trip 3");
  });
});

describe("the name typed in the window of a trip's details", () => {
  test("the default left as it was is no name", () => {
    expect(typedName("Untitled trip 12", "Untitled trip 12")).toBeNull();
    expect(typedName(" Untitled trip 12 ", "Untitled trip 12")).toBeNull();
  });

  test("a changed name is kept, trimmed; an empty one is none", () => {
    expect(typedName(" Meeting ", "Untitled trip 12")).toBe("Meeting");
    expect(typedName("Untitled trip 13", "Untitled trip 12")).toBe("Untitled trip 13");
    expect(typedName("", "Untitled trip 12")).toBeNull();
  });

  test("a trip that has a name has no default to compare with", () => {
    expect(typedName("Meeting", null)).toBe("Meeting");
  });
});
