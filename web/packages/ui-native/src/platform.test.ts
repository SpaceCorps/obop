import { describe, expect, test } from "vite-plus/test";
import { bannerAnnouncement, screenAvoidsKeyboard } from "./platform.ts";

describe("the keyboard", () => {
  test("a screen pads itself on iOS only", () => {
    expect(screenAvoidsKeyboard("ios")).toBe(true);
    expect(screenAvoidsKeyboard("android")).toBe(false);
    expect(screenAvoidsKeyboard("web")).toBe(false);
  });
});

describe("a banner that appears", () => {
  test("a warning or a problem is announced on iOS, with the words that the banner speaks", () => {
    expect(bannerAnnouncement("ios", "danger", "Problem: No network")).toBe("Problem: No network");
    expect(bannerAnnouncement("ios", "warning", "Warning: Slow")).toBe("Warning: Slow");
  });

  test("a note or a success is not announced", () => {
    expect(bannerAnnouncement("ios", "info", "Note: x")).toBeNull();
    expect(bannerAnnouncement("ios", "success", "Done: x")).toBeNull();
  });

  test("Android and the web have a live region of their own", () => {
    expect(bannerAnnouncement("android", "danger", "x")).toBeNull();
    expect(bannerAnnouncement("web", "danger", "x")).toBeNull();
  });
});
