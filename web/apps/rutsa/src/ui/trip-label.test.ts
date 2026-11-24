import { describe, expect, test } from "vite-plus/test";
import { tripTitle } from "./trip-label.ts";

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
