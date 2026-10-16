// Paths, credentials and URLs shared by the Playwright config, the server launcher and the tests.
// The config process picks the temp directory and the ports once and exports them as environment
// variables, so the workers it spawns (which load the config again) see the same values.
import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../../..");

/** The binary under test: `$OBOP_BIN`, else the debug build at the repository root. */
export function obopBinary(): string {
  const bin = process.env.OBOP_BIN ?? resolve(repoRoot, "target/debug/obop");
  if (!existsSync(bin))
    throw new Error(`obop binary not found at ${bin}; run \`cargo build\` or set OBOP_BIN`);
  return bin;
}

export const backofficeDist = resolve(repoRoot, "web/apps/backoffice/dist");
export const siteDir = resolve(repoRoot, "web/site");

export function requireBuilt(): void {
  for (const [dir, how] of [
    [backofficeDist, "(cd web/apps/backoffice && vp build)"],
    [siteDir, "deploy/build-site.sh"],
  ] as const)
    if (!existsSync(resolve(dir, "index.html")))
      throw new Error(`${dir} is missing; run ${how} first`);
}

/** Test accounts, created with the CLI before the instance starts. Test values only. */
export const users = {
  root: { name: "root", password: "e2e root password 1", groups: "platform.admin" },
  sam: {
    name: "sam",
    password: "e2e sam password 1",
    groups: "base.user,crm.user,sales.user,product.user",
  },
  // Reads dashboards but not sales or invoicing, so a tile over those models shows "No access".
  dana: {
    name: "dana",
    password: "e2e dana password 1",
    groups: "base.user,crm.user,dashboards.user",
  },
  // Holds the groups of the fixed-id module "wfagent" (see the agent tests), so it can issue an agent
  // token that may read and write that module.
  ivy: {
    name: "ivy",
    password: "e2e ivy password 1",
    groups: "wfagent.user,wfagent.manager",
  },
} as const;
export type UserName = keyof typeof users;

export const appUrl = () => `http://127.0.0.1:${process.env.OBOP_E2E_APP_PORT}`;
export const platformUrl = () => `http://127.0.0.1:${process.env.OBOP_E2E_PLATFORM_PORT}`;
