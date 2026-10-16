// Refreshes data/changelog.snapshot.json from `git log`, for builds without a .git directory (Docker).
import { writeSnapshot } from "./lib/changelog.ts";

console.log(`changelog snapshot: ${writeSnapshot()} commits`);
