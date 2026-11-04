import { describe, expect, test } from "vite-plus/test";
import { directImports } from "../test-support/imports.ts";

// The app is to be split one day into a host (session, sign-in, the key and the database, the
// owner of the outbox, the list of modules) and modules (Mileage first) that a host carries. Until
// then the parts are told apart by this test and by nothing else, so a host file can pick up an
// import of a Mileage file without anyone noticing. The test is a ratchet: it lists today's
// violations; a new one fails it, and a listed one that has been removed fails it too, so that the
// list only gets shorter. Types count: the boundary is about what a file knows, not only about
// what runs.
//
// This list is edited by the change that adds or removes the import. A branch that adds host-side
// imports of Mileage files (for example a language store imported by `native/auth.ts`) makes this
// test fail when it is merged, until each new import is removed or listed here with a reason.

/** Files that a host owns. `native/runtime.ts` is mixed until the runtime is split. */
const HOST_SIDE = [
  "src/core/address.ts",
  "src/core/deadline.ts",
  "src/core/device.ts",
  "src/core/host-config.ts",
  "src/core/open-outbox.ts",
  "src/core/session.ts",
  "src/core/sql.ts",
  "src/core/startup-config.ts",
  "src/native/auth.ts",
  "src/native/bus.ts",
  "src/native/database.ts",
  "src/native/host.ts",
  "src/native/keychain.ts",
  "src/native/platform-deps.ts",
  "src/native/secure-storage.ts",
];

/** Files that are Mileage's or mixed (`core/identity.ts` reads the outbox of Mileage). */
const MILEAGE_SIDE = [
  "src/core/api.ts",
  "src/core/engine.ts",
  "src/core/fix.ts",
  "src/core/identity.ts",
  "src/core/liveness.ts",
  "src/core/present.ts",
  "src/core/sql-store.ts",
  "src/core/store.ts",
  "src/core/sync.ts",
  "src/core/thin.ts",
  "src/core/types.ts",
  "src/core/values.ts",
  "src/native/actions.ts",
  "src/native/config.ts",
  "src/native/devices.ts",
  "src/native/recorder.ts",
  "src/native/runtime.ts",
  "src/native/store.ts",
  "src/native/task.ts",
];
const MILEAGE_PREFIXES = ["src/ui/", "src/i18n/"];
const MILEAGE_PACKAGES = ["@obop/trip-core", "@obop/ui-native"];

/** `from -> to`: known, with the reason. */
const KNOWN: Record<string, string> = {
  "src/core/open-outbox.ts -> src/core/sql-store.ts":
    "migrates and writes the stale-file note through the Mileage store",
  "src/core/open-outbox.ts -> src/core/store.ts": "the note helpers are typed on the Mileage store",
  "src/core/open-outbox.ts -> src/core/values.ts": "formats the time of the note",
  "src/native/auth.ts -> src/core/identity.ts": "Binding and Owner (types) of the outbox owner",
};

function violations(): string[] {
  const found: string[] = [];
  for (const from of HOST_SIDE) {
    const { files, packages } = directImports(from, true);
    for (const to of files) {
      if (MILEAGE_SIDE.includes(to) || MILEAGE_PREFIXES.some((p) => to.startsWith(p))) {
        found.push(`${from} -> ${to}`);
      }
    }
    for (const to of packages) {
      if (MILEAGE_PACKAGES.includes(to)) found.push(`${from} -> ${to}`);
    }
  }
  return found.sort();
}

describe("the host side of the app does not import the Mileage side", () => {
  const found = violations();

  test("no import that is not known", () => {
    expect(found.filter((v) => !(v in KNOWN))).toEqual([]);
  });

  test("every import that is known is still there (remove it from the list when it goes)", () => {
    expect(Object.keys(KNOWN).filter((v) => !found.includes(v))).toEqual([]);
  });

  test("the files of the lists exist", () => {
    for (const file of [...HOST_SIDE, ...MILEAGE_SIDE]) {
      expect(() => directImports(file, true), file).not.toThrow();
    }
  });

  test("no file is on both sides", () => {
    expect(HOST_SIDE.filter((f) => MILEAGE_SIDE.includes(f))).toEqual([]);
  });
});
