import { describe, expect, test } from "vite-plus/test";
import { deviceName } from "./device.ts";

describe("the name of the device that the instance lists", () => {
  test("Android gives the maker and the model", () => {
    expect(deviceName({ OS: "android", constants: { Brand: "google", Model: "Pixel 9" } })).toBe(
      "google Pixel 9",
    );
    expect(deviceName({ OS: "android", constants: { Model: " Pixel 9 " } })).toBe("Pixel 9");
    expect(deviceName({ OS: "android", constants: {} })).toBe("Android phone");
    expect(deviceName({ OS: "android" })).toBe("Android phone");
  });

  test("an iPhone is not called an Android phone", () => {
    expect(deviceName({ OS: "ios", constants: { interfaceIdiom: "phone" } })).toBe("iPhone");
    expect(deviceName({ OS: "ios", constants: { interfaceIdiom: "pad" } })).toBe("iPad");
    expect(deviceName({ OS: "ios", constants: { interfaceIdiom: "vision" } })).toBe("iOS device");
    expect(deviceName({ OS: "ios", constants: {} })).toBe("iOS device");
    expect(deviceName({ OS: "ios", constants: null })).toBe("iOS device");
  });

  test("anything else has a plain name", () => {
    expect(deviceName({ OS: "web" })).toBe("Phone");
  });
});
