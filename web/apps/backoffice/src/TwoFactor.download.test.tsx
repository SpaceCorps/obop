// @vitest-environment jsdom
// The file that the recovery codes are downloaded as (docs/spec/branding.md, section 6): named by the
// slug of the brand the page carries, and `obop-recovery-codes.txt` on a page that carries none (the
// control-plane app). The name function is held by the tests of `@obop/ui`; this is the button that
// uses it, in the panel the back office mounts.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { TwoFactorPanel, type TwoFactorApi } from "@obop/ui";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let downloads: string[];

beforeEach(() => {
  downloads = [];
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (
    this: HTMLAnchorElement,
  ) {
    downloads.push(this.download);
  });
  vi.stubGlobal("URL", { createObjectURL: () => "blob:codes", revokeObjectURL: () => {} });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(() => {
  act(() => root.unmount());
  container.remove();
  document.head.querySelectorAll('meta[name="obop-brand"]').forEach((m) => m.remove());
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function setBrand(content: object | null) {
  document.head.querySelectorAll('meta[name="obop-brand"]').forEach((m) => m.remove());
  if (!content) return;
  const meta = document.createElement("meta");
  meta.name = "obop-brand";
  meta.content = JSON.stringify(content);
  document.head.append(meta);
}

const api: TwoFactorApi = {
  status: async () => ({
    ok: true,
    value: {
      available: true,
      enabled: false,
      pending: false,
      recovery_remaining: 0,
      limited: false,
    },
  }),
  begin: async () => ({
    ok: true,
    value: { secret: "ABCD", uri: "otpauth://totp/x?secret=ABCD", issuer: "X", account: "ann" },
  }),
  confirm: async () => ({ ok: true, value: { recovery_codes: ["AAAA-BBBB-CCCC-DDDD"] } }),
  disable: async () => ({ ok: true, value: {} }),
  regenerate: async () => ({ ok: true, value: { recovery_codes: [] } }),
};

const button = (name: string) =>
  [...container.querySelectorAll<HTMLButtonElement>("button")].find((b) =>
    b.textContent?.includes(name),
  )!;

/** Sets two-factor up as far as the screen with the codes, and presses Download. */
async function download() {
  await act(async () => root.render(<TwoFactorPanel api={api} />));
  await act(async () => {});
  await act(async () => button("Set up two-factor").click());
  const input = container.querySelector<HTMLInputElement>('input[name="code"]')!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(
      input,
      "123456",
    );
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await act(async () => input.form!.requestSubmit());
  await act(async () => button("Download").click());
}

describe("the download of the recovery codes", () => {
  it("is named by the slug of the brand of the page", async () => {
    setBrand({ name: "MOHV", slug: "mohv", messages: {}, v: "" });
    await download();
    expect(downloads).toEqual(["mohv-recovery-codes.txt"]);
  });

  it("is named after the product on a page that has no brand", async () => {
    await download();
    expect(downloads).toEqual(["obop-recovery-codes.txt"]);
  });
});
