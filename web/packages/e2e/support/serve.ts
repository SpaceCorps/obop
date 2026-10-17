// Launched by Playwright's `webServer`: `node support/serve.ts app|platform`.
// Playwright starts web servers before globalSetup, so the accounts have to be created here, by the
// same process that then starts the server, to be sure they exist before the first request.
// Builds nothing; the binary and the static directories must exist already.
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { backofficeDist, obopBinary, secretKey, siteDir, users } from "./env.ts";

const [kind] = process.argv.slice(2);
const dir = process.env.OBOP_E2E_DIR;
if (!dir) throw new Error("OBOP_E2E_DIR is not set; start the suite with `playwright test`");
mkdirSync(dir, { recursive: true });
const bin = obopBinary();

function run(args: string[], input?: string) {
  const r = spawnSync(bin, args, { input, encoding: "utf8" });
  if (r.status !== 0) throw new Error(`obop ${args.join(" ")} failed:\n${r.stdout}\n${r.stderr}`);
}

let args: string[];
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
} else if (kind === "platform") {
  const db = `sqlite://${join(dir, "platform.sqlite")}`;
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
} else throw new Error("usage: serve.ts app|platform");

// Connectors: the specs run a receiver on loopback, which is refused unless the operator allows it.
// The secret key is a 32-byte key in the form two-factor authentication and connectors both accept.
const child = spawn(bin, args, {
  stdio: "inherit",
  env: {
    ...process.env,
    OBOP_SECRET_KEY: secretKey,
    OBOP_REQUIRE_MFA_FOR_ADMINS: "0",
    OBOP_CONNECTORS_ALLOW_LOOPBACK: "1",
    OBOP_PUBLIC_URL: `http://127.0.0.1:${process.env.OBOP_E2E_APP_PORT}`,
  },
});
for (const sig of ["SIGTERM", "SIGINT"] as const) process.on(sig, () => child.kill(sig));
child.on("exit", (code) => process.exit(code ?? 0));
