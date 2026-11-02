// The sheet for the native-speaker review of the Swedish texts (docs/spec/mobile.md 8.9).
//
//   vp run i18n:review                      the sheet as Markdown on the standard output
//   vp run i18n:review -- --out FILE        the same into a file
//   vp run i18n:review -- --lang sv         the language to review (only sv has a catalogue so far)
//
// For every sentence of the catalogue: the key, the English text, the Swedish text, and the file
// that shows it, grouped by that file (a screen, `present.ts` for the words of a trip, the
// recorder for the notification). A reviewer reads the Swedish next to the English, changes the
// Swedish in `src/i18n/sv.ts` (the key is the line), and ticks the row. The texts of the system
// (the name under the icon and the two dialogs of iOS) are listed after the catalogue.
//
// The sheet is made from the sources and the catalogues; it is not checked in.
import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { PLURAL_CATEGORIES, en } from "../src/i18n/en.ts";
import { pluralBases } from "../src/i18n/check.ts";
import { LANGUAGES } from "../src/i18n/languages.ts";
import { keyFamilies, mentions } from "../src/i18n/usage.ts";
import { NATIVE_TEXTS } from "../config/native-texts.ts";

const appDir = join(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const valueOf = (name) => (args.includes(name) ? args[args.indexOf(name) + 1] : undefined);
const lang = valueOf("--lang") ?? "sv";
const out = valueOf("--out");
const target = LANGUAGES[lang];
if (!target || lang === "en") {
  console.error(
    `i18n-review: no catalogue to review for "${lang}" (have: ${Object.keys(LANGUAGES).join(", ")})`,
  );
  process.exit(2);
}

/** Every non-test source of the app that can show a sentence. */
function sources() {
  const files = [];
  const walk = (dir) => {
    for (const name of readdirSync(dir).sort()) {
      const path = join(dir, name);
      if (statSync(path).isDirectory()) {
        if (!["node_modules", "i18n", "test-support"].includes(name)) walk(path);
      } else if (/\.tsx?$/.test(name) && !/\.test\./.test(name)) {
        files.push({
          path: relative(appDir, path).replaceAll("\\", "/"),
          text: readFileSync(path, "utf8"),
        });
      }
    }
  };
  walk(join(appDir, "src"));
  files.push({ path: "App.tsx", text: readFileSync(join(appDir, "App.tsx"), "utf8") });
  return files;
}

/**
 * The sign-in and consent pages the system browser shows are the server's (`page.rs` of obop-oauth):
 * a table of rows `("key", [en, de, fr, pl, fi, sv, es])`. The sheet lists the English text and the
 * text of `lang` for each row, so the reviewer reads what a Swedish driver sees first.
 */
function pageCatalogue(code) {
  const text = readFileSync(
    join(appDir, "..", "..", "..", "crates", "kernel", "obop-oauth", "src", "page.rs"),
    "utf8",
  );
  const order = /pub const LANGS: \[&str; \d+\] = \[([^\]]*)\]/
    .exec(text)?.[1]
    ?.split(",")
    .map((x) => x.trim().replaceAll('"', ""));
  const column = order?.indexOf(code) ?? -1;
  const table = /pub const CATALOGUE[^=]*=\s*&\[([\s\S]*?)\n\];/.exec(text)?.[1];
  if (!order || column < 0 || !table) {
    console.error(
      "i18n-review: page.rs of obop-oauth has a different shape than this script reads",
    );
    process.exit(2);
  }
  const unescape = (raw) => raw.replace(/\\(.)/g, (_, c) => (c === "n" ? " " : c));
  const strings = (line) => [...line.matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((m) => unescape(m[1]));
  const rows = [];
  for (const line of table.split("\n")) {
    if (!/^\s*\("/.test(line)) continue;
    const [key, ...columns] = strings(line);
    if (columns.length !== order.length) {
      console.error(
        `i18n-review: page.rs row ${key} has ${columns.length} texts, not ${order.length}`,
      );
      process.exit(2);
    }
    rows.push({ key, english: columns[0], text: columns[column] });
  }
  return rows;
}

const files = sources();
const families = files.flatMap((file) =>
  [...keyFamilies(file.text)].map((family) => [family, file.path]),
);
const bases = pluralBases(en);

/** The names of the catalogue: a plural key by its base, an iOS version by the key it belongs to. */
const names = new Set();
for (const key of Object.keys(en)) {
  const base = key.replace(new RegExp(`\\.(${PLURAL_CATEGORIES.join("|")})$`), "");
  names.add((bases.has(base) ? base : key).replace(/\.ios$/, ""));
}

/** The files that show a name: the ones that have it as a string, and the ones whose template builds it. */
function whereIs(name) {
  const where = new Set(files.filter((file) => mentions(file.text, name)).map((file) => file.path));
  for (const [family, path] of families) {
    if (name.startsWith(`${family}.`) && !name.slice(family.length + 1).includes("."))
      where.add(path);
  }
  return [...where].sort();
}

const cell = (text) =>
  String(text ?? "")
    .replaceAll("|", "\\|")
    .replaceAll("\u00a0", "\u2423")
    .replaceAll("\n", " ");

/** The rows of a name: the plain sentence, its iOS version, or one row per plural form. */
function rowsOf(name) {
  const rows = [];
  const forms = bases.has(name)
    ? [...new Set([...target.categories, "other"])].map((category) => [
        `${name}.${category}`,
        `${name}.${category}`,
      ])
    : [[name, name]];
  for (const [key, sourceKey] of forms) {
    rows.push({
      key,
      english: en[sourceKey] ?? en[`${name}.other`],
      swedish: target.messages[key],
    });
  }
  if (`${name}.ios` in en) {
    rows.push({
      key: `${name}.ios`,
      english: en[`${name}.ios`],
      swedish: target.messages[`${name}.ios`],
    });
  }
  return rows;
}

const groups = new Map();
for (const name of [...names].sort((a, b) => a.localeCompare(b))) {
  const where = whereIs(name);
  const group =
    where.length === 0
      ? "Not read by any screen yet"
      : where.length === 1
        ? where[0]
        : "Several files";
  if (!groups.has(group)) groups.set(group, []);
  groups.get(group).push({ name, where });
}

const lines = [];
lines.push(`# Review sheet: ${target.name} (${lang}) texts of the Mileage app`);
lines.push("");
lines.push(
  `Made by \`vp run i18n:review\`. ${names.size} sentences. MACHINE-DRAFTED: no native speaker has read them yet.`,
);
lines.push(
  "Change the Swedish in `src/i18n/sv.ts` (search for the key); `\u2423` marks a no-break space.",
);
lines.push("");
for (const [group, entries] of [...groups].sort(([a], [b]) => a.localeCompare(b))) {
  lines.push(`## ${group}`);
  lines.push("");
  lines.push("| Done | Key | English | Svenska | Also shown in |");
  lines.push("| --- | --- | --- | --- | --- |");
  for (const { name, where } of entries) {
    for (const row of rowsOf(name)) {
      const also = group === "Several files" ? where.join(", ") : "";
      lines.push(
        `| [ ] | \`${row.key}\` | ${cell(row.english)} | ${cell(row.swedish)} | ${also} |`,
      );
    }
  }
  lines.push("");
}
lines.push("## System texts (config/native-texts.ts)");
lines.push("");
lines.push("Shown by the system in the language of the phone, not the language chosen in the app.");
lines.push("");
lines.push("| Done | Where | English | Svenska |");
lines.push("| --- | --- | --- | --- |");
const placeOf = {
  appName: "Name under the icon (iOS CFBundleDisplayName, Android app_name)",
  locationWhenInUse: "iOS dialog for the location (NSLocationWhenInUseUsageDescription)",
  localNetwork: "iOS dialog for the local network (NSLocalNetworkUsageDescription)",
};
for (const key of Object.keys(placeOf)) {
  lines.push(
    `| [ ] | ${placeOf[key]} | ${cell(NATIVE_TEXTS.en[key])} | ${cell(NATIVE_TEXTS[lang]?.[key])} |`,
  );
}
lines.push("");
lines.push("");
lines.push("## Sign-in and consent pages of the instance (crates/kernel/obop-oauth/src/page.rs)");
lines.push("");
lines.push(
  "These are the pages the system browser shows for the sign-in of the app. The app asks for the language it knows (`lang`), so a Swedish app shows the Swedish column. It is machine-drafted too (docs/ISSUES.md D7) and is part of the Swedish review: the server, not the app, holds it. Change the text in `page.rs`.",
);
lines.push("");
lines.push("| Done | Key | English | Svenska |");
lines.push("| --- | --- | --- | --- |");
for (const row of pageCatalogue(lang)) {
  lines.push(`| [ ] | \`${row.key}\` | ${cell(row.english)} | ${cell(row.text)} |`);
}
lines.push("");
lines.push(
  "The label table of the module on the server is `packages/labels/sv/mileage.toml` (its own review).",
);
lines.push("");

const sheet = lines.join("\n");
if (out) writeFileSync(out, sheet);
else process.stdout.write(sheet);
