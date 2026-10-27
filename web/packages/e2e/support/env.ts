// Paths, credentials and URLs shared by the Playwright config, the server launcher and the tests.
// The config process picks the temp directory and the ports once and exports them as environment
// variables, so the workers it spawns (which load the config again) see the same values.
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../../..");

/** The binary under test: `$OBOP_BIN`, else the debug build at the repository root. */
export function obopBinary(): string {
  const bin = process.env.OBOP_BIN ?? resolve(repoRoot, "target/debug/obop");
  if (!existsSync(bin))
    throw new Error(`obop binary not found at ${bin}; run \`cargo build\` or set OBOP_BIN`);
  return bin;
}

/** The core version under test: the workspace version in Cargo.toml (D55). */
const [coreMajor, coreMinor, corePatch] = (() => {
  const m = /^version = "(\d+)\.(\d+)\.(\d+)"/m.exec(
    readFileSync(resolve(repoRoot, "Cargo.toml"), "utf8"),
  );
  if (!m) throw new Error("no workspace version in Cargo.toml");
  return [Number(m[1]), Number(m[2]), Number(m[3])];
})();
export const coreVersion = `${coreMajor}.${coreMinor}.${corePatch}`;
/** The core range a module written for this series declares: `^0.2`. */
export const coreSeries = `^${coreMajor}.${coreMinor}`;
/** The releases the control plane publishes to the instance under test (D63). */
export const nextPatch = `${coreMajor}.${coreMinor}.${corePatch + 1}`;
export const nextMinor = `${coreMajor}.${coreMinor + 1}.0`;
/** The update token the app instance holds for its control plane. A throwaway test value. */
export const updateToken = `obu_${"0123456789abcdef".repeat(4)}`;

export const backofficeDist = resolve(repoRoot, "web/apps/backoffice/dist");
/** The same app built for /admin/ (`OBOP_ADMIN_BASE=/admin/ vp build --outDir dist-admin`): the shop instance serves it. */
export const backofficeAdminDist = resolve(repoRoot, "web/apps/backoffice/dist-admin");
export const siteDir = resolve(repoRoot, "web/site");

export function requireBuilt(): void {
  for (const [dir, how] of [
    [backofficeDist, "(cd web/apps/backoffice && vp build)"],
    [
      backofficeAdminDist,
      "(cd web/apps/backoffice && OBOP_ADMIN_BASE=/admin/ vp build --outDir dist-admin)",
    ],
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
    groups: "base.user,crm.user,product.user,dashboards.user",
  },
  // Holds the groups of the fixed-id module "wfagent" (see the agent tests), so it can issue an agent
  // token that may read and write that module.
  ivy: {
    name: "ivy",
    password: "e2e ivy password 1",
    groups: "wfagent.user,wfagent.manager",
  },
  // Two-factor specs (tests/mfa.spec.ts). Each spec owns its user: a lockout or an enrolment must
  // never leak into another spec, and users cannot be created while the instance runs.
  tess: { name: "tess", password: "e2e tess password 1", groups: "base.user" },
  lock: { name: "lock", password: "e2e lock password 1", groups: "base.user" },
  quinn: { name: "quinn", password: "e2e quinn password 1", groups: "base.user" },
  nova: { name: "nova", password: "e2e nova password 1", groups: "platform.admin" },
  // An employee: reads the people records but sees only her own leave requests (model.spec.ts).
  eve: { name: "eve", password: "e2e eve password 1", groups: "base.user,hr.user" },
  // Connects her own accounts (connectors.user) and nothing else: the connectors specs use her to
  // check that a person sees only her own connections and may withdraw her consent.
  // Account specs (tests/security.spec.ts): one changes her own password and ends sessions, the
  // other is disabled, enabled and given a temporary password by the administrator.
  // Passkey specs (tests/passkeys.spec.ts): each owns her passkeys, so one spec never sees another's.
  pia: { name: "pia", password: "e2e pia password 1!", groups: "base.user" },
  pax: { name: "pax", password: "e2e pax password 1!", groups: "base.user" },
  pom: { name: "pom", password: "e2e pom password 1!", groups: "base.user" },
  jo: { name: "jo", password: "e2e jo password 1!", groups: "base.user" },
  pat: { name: "pat", password: "e2e pat password 1!", groups: "base.user" },
  cora: {
    name: "cora",
    password: "e2e cora password 1",
    groups: "base.user,crm.user,connectors.user",
  },
} as const;
export type UserName = keyof typeof users;

/**
 * The web shop instance (tests/shop.spec.ts): a second server with the public site on, the website
 * and ecommerce packages installed, demo content seeded and the demo payment provider switched on.
 * Its own database and its own owner account.
 */
export const shopOwner = { name: "owner", password: "e2e shop owner 1", groups: "platform.admin" };

export const appUrl = () => `http://127.0.0.1:${process.env.OBOP_E2E_APP_PORT}`;
export const shopUrl = () => `http://127.0.0.1:${process.env.OBOP_E2E_SHOP_PORT}`;
/** The instance with `OBOP_DEFAULT_LANGUAGE=sv` (tests/default-language.spec.ts). */
export const languageUrl = () => `http://127.0.0.1:${process.env.OBOP_E2E_LANGUAGE_PORT}`;
export const platformUrl = () => `http://127.0.0.1:${process.env.OBOP_E2E_PLATFORM_PORT}`;
/** A small marketplace the specs fill (D63): packages the app instance can update to. */
export const marketUrl = () => `http://127.0.0.1:${process.env.OBOP_E2E_MARKET_PORT}`;
export const marketDir = () => join(process.env.OBOP_E2E_DIR ?? "", "market");

/**
 * The server secret the instance seals two-factor secrets with (D53). A throwaway test value; the
 * suite also sets OBOP_REQUIRE_MFA_FOR_ADMINS=0 so the admin `root` signs in without a second
 * factor, as the other 125 tests expect. The two-factor spec turns the policy on where it needs it.
 */
export const secretKey = "0123456789abcdef".repeat(4);
