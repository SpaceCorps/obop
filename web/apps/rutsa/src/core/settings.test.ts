import { beforeEach, describe, expect, test } from "vite-plus/test";
import { memoryDb } from "../test-support/memory-db.ts";
import { migrate } from "./schema.ts";
import {
  DISCLOSURE_ACCEPTED,
  STOP_DETAILS,
  createSettings,
  stopDetailsOn,
  type Settings,
} from "./settings.ts";
import type { SqlDb } from "./sql.ts";

let db: SqlDb;
let settings: Settings;

beforeEach(async () => {
  db = memoryDb();
  await migrate(db);
  settings = createSettings(db);
});

describe("the settings of the app", () => {
  test("a setting that was never set is null", async () => {
    expect(await settings.get(DISCLOSURE_ACCEPTED)).toBeNull();
  });

  test("a value is kept, and a second set replaces it", async () => {
    await settings.set(DISCLOSURE_ACCEPTED, "2026-11-22T10:00:00.000Z");
    expect(await settings.get(DISCLOSURE_ACCEPTED)).toBe("2026-11-22T10:00:00.000Z");
    await settings.set(DISCLOSURE_ACCEPTED, "2026-11-23T08:00:00.000Z");
    expect(await settings.get(DISCLOSURE_ACCEPTED)).toBe("2026-11-23T08:00:00.000Z");
    expect(await db.all("SELECT key FROM meta")).toEqual([{ key: DISCLOSURE_ACCEPTED }]);
  });

  test("the switch of the details at Stop is on until it is turned off, and stays so", async () => {
    expect(stopDetailsOn(await settings.get(STOP_DETAILS))).toBe(true);
    await settings.set(STOP_DETAILS, "off");
    expect(stopDetailsOn(await createSettings(db).get(STOP_DETAILS))).toBe(false);
    await settings.set(STOP_DETAILS, "on");
    expect(stopDetailsOn(await settings.get(STOP_DETAILS))).toBe(true);
  });

  test("a value is in the file: another connection to it reads it (the next start)", async () => {
    await settings.set(DISCLOSURE_ACCEPTED, "x");
    expect(await createSettings(db).get(DISCLOSURE_ACCEPTED)).toBe("x");
  });
});
