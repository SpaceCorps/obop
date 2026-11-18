import { describe, expect, it } from "vite-plus/test";
import { downloadPath } from "./blobs.ts";

describe("the URL of a download", () => {
  it("names the record it comes from, so the file is named as that record holds it (K55)", () => {
    expect(downloadPath("abc", { model: "document", record: "01a1", field: "file" })).toBe(
      "/api/blobs/abc?model=document&record=01a1&field=file",
    );
  });
  it("is the bare blob without a record", () => {
    expect(downloadPath("abc")).toBe("/api/blobs/abc");
  });
  it("escapes what it carries", () => {
    expect(downloadPath("abc", { model: "a&b", record: "r=1", field: "f g" })).toBe(
      "/api/blobs/abc?model=a%26b&record=r%3D1&field=f+g",
    );
  });
});
