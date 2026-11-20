// The mail server guard's refusal of a password without TLS reads as the form's own words.
import { beforeEach, describe, expect, it } from "vite-plus/test";
import { provideCatalogue, setLanguage } from "@obop/ui";
import { mailRefusal } from "./refusal.ts";

beforeEach(async () => {
  provideCatalogue("en", {});
  await setLanguage("en", false);
});

describe("mailRefusal", () => {
  it("names the labels of the form, not the option values", () => {
    const m = mailRefusal(
      "a password is never sent without TLS: choose starttls or tls, leave the user name empty, or allow the business's own network",
    );
    expect(m).toBe(
      "A password is never sent without TLS. Set Security to STARTTLS or TLS, leave the user name empty, or switch on “Allow a relay on our own network”.",
    );
  });

  it("says what to do when the switch is being turned off", () => {
    const m = mailRefusal(
      "a password is never sent without TLS: to turn off the business's own network, also choose starttls or tls, or leave the user name empty",
    );
    expect(m).toBe(
      "A password is never sent without TLS. To switch off “Allow a relay on our own network”, also set Security to STARTTLS or TLS, or leave the user name empty.",
    );
  });

  it("leaves any other message alone", () => {
    expect(mailRefusal("host is required")).toBeUndefined();
    expect(mailRefusal(undefined)).toBeUndefined();
  });
});
