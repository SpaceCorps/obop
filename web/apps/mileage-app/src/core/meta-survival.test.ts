import { readFileSync } from "node:fs";
import { describe, expect, test } from "vite-plus/test";
import { DISCLOSURE_META } from "./host-state.ts";
import { bindOwner, parseOwner, takeOver } from "./identity.ts";
import { createSqlStore, migrate } from "./sql-store.ts";
import { memoryDb } from "../test-support/memory-db.ts";

// Four rows of the `meta` table matter to an installed app: the owner of the outbox, the flag that
// the person has read the location disclosure, the note that a file of a restored backup was
// replaced, and the pointer to the trip that is recording. A sign-out clears the outbox (`clearAll`)
// and must clear the last one only: if it took the others too, every sign-out would ask the person
// to accept the disclosure again, forget whose trips are on the phone, and lose the note about
// the replaced file before it was read. The names are literals on purpose: they are what a file
// written by an earlier build holds.

const T0 = Date.parse("2026-10-15T09:00:00Z");
const OWNER = { origin: "https://phone.example", user: "dave" };

async function rig() {
  const db = memoryDb();
  await migrate(db);
  const store = createSqlStore(db);
  await store.startTrip({
    clientTripId: "c1",
    purpose: "business",
    startedAt: new Date(T0).toISOString(),
    nowMs: T0,
  });
  await store.setMeta("owner", JSON.stringify(OWNER));
  await store.setMeta("location_disclosure_accepted", "2026-10-01T08:00:00.000Z");
  await store.setMeta("stale_file_recovered_at", "2026-10-30T10:00:00.000Z");
  return { db, store };
}

describe("what a sign-out leaves in the meta table", () => {
  test("clearAll removes the pointer to the active trip and nothing else", async () => {
    const { store } = await rig();
    expect(await store.getMeta("active_trip")).toBe("c1");

    await store.clearAll();

    expect(await store.getMeta("active_trip")).toBeNull();
    expect(await store.activeTrip()).toBeNull();
    expect(parseOwner(await store.getMeta("owner"))).toEqual(OWNER);
    expect(await store.getMeta("location_disclosure_accepted")).toBe("2026-10-01T08:00:00.000Z");
    expect(await store.getMeta("stale_file_recovered_at")).toBe("2026-10-30T10:00:00.000Z");
  });

  test("the rows that are left are the whole table", async () => {
    const { db, store } = await rig();
    await store.clearAll();
    const keys = (await db.all<{ key: string }>("SELECT key FROM meta ORDER BY key")).map(
      (r) => r.key,
    );
    expect(keys).toEqual(["location_disclosure_accepted", "owner", "stale_file_recovered_at"]);
  });

  test("taking over the outbox of another person replaces the owner and keeps the other rows", async () => {
    const { store } = await rig();
    const next = { origin: "https://phone.example", user: "rita" };
    await takeOver(store, next);
    expect(parseOwner(await store.getMeta("owner"))).toEqual(next);
    expect(await store.getMeta("location_disclosure_accepted")).not.toBeNull();
    expect(await store.getMeta("stale_file_recovered_at")).not.toBeNull();
    expect(await store.getMeta("active_trip")).toBeNull();
  });

  test("an outbox of another person that holds nothing unsent is emptied and the other rows stay", async () => {
    const { store } = await rig();
    await store.deleteTrip("c1"); // nothing unsent is left
    const next = { origin: "https://phone.example", user: "rita" };
    await expect(bindOwner(store, next)).resolves.toEqual({ kind: "own" });
    expect(parseOwner(await store.getMeta("owner"))).toEqual(next);
    expect(await store.getMeta("location_disclosure_accepted")).not.toBeNull();
    expect(await store.getMeta("stale_file_recovered_at")).not.toBeNull();
  });
});

describe("the name of the flag that the screen of the disclosure keeps", () => {
  // The screen is a component and is not run here; its use of the name is read from its source. The
  // name itself is the host's (`host-state.ts`) since the schema manager: the screen no longer
  // defines it, it imports it, and reads and writes it through the host's way in.
  test("is the one an installed app has", () => {
    expect(DISCLOSURE_META).toBe("location_disclosure_accepted");
    const host = readFileSync(new URL("./host-state.ts", import.meta.url), "utf8");
    expect(host).toMatch(/DISCLOSURE_META = "location_disclosure_accepted";/);
  });

  test("is the one the screen uses, through the host's state", () => {
    const source = readFileSync(new URL("../ui/screens/Permissions.tsx", import.meta.url), "utf8");
    expect(source).toMatch(/import \{ DISCLOSURE_META \} from "..\/..\/core\/host-state.ts";/);
    expect(source).not.toMatch(/DISCLOSURE_META =/);
    expect(source).toMatch(/rt\.hostState\.getMeta\(DISCLOSURE_META\)/);
    expect(source).toMatch(/rt\.hostState\.setMeta\(DISCLOSURE_META, /);
  });
});

describe("the note about a replaced file that the hook of the screens keeps", () => {
  // The hook is a React hook and is not run here; its use of the host's state is read from its source.
  test("is read and dismissed through the host's state, not through the outbox's store", () => {
    const source = readFileSync(new URL("../ui/hooks.ts", import.meta.url), "utf8");
    expect(source).toMatch(/staleFileRecoveredAt\(rt\.hostState\)/);
    expect(source).toMatch(/dismissStaleFileNote\(rt\.hostState\)/);
    expect(source).not.toMatch(/staleFileRecoveredAt\(rt\.store\)/);
    expect(source).not.toMatch(/dismissStaleFileNote\(rt\.store\)/);
  });
});
