// @vitest-environment jsdom
// The time box of a date and time's calendar (docs/ISSUES.md S5): the 24-hour clock that the
// form's other time boxes use, typed as text, not the browser's time box ("09:00 AM" in English).
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import type { FieldMeta } from "../api.ts";
import { setTime } from "../api.ts";
import { DatetimeInput } from "./DateInput.tsx";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const field: FieldMeta = {
  name: "starts_at",
  kind: "datetime",
  description: "",
  class: "none",
  personal_data: false,
  required: false,
};

let container: HTMLDivElement;
let root: Root;

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en");
  setTime(Intl.DateTimeFormat().resolvedOptions().timeZone);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("the time box of a date and time's calendar", () => {
  it("is a text box on the 24-hour clock that takes 1430", async () => {
    const onChange = vi.fn();
    await act(async () =>
      root.render(
        <DatetimeInput field={field} value="2026-11-20 09:00" onChange={onChange} common={{}} />,
      ),
    );
    const open = container.querySelector<HTMLButtonElement>("button.calbtn")!;
    await act(async () => open.click());
    const box = container.querySelector<HTMLInputElement>(".caltime input")!;
    expect(box.type).toBe("text");
    expect(box.value).toBe("09:00");

    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    await act(async () => {
      set.call(box, "1430");
      box.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () => {
      box.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    });
    expect(onChange).toHaveBeenLastCalledWith("2026-11-20 14:30");
  });
});
