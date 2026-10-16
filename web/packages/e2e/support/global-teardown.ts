import { rmSync } from "node:fs";

// The databases live in a per-run temp directory; remove it unless asked to keep it for debugging.
export default function globalTeardown(): void {
  const dir = process.env.OBOP_E2E_DIR;
  if (dir && !process.env.OBOP_E2E_KEEP) rmSync(dir, { recursive: true, force: true });
}
