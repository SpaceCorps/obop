// Launched by Playwright's `webServer`: `node support/serve.ts app|platform`.
// Playwright starts web servers before globalSetup, so the accounts have to be created here, by the
// same process that then starts the server, to be sure they exist before the first request.
// Builds nothing; the binary and the static directories must exist already.
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";
import {
  backofficeAdminDist,
  backofficeDist,
  nextMinor,
  nextPatch,
  obopBinary,
  secretKey,
  shopOwner,
  siteDir,
  updateToken,
  users,
} from "./env.ts";

const [kind] = process.argv.slice(2);
const dir = process.env.OBOP_E2E_DIR;
if (!dir) throw new Error("OBOP_E2E_DIR is not set; start the suite with `playwright test`");
mkdirSync(dir, { recursive: true });
const bin = obopBinary();

// The marketplace the update specs fill (D63): `index.json` and `packages/<id>.toml` in a directory
// the specs write to, served as the app's OBOP_MARKETPLACE_URL.
if (kind === "market") {
  const root = join(dir, "market");
  mkdirSync(join(root, "packages"), { recursive: true });
  writeFileSync(join(root, "index.json"), "[]");
  const server = createServer((req, res) => {
    const path = (req.url ?? "/").split("?")[0] ?? "/";
    const file =
      path === "/index.json"
        ? join(root, "index.json")
        : /^\/packages\/[a-z0-9_]+\.toml$/.test(path)
          ? join(root, path)
          : "";
    if (!file || !existsSync(file)) {
      res.statusCode = 404;
      res.end("not found");
      return;
    }
    res.setHeader("content-type", path.endsWith(".json") ? "application/json" : "text/plain");
    res.end(readFileSync(file));
  });
  server.listen(Number(process.env.OBOP_E2E_MARKET_PORT), "127.0.0.1");
  for (const sig of ["SIGTERM", "SIGINT"] as const)
    process.on(sig, () => server.close(() => process.exit(0)));
  await new Promise(() => {});
}

function run(args: string[], input?: string) {
  const r = spawnSync(bin, args, { input, encoding: "utf8" });
  if (r.status !== 0) throw new Error(`obop ${args.join(" ")} failed:\n${r.stdout}\n${r.stderr}`);
}

let args: string[];
const extraEnv: Record<string, string> = {};
if (kind === "app") {
  const db = `sqlite://${join(dir, "app.sqlite")}`;
  for (const u of Object.values(users))
    run(["admin", "create", u.name, "--groups", u.groups, "--db", db], `${u.password}\n`);
  args = [
    "serve",
    "--addr",
    `127.0.0.1:${process.env.OBOP_E2E_APP_PORT}`,
    "--db",
    db,
    "--static",
    backofficeDist,
  ];
  // Updating the instance (D63): it holds the update token of an instance the control plane
  // knows, a directory for the backup before an update, and a marketplace the specs fill.
  extraEnv.OBOP_UPDATE_URL = `http://127.0.0.1:${process.env.OBOP_E2E_PLATFORM_PORT}`;
  extraEnv.OBOP_UPDATE_TOKEN = updateToken;
  extraEnv.OBOP_BACKUP_DIR = join(dir, "backups");
  extraEnv.OBOP_MARKETPLACE_URL = `http://127.0.0.1:${process.env.OBOP_E2E_MARKET_PORT}`;
} else if (kind === "shop") {
  // A web shop: public site at /, back office at /admin/, the two shop packages, demo content
  // with generated pictures, and the demo payment provider (no money moves, no card is asked).
  const db = `sqlite://${join(dir, "shop.sqlite")}`;
  run(["modules", "install-from", "website", "--db", db]);
  run(["modules", "install-from", "ecommerce", "--db", db]);
  run(["site", "seed", "--db", db]);
  run(["site", "set", "payment_methods", "fake,manual", "--db", db]);
  run([
    "site",
    "set",
    "payment_instructions",
    "Pay by bank transfer to the shop account.",
    "--db",
    db,
  ]);
  run(
    ["admin", "create", shopOwner.name, "--groups", shopOwner.groups, "--db", db],
    `${shopOwner.password}\n`,
  );
  args = [
    "serve",
    "--public-site",
    "--admin-prefix",
    "/admin",
    "--addr",
    `127.0.0.1:${process.env.OBOP_E2E_SHOP_PORT}`,
    "--db",
    db,
    "--static",
    backofficeAdminDist,
  ];
} else if (kind === "platform") {
  const db = `sqlite://${join(dir, "platform.sqlite")}`;
  // Two published releases, a patch that fits every installed module and a minor that does not
  // (the modules declare the current series), and the app instance registered with its token
  // (D63). The fake provisioner takes a moment to "redeploy" so the progress can be seen.
  for (const [version, notes] of [
    [nextPatch, "Bug fixes and small improvements."],
    [
      nextMinor,
      "A new minor version. Modules written for the earlier series must be updated first.",
    ],
  ] as const)
    run([
      "platform",
      "release",
      "publish",
      "--version",
      version,
      "--image",
      `ghcr.io/spacecorps/obop:${version}`,
      "--notes",
      notes,
      "--db",
      db,
    ]);
  run(
    [
      "platform",
      "register-instance",
      "e2e-app",
      "--url",
      `http://127.0.0.1:${process.env.OBOP_E2E_APP_PORT}`,
      "--external-id",
      "fake-e2e-app",
      "--image",
      "ghcr.io/spacecorps/obop:current",
      "--token-stdin",
      "--db",
      db,
    ],
    `${updateToken}\n`,
  );
  extraEnv.OBOP_FAKE_UPGRADE_DELAY_MS = "2500";
  extraEnv.OBOP_CONTROL_URL = `http://127.0.0.1:${process.env.OBOP_E2E_PLATFORM_PORT}`;
  args = [
    "platform",
    "serve",
    "--provisioner",
    "fake",
    "--addr",
    `127.0.0.1:${process.env.OBOP_E2E_PLATFORM_PORT}`,
    "--db",
    db,
    "--static",
    siteDir,
  ];
} else throw new Error("usage: serve.ts app|shop|platform");

// Connectors: the specs run a receiver on loopback, which is refused unless the operator allows it.
// The secret key is a 32-byte key in the form two-factor authentication and connectors both accept.
const child = spawn(bin, args, {
  stdio: "inherit",
  env: {
    ...process.env,
    ...extraEnv,
    OBOP_SECRET_KEY: secretKey,
    OBOP_REQUIRE_MFA_FOR_ADMINS: "0",
    OBOP_CONNECTORS_ALLOW_LOOPBACK: "1",
    OBOP_DEMO_PAYMENTS: "1",
    OBOP_PUBLIC_URL:
      kind === "shop"
        ? `http://127.0.0.1:${process.env.OBOP_E2E_SHOP_PORT}`
        : `http://127.0.0.1:${process.env.OBOP_E2E_APP_PORT}`,
  },
});
for (const sig of ["SIGTERM", "SIGINT"] as const) process.on(sig, () => child.kill(sig));
child.on("exit", (code) => process.exit(code ?? 0));
