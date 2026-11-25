import { describe, expect, test } from "vite-plus/test";
import { shareRoom } from "./home-room.ts";

const LIMITS = { maxRows: 5, minButton: 88, maxButton: 144 };

describe("the room of the round button and Recent trips", () => {
  test("five rows, and the round button takes the rest (the emulator: 389 dp, rows of 55.5)", () => {
    expect(shareRoom(389, 55.5, LIMITS)).toEqual({ rows: 5, button: 111 });
  });

  test("never more than five rows: a tall screen gives the button more, up to 144", () => {
    expect(shareRoom(420, 55.5, LIMITS)).toEqual({ rows: 5, button: 142 });
    expect(shareRoom(600, 55.5, LIMITS)).toEqual({ rows: 5, button: 144 });
  });

  test("a short screen (or a banner) takes rows away before the button gets smaller than 88", () => {
    // 5 rows and 88 need 365.5: one row fewer, and the button grows with the room left.
    expect(shareRoom(360, 55.5, LIMITS)).toEqual({ rows: 4, button: 138 });
    expect(shareRoom(300, 55.5, LIMITS)).toEqual({ rows: 3, button: 133 });
  });

  test("at least one row, and the button not under 88", () => {
    expect(shareRoom(100, 55.5, LIMITS)).toEqual({ rows: 1, button: 88 });
  });

  test("the share is stable: measured again with the button it gave, it gives the same", () => {
    const first = shareRoom(389, 55.5, LIMITS);
    // A bigger button moves the list down as much, so the room is the same.
    expect(shareRoom(389, 55.5, LIMITS)).toEqual(first);
  });
});
