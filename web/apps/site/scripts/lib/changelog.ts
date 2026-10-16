import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { REPO_ROOT, SITE_ROOT } from "./paths.ts";

export interface Entry {
  hash: string;
  date: string;
  subject: string;
}

const SNAPSHOT = join(SITE_ROOT, "data/changelog.snapshot.json");

function fromGit(): Entry[] | null {
  if (!existsSync(join(REPO_ROOT, ".git"))) return null;
  try {
    const out = execFileSync(
      "git",
      ["log", "--no-merges", "--date=short", "--format=%h%x1f%ad%x1f%s"],
      {
        cwd: REPO_ROOT,
        encoding: "utf8",
        maxBuffer: 16 * 1024 * 1024,
      },
    );
    return out
      .split("\n")
      .filter(Boolean)
      .map((l) => {
        const [hash = "", date = "", subject = ""] = l.split("\x1f");
        return { hash, date, subject };
      })
      .filter((e) => !/^(Initial commit|WIP|fixup)/i.test(e.subject));
  } catch {
    return null;
  }
}

/** Commits from `git log`; falls back to the committed snapshot where there is no repository (Docker). */
export function loadChangelog(): { entries: Entry[]; source: "git" | "snapshot" } {
  const live = fromGit();
  if (live && live.length > 0) return { entries: live, source: "git" };
  return { entries: JSON.parse(readFileSync(SNAPSHOT, "utf8")) as Entry[], source: "snapshot" };
}

export function writeSnapshot(): number {
  const live = fromGit();
  if (!live) throw new Error("no git repository to snapshot");
  writeFileSync(SNAPSHOT, `${JSON.stringify(live, null, 2)}\n`);
  return live.length;
}

/** Markdown for the changelog page: one section per day, newest first. */
export function changelogMarkdown(entries: Entry[]): string {
  const days = new Map<string, Entry[]>();
  for (const e of entries) days.set(e.date, [...(days.get(e.date) ?? []), e]);
  const out: string[] = [];
  for (const [date, list] of [...days].sort((a, b) => b[0].localeCompare(a[0]))) {
    out.push(`## ${date}`, "");
    for (const e of list) out.push(`- ${e.subject.replace(/[<>{}]/g, "")} (\`${e.hash}\`)`);
    out.push("");
  }
  return out.join("\n");
}
