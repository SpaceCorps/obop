import { beforeEach, describe, expect, test } from "vite-plus/test";
import { memoryDb } from "../test-support/memory-db.ts";
import { migrate } from "./schema.ts";
import {
  DISCLOSURE_ACCEPTED,
  THEME,
  createSettings,
  themeChoiceOf,
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

  test("a value is in the file: another connection to it reads it (the next start)", async () => {
    await settings.set(DISCLOSURE_ACCEPTED, "x");
    expect(await createSettings(db).get(DISCLOSURE_ACCEPTED)).toBe("x");
  });
});

describe("the look of the app", () => {
  test("follows the system until light or dark is chosen, and stays chosen", async () => {
    expect(themeChoiceOf(await settings.get(THEME))).toBe("system");
    await settings.set(THEME, "dark");
    expect(themeChoiceOf(await createSettings(db).get(THEME))).toBe("dark");
    await settings.set(THEME, "light");
    expect(themeChoiceOf(await settings.get(THEME))).toBe("light");
  });

  test("a value it does not know is the system's", () => {
    expect(themeChoiceOf("sepia")).toBe("system");
    expect(themeChoiceOf(null)).toBe("system");
  });
});
