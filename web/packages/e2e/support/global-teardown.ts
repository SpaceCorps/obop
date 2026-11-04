import { rmSync } from "node:fs";
import { stopServers } from "./shutdown.ts";

// The databases live in a per-run temp directory; remove it unless asked to keep it for debugging.
// Playwright stops its web servers only after this hook, so they are stopped first (and waited for):
// removing the directory under servers that still write to it fails with ENOTEMPTY, or cannot be done
// at all on Windows. Any failure here fails the run; nothing is swallowed.
export default async function globalTeardown(): Promise<void> {
  const dir = process.env.OBOP_E2E_DIR;
  if (!dir || process.env.OBOP_E2E_KEEP) return;
  await stopServers(dir);
  // The servers are gone; the retries only cover a file handle that the system lets go of a moment late.
  rmSync(dir, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
}
