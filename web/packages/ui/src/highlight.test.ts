import { describe, expect, it } from "vite-plus/test";
import { highlight } from "./highlight.ts";

const classes = (line: string, lang: string) =>
  highlight(line, lang)[0]!
    .filter((t) => t.cls)
    .map((t) => [t.cls, t.text]);

describe("highlight", () => {
  it("knows TOML tables, keys, strings, numbers, booleans and comments", () => {
    expect(classes("[[model.field]]", "toml")).toEqual([["table", "[[model.field]]"]]);
    expect(classes('name = "x" # why', "toml")).toEqual([
      ["key", "name"],
      ["punct", " = "],
      ["string", '"x"'],
      ["comment", "# why"],
    ]);
    expect(classes("required = true", "toml")).toContainEqual(["keyword", "true"]);
    expect(classes("version = 3", "toml")).toContainEqual(["number", "3"]);
    expect(classes('options = ["a", "b"]', "toml").filter(([c]) => c === "string")).toHaveLength(2);
  });

  it("a hash inside a string is not a comment", () => {
    expect(classes('color = "#fff"', "toml").some(([c]) => c === "comment")).toBe(false);
  });

  it("never changes the text: tokens put back together are the source", () => {
    const src = '[module]\nid = "assets"  # id\nlist = [1, 2.5, true]\n<script>alert(1)</script>';
    const joined = highlight(src, "toml")
      .map((l) => l.map((t) => t.text).join(""))
      .join("\n");
    expect(joined).toBe(src);
  });

  it("JSON keys and values, shell flags, and plain text for anything else", () => {
    expect(classes('{"a": 1, "b": "x", "c": null}', "json")).toEqual(
      expect.arrayContaining([
        ["key", '"a"'],
        ["number", "1"],
        ["string", '"x"'],
        ["keyword", "null"],
      ]),
    );
    expect(classes("cargo run -- modules check --db x # try", "sh")).toEqual(
      expect.arrayContaining([
        ["keyword", "--db"],
        ["comment", "# try"],
      ]),
    );
    expect(highlight("whatever 1 2", "text")[0]).toEqual([{ text: "whatever 1 2" }]);
  });
});
