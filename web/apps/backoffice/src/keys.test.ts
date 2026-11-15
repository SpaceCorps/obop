import { describe, expect, it } from "vite-plus/test";
import { isApple, modKey, shortcut } from "./keys.ts";

describe("modKey", () => {
  it("prints the Command sign on Apple platforms and Ctrl elsewhere", () => {
    expect(modKey("MacIntel")).toBe("⌘");
    expect(modKey("iPhone")).toBe("⌘");
    expect(modKey("Win32")).toBe("Ctrl");
    expect(modKey("Linux x86_64")).toBe("Ctrl");
    expect(shortcut("K", "macOS")).toBe("⌘K");
    expect(shortcut("K", "Windows")).toBe("Ctrl+K");
    expect(isApple("Android")).toBe(false);
  });
});
