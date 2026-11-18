// @vitest-environment jsdom
// D8: a confirm question opened before a change of language words its default buttons when it is
// drawn, so they follow the new language.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import { ConfirmDialog, type ConfirmRequest } from "./ui.tsx";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root;
let container: HTMLDivElement;

beforeEach(async () => {
  provideCatalogue("en", {});
  provideCatalogue("de", { Discard: "Verwerfen", "Keep editing": "Weiter bearbeiten" });
  await setLanguage("en", false);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(async () => {
  act(() => root.unmount());
  container.remove();
  await setLanguage("en", false);
});

it("words the default buttons in the language of the moment (D8)", async () => {
  const req: ConfirmRequest = { message: "Leave?", resolve: () => {} };
  await act(async () => root.render(<ConfirmDialog req={req} />));
  const buttons = () => [...document.querySelectorAll("button")].map((b) => b.textContent);
  expect(buttons()).toEqual(expect.arrayContaining(["Keep editing", "Discard"]));
  await setLanguage("de", false);
  await act(async () => root.render(<ConfirmDialog req={{ ...req }} />));
  expect(buttons()).toEqual(expect.arrayContaining(["Weiter bearbeiten", "Verwerfen"]));
});
