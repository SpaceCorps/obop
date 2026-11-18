// @vitest-environment jsdom
// R16: focus goes back to what opened a dialog, also when the opener is disabled while the dialog's
// work runs or is drawn again as a new element.
import { afterEach, expect, it } from "vite-plus/test";
import { giveFocusBack } from "@obop/ui";

const wait = (ms: number) => new Promise((done) => setTimeout(done, ms));
afterEach(() => {
  document.body.innerHTML = "";
});

it("waits for a disabled opener and focuses it once it is usable again", async () => {
  const opener = document.createElement("button");
  opener.textContent = "Reject";
  opener.disabled = true;
  document.body.append(opener);
  giveFocusBack(opener);
  expect(document.activeElement).toBe(document.body);
  opener.disabled = false;
  await wait(350);
  expect(document.activeElement).toBe(opener);
});

it("focuses the element drawn in place of an opener that is gone", async () => {
  const old = document.createElement("button");
  old.textContent = "Reject";
  document.body.append(old);
  old.remove();
  const now = document.createElement("button");
  now.textContent = "Reject";
  document.body.append(now);
  giveFocusBack(old);
  expect(document.activeElement).toBe(now);
});
