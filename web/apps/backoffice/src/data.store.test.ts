// The cached rows of a model (data.tsx RowStore): a refresh asked for after a write reads the
// records after that write, even while an older load is still running (FLAKY2: on a loaded runner
// a sale order's total stayed 0.00 after its line was saved, because the refresh shared the answer
// of a load that started before the line existed).
import { describe, expect, it, vi } from "vite-plus/test";

type Answer = { rows: { id: string; total: number }[]; denied: string[]; total: number };
const answers: ((a: Answer) => void)[] = [];
vi.mock("./api.ts", async (orig) => ({
  ...(await orig<typeof import("./api.ts")>()),
  list: vi.fn(() => new Promise<Answer>((res) => answers.push(res))),
}));

const { RowStore } = await import("./data.tsx");
const { list } = await import("./api.ts");

const answer = (i: number, total: number) =>
  answers[i]!({ rows: [{ id: "o1", total }], denied: [], total: 1 });

describe("a refresh while a load is running", () => {
  it("loads again after it, so the answer is read after the write", async () => {
    const store = new RowStore("t", () => ({}) as never);
    void store.load("sale_order");
    answer(0, 0);
    await Promise.resolve();
    await Promise.resolve();
    expect(store.cached("sale_order")?.[0]?.total).toBe(0);

    // A load starts (the order was created), then the line is saved and asks for a refresh.
    void store.refresh("sale_order");
    const after = store.refresh("sale_order");
    expect(list).toHaveBeenCalledTimes(2);
    // The running load was answered before the line existed.
    answer(1, 0);
    await vi.waitFor(() => expect(list).toHaveBeenCalledTimes(3));
    answer(2, 249);
    await after;
    expect(store.cached("sale_order")?.[0]?.total).toBe(249);
  });
});
