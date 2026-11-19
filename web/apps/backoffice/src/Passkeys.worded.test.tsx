// @vitest-environment jsdom
// A refusal of the re-authentication form follows a switch of language (D9): the form keeps how to
// word it, not the finished sentence of the language it was made in.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import { ReauthForm, passkeyMessage } from "./Passkeys.tsx";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const WRONG = "That password is not right.";
let container: HTMLDivElement;
let root: Root;

beforeEach(async () => {
  provideCatalogue("en", {});
  provideCatalogue("de", { [WRONG]: "Das Passwort stimmt nicht." });
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

describe("the re-authentication form", () => {
  it("rewords its refusal after a switch of language", async () => {
    await act(async () => {
      root.render(
        <ReauthForm
          token="t"
          hasTotp={false}
          hasPasskeys={false}
          heading="Remove passkey"
          submitLabel="Remove"
          onSubmit={async () => () =>
            passkeyMessage({ kind: "server", status: 403, code: "invalid_password", message: "" })
          }
          onCancel={() => {}}
        />,
      );
    });
    const form = container.querySelector("form")!;
    container.querySelector<HTMLInputElement>('input[name="password"]')!.value = "not the one";
    await act(async () => {
      form.requestSubmit();
    });
    expect(container.textContent).toContain(WRONG);
    await act(async () => {
      await setLanguage("de", false);
    });
    expect(container.textContent).toContain("Das Passwort stimmt nicht.");
    expect(container.textContent).not.toContain(WRONG);
  });
});
