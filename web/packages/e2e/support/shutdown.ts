// How the global teardown stops the servers of a run and waits for them before it removes the run
// directory.
//
// Playwright runs `globalTeardown` while the web servers are still up and stops them only afterwards,
// so removing the directory there raced with servers that were still writing to it (ENOTEMPTY on
// Linux; open database files cannot be removed at all on Windows). The two sides talk through files
// in the run directory, because that works the same on every platform:
//   running/<kind>   a server launcher (`serve.ts <kind>`) writes it when it starts;
//   stop             the teardown writes it to ask every launcher to shut its server down;
//   stopped/<kind>   a launcher writes it as its last act before it exits.
import { existsSync, mkdirSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const RUNNING = "running";
const STOP = "stop";
const STOPPED = "stopped";

/**
 * For a server launcher: registers `kind` in the run directory and calls `stop` once, when the
 * teardown asks for it. Returns the function to call when everything the launcher started has
 * exited, just before the launcher itself does.
 */
export function register(dir: string, kind: string, stop: () => void): () => void {
  mkdirSync(join(dir, RUNNING), { recursive: true });
  writeFileSync(join(dir, RUNNING, kind), "");
  let asked = false;
  const timer = setInterval(() => {
    if (asked || !existsSync(join(dir, STOP))) return;
    asked = true;
    stop();
  }, 100);
  return () => {
    clearInterval(timer);
    // The directory may be gone if Playwright stopped this server on its own after a failed
    // teardown; there is nobody left to tell then.
    try {
      mkdirSync(join(dir, STOPPED), { recursive: true });
      writeFileSync(join(dir, STOPPED, kind), "");
    } catch {}
  };
}

/**
 * For the teardown: asks every registered launcher to stop its server and waits until all of them
 * have said so. Throws, naming the ones that did not, if that takes longer than `timeoutMs`.
 */
export async function stopServers(dir: string, timeoutMs = 30_000): Promise<void> {
  const kinds = existsSync(join(dir, RUNNING)) ? readdirSync(join(dir, RUNNING)) : [];
  writeFileSync(join(dir, STOP), "");
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const waiting = kinds.filter((kind) => !existsSync(join(dir, STOPPED, kind)));
    if (waiting.length === 0) return;
    if (Date.now() > deadline)
      throw new Error(
        `the e2e servers did not stop within ${timeoutMs / 1000} s: ${waiting.join(", ")} (${dir} was left in place)`,
      );
    await new Promise((done) => setTimeout(done, 100));
  }
}
