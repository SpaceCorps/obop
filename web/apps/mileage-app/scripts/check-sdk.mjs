// `expo install --check` for the pins of this app.
//
//   node scripts/check-sdk.mjs            compares every Expo package with the bill of materials of
//                                         the installed `expo` (its bundledNativeModules.json),
//                                         offline: the check a pin means
//   node scripts/check-sdk.mjs --latest   asks Expo's versions API what is newest instead
//
// The online form wants the newest patch release of every package. Under the 7 day cooldown of D23
// (`minimumReleaseAge`, docs/DECISIONS.md 1.1) the newest patch is often younger than 7 days and
// cannot be installed yet, so the online form fails for that reason alone; it is for reading what
// is waiting, not for a gate (docs/spec/mobile.md 8.7).
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";

const latest = process.argv.includes("--latest");
const cli = createRequire(import.meta.url).resolve("expo/bin/cli");
const result = spawnSync(process.execPath, [cli, "install", "--check"], {
  stdio: "inherit",
  env: { ...process.env, EXPO_NO_TELEMETRY: "1", ...(latest ? {} : { EXPO_OFFLINE: "1" }) },
});
process.exit(result.status ?? 1);
