// @vitest-environment jsdom
// useRecord (useServer.ts) mounted for real with a fake server (docs/ISSUES.md D15b): a `get` that
// finds nothing must reach `remember()` as "nothing" (null), so the row of a deleted record is
// forgotten and never shown again while a later `get` of the same record is on its way. The
// sequence: a record opened by its address is seen; it is deleted elsewhere and, after a write, its
// `get` finds nothing; another missing record is opened; the first is opened again with its `get`
// held. Nothing old may show. A glue that passed `row ?? undefined` to remember() showed the old row.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import type { Row } from "./api.ts";
import { DataProvider, useData } from "./data.tsx";
import { useRecord } from "./useServer.ts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const MODEL = "thing";
const A: Row = { id: "a", title: "Old A" };

/** What the fake server holds, and the `get`s it holds back until released. */
let records: Map<string, Row>;
let held: Map<string, () => void>;
let holdNext: string | null;

const json = (value: unknown, status = 200) =>
  new Response(JSON.stringify(value), { status, headers: { "content-type": "application/json" } });

function fakeServer() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init?: RequestInit) => {
      const path = String(url).split("?")[0]!;
      const body = init?.body ? JSON.parse(String(init.body)) : {};
      if (path === `/api/${MODEL}/get`) {
        if (holdNext === body.id) {
          holdNext = null;
          await new Promise<void>((resolve) => held.set(body.id, resolve));
        }
        const row = records.get(body.id);
        return row ? json({ value: row }) : json({ error: `no ${MODEL} ${body.id}` }, 400);
      }
      // Lists and searches the store may ask for: nothing on screen.
      return json({ value: { rows: [], total: 0 } });
    }),
  );
}

let shown: Row | null | undefined;
let data: ReturnType<typeof useData> | null;

function Probe({ id }: { id: string }) {
  data = useData();
  shown = useRecord(MODEL, id, []);
  return null;
}

let container: HTMLDivElement;
let root: Root;

async function open(id: string) {
  await act(async () => {
    root.render(
      <DataProvider
        token="t"
        modules={[]}
        apps={[]}
        notify={() => {}}
        confirm={async () => true}
        linkFor={() => null}
      >
        <Probe id={id} />
      </DataProvider>,
    );
  });
}

const settle = () => act(async () => new Promise((r) => setTimeout(r, 0)));

beforeEach(() => {
  records = new Map([[A.id, A]]);
  held = new Map();
  holdNext = null;
  shown = undefined;
  data = null;
  fakeServer();
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.unstubAllGlobals();
});

describe("useRecord and remember (D15b)", () => {
  it("never brings back the row of a record whose get found nothing", async () => {
    // Opened by its address and seen.
    await open("a");
    await settle();
    expect(shown).toEqual(A);

    // Deleted elsewhere; after a write its get is asked again and finds nothing.
    records.delete("a");
    await act(async () => {
      await data!.changed(MODEL);
    });
    await settle();
    expect(shown).toBeNull();

    // Another record that is not there.
    await open("b");
    await settle();
    expect(shown).toBeNull();

    // The first one again, its get held: nothing is known of it, so nothing old is shown.
    holdNext = "a";
    await open("a");
    await settle();
    expect(held.has("a")).toBe(true);
    expect(shown).toBeUndefined();

    held.get("a")!();
    await settle();
    expect(shown).toBeNull();
  });
});
