// @vitest-environment jsdom
import { describe, expect, it } from "vite-plus/test";
import { applyTheme, DEFAULT_THEME } from "@obop/ui";
import { clearInstanceTheme, sessionCounter } from "./sessionGuard.ts";

describe("the session counter", () => {
  it("tells work begun before a sign-out that it is stale", () => {
    const s = sessionCounter();
    const at = s.current();
    expect(s.live(at)).toBe(true);
    s.next();
    expect(s.live(at)).toBe(false);
    expect(s.live(s.current())).toBe(true);
  });
});

describe("clearing the theme of the instance", () => {
  it("leaves the page root as a fresh load has it", () => {
    const root = document.createElement("html");
    root.style.setProperty("--unrelated", "1");
    applyTheme({ ...DEFAULT_THEME, mode: "dark", density: "compact", radius: "round" }, root);
    expect(root.dataset.theme).toBe("dark");
    expect(root.style.getPropertyValue("--accent")).not.toBe("");
    clearInstanceTheme(root);
    expect(root.dataset.theme).toBeUndefined();
    expect(root.dataset.density).toBeUndefined();
    expect(root.dataset.radius).toBeUndefined();
    expect(root.style.getPropertyValue("--accent")).toBe("");
    expect(root.style.getPropertyValue("--accent-h")).toBe("");
    expect(root.style.getPropertyValue("--radius")).toBe("");
    expect(root.style.getPropertyValue("--unrelated")).toBe("1");
  });
});
