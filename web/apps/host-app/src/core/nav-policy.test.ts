import { describe, expect, test } from "vite-plus/test";
import {
  assignScript,
  decideNavigation,
  isStartDocument,
  schemeOf,
  topVerdict,
} from "./nav-policy.ts";

const origin = "https://erp.example.com";

const same = (url: string) => decideNavigation({ url, isTopFrame: true, newWindow: false, origin });
const blank = (url: string) => decideNavigation({ url, isTopFrame: true, newWindow: true, origin });

describe("a navigation in the same window", () => {
  test("the stored origin loads, by an exact comparison of origins", () => {
    for (const url of [
      "https://erp.example.com",
      "https://erp.example.com/",
      "https://erp.example.com/admin/#/crm/lead",
      "https://erp.example.com:443/api/files/1",
      "HTTPS://ERP.EXAMPLE.COM/x?y=1",
    ]) {
      expect(same(url), url).toBe("load");
    }
  });

  test("an origin that only starts like the stored one goes to the system browser", () => {
    for (const url of [
      // What the library's own prefix match would let in.
      "https://erp.example.com.evil.net/",
      "https://erp.example.comx/",
      "https://erp.example.com:8443/",
      "http://erp.example.com/",
      "https://sub.erp.example.com/",
      "https://example.com/",
      // An address the standard reads another way than its text suggests.
      "https://erp.example.com@evil.net/",
      "https://erp.example.com%2f.evil.net/",
    ]) {
      expect(same(url), url).toBe("external");
    }
  });

  test("mail, phone, message and map addresses go to the system", () => {
    for (const url of ["mailto:a@example.com", "tel:+46123456", "sms:+46123456", "geo:59.3,18.0"]) {
      expect(same(url), url).toBe("external");
    }
    expect(same("MAILTO:a@example.com")).toBe("external");
  });

  test("an empty document and a blob of the instance load", () => {
    expect(same("about:blank")).toBe("load");
    expect(same("about:srcdoc")).toBe("load");
    expect(same("about:blank#x")).toBe("load");
    expect(same("blob:https://erp.example.com/0b0d1a0e-1111-2222-3333-444455556666")).toBe("load");
  });

  test("everything else is dropped", () => {
    for (const url of [
      "intent://scan/#Intent;scheme=zxing;end",
      "javascript:alert(1)",
      "data:text/html,<script>1</script>",
      "file:///data/data/org.obop.app.test/files/x",
      "content://media/external/file/1",
      "market://details?id=x",
      "about:config",
      "about:blankx",
      "blob:https://evil.net/0b0d1a0e",
      "blob:null/0b0d1a0e",
      "chrome://flags",
      "",
      "/relative/path",
      "erp.example.com",
    ]) {
      expect(same(url), url).toBe("refuse");
    }
  });

  test("a frame loads what it likes (iOS says which frame asks; the back office has none)", () => {
    for (const url of ["https://evil.net/", "about:blank", "data:text/html,x"]) {
      expect(decideNavigation({ url, isTopFrame: false, newWindow: false, origin }), url).toBe(
        "load",
      );
    }
  });
});

describe("a new window", () => {
  test("an address of the instance goes to the one WebView, another origin to the system", () => {
    expect(blank("https://erp.example.com/#/crm/lead/7")).toBe("load");
    expect(blank("https://docs.example.org/guide")).toBe("external");
    expect(blank("https://erp.example.com.evil.net/")).toBe("external");
    expect(blank("mailto:a@example.com")).toBe("external");
  });

  test("a blob, an empty document and every other scheme are refused", () => {
    for (const url of [
      "blob:https://erp.example.com/0b0d1a0e",
      "about:blank",
      "javascript:alert(1)",
      "data:text/html,x",
      "intent://x",
      "file:///x",
    ]) {
      expect(blank(url), url).toBe("refuse");
    }
    // A frame that asks for a new window gets no more than the top document does.
    expect(
      decideNavigation({ url: "about:blank", isTopFrame: false, newWindow: true, origin }),
    ).toBe("refuse");
  });
});

describe("what a reported top-level address says about the document on top", () => {
  test("a document of the instance", () => {
    expect(topVerdict("https://erp.example.com/#/crm", origin)).toBe("instance");
    expect(topVerdict("blob:https://erp.example.com/0b0d", origin)).toBe("instance");
  });

  test("no document of anybody: an empty one, or Android's own page for a failed load", () => {
    expect(topVerdict("about:blank", origin)).toBe("neutral");
    expect(topVerdict("chrome-error://chromewebdata/", origin)).toBe("neutral");
  });

  test("everything the app would not load in the top frame is foreign", () => {
    for (const url of [
      "https://erp.example.com.evil.net/",
      "https://example.com/",
      "http://erp.example.com/",
      "data:text/html,x",
      "javascript:1",
      "mailto:a@example.com",
      "blob:https://evil.net/1",
      "",
    ]) {
      expect(topVerdict(url, origin), url).toBe("foreign");
    }
  });
});

describe("small helpers", () => {
  test("the scheme of an address", () => {
    expect(schemeOf("HTTPS://x")).toBe("https");
    expect(schemeOf("mailto:a@b")).toBe("mailto");
    expect(schemeOf("/path")).toBeNull();
    expect(schemeOf("1x:y")).toBeNull();
  });

  test("the start document is the start address with or without a fragment", () => {
    const start = "https://erp.example.com/admin/";
    expect(isStartDocument("https://erp.example.com/admin/", start)).toBe(true);
    expect(isStartDocument("https://erp.example.com/admin/#/crm", start)).toBe(true);
    expect(isStartDocument("https://erp.example.com/admin/logo.png", start)).toBe(false);
    expect(isStartDocument("https://erp.example.com/", start)).toBe(false);
  });

  test("the script for a new window of the instance passes the address as a closed literal", () => {
    const script = assignScript("https://erp.example.com/#/crm");
    expect(script).toBe(
      '(function(){try{window.location.assign("https://erp.example.com/#/crm");}catch(e){}})();true;',
    );
    const hostile = assignScript('https://erp.example.com/#"</script><script>alert(1)//\u2028');
    expect(hostile).not.toContain("</script>");
    expect(hostile).not.toContain("\u2028");
    expect(hostile).toContain('\\"');
    expect(hostile).toContain("\\u003c/script\\u003e");
    // The literal is one string again when it is evaluated.
    const literal = /assign\((.*)\);\}catch/.exec(hostile)?.[1] ?? "";
    expect(JSON.parse(literal)).toBe('https://erp.example.com/#"</script><script>alert(1)//\u2028');
  });
});
