import { describe, expect, test } from "vite-plus/test";
import { originOf } from "./origin.ts";

// Node's `URL` follows the URL standard, as a browser's does, so it is the reference here. React
// Native's does not (the note at the top of origin.ts), which is why the origin is worked out by
// hand: these tests hold the hand-written rule to the standard.

/** The origin by the standard, or `null` where the standard refuses the address or it has none. */
function standard(address: string): string | null {
  try {
    const origin = new URL(address).origin;
    return origin === "null" ? null : origin;
  } catch {
    return null;
  }
}

describe("the origin of an address", () => {
  const same: [string, string][] = [
    ["https://erp.example.com", "https://erp.example.com"],
    ["https://erp.example.com/", "https://erp.example.com"],
    ["https://erp.example.com/admin/#/contacts?q=a\\b c", "https://erp.example.com"],
    ["https://erp.example.com?next=/x", "https://erp.example.com"],
    ["https://erp.example.com#/crm", "https://erp.example.com"],
    ["HTTPS://ERP.Example.COM/Path", "https://erp.example.com"],
    ["https://erp.example.com:443/", "https://erp.example.com"],
    ["http://erp.example.com:80/", "http://erp.example.com"],
    ["https://erp.example.com:80/", "https://erp.example.com:80"],
    ["http://erp.example.com:443/", "http://erp.example.com:443"],
    ["https://erp.example.com:8443/x", "https://erp.example.com:8443"],
    ["https://erp.example.com:08443/", "https://erp.example.com:8443"],
    ["https://erp.example.com:/", "https://erp.example.com"],
    ["http://localhost:18180/", "http://localhost:18180"],
    ["http://localhost:18180", "http://localhost:18180"],
    ["http://shop.localhost:18180/admin/", "http://shop.localhost:18180"],
    ["http://10.0.2.2:18180/", "http://10.0.2.2:18180"],
    ["http://192.168.1.132:18081/", "http://192.168.1.132:18081"],
    ["http://0.0.0.0/", "http://0.0.0.0"],
    ["http://255.255.255.255/", "http://255.255.255.255"],
    ["http://[::1]:18180/", "http://[::1]:18180"],
    ["http://[0:0:0:0:0:0:0:1]/", "http://[::1]"],
    ["http://[FE80::1]/", "http://[fe80::1]"],
    ["http://[fd00:0:0:1::2]:8080/", "http://[fd00:0:0:1::2]:8080"],
    ["http://[2001:db8:0:0:1:0:0:1]/", "http://[2001:db8::1:0:0:1]"],
    ["http://[1:2:3:4:5:6:7::]/", "http://[1:2:3:4:5:6:7:0]"],
    ["http://[1:0:0:2:0:0:0:3]/", "http://[1:0:0:2::3]"],
    ["http://[::]/", "http://[::]"],
    ["https://xn--mnchen-3ya.example/", "https://xn--mnchen-3ya.example"],
    ["https://a-b.c-d.example/", "https://a-b.c-d.example"],
    ["https://1.example/", "https://1.example"],
    ["https://example.1a/", "https://example.1a"],
  ];

  test.each(same)("%s is %s, as the standard has it", (address, origin) => {
    expect(originOf(address)).toBe(origin);
    expect(standard(address)).toBe(origin);
  });

  // Each of these could be taken for the instance by a rule that looks at how the address begins.
  const foreign: string[] = [
    "https://erp.example.com.evil.net/",
    "https://erp.example.com@evil.net/",
    "https://erp.example.com:443@evil.net/",
    "https://erp.example.com%2f@evil.net/",
    "https://evil.net/https://erp.example.com/",
    "https://evil.net#https://erp.example.com/",
    "https://evil.net?https://erp.example.com/",
    "https://erp.example.co/",
    "https://xerp.example.com/",
    "http://erp.example.com/",
    "https://erp.example.com:8443/",
  ];

  test.each(foreign)("%s is not https://erp.example.com", (address) => {
    expect(originOf(address)).not.toBe("https://erp.example.com");
    expect(standard(address)).not.toBe("https://erp.example.com");
  });

  // Where the standard rewrites the address in a way that is not done here, or refuses it, the
  // answer is null: never an origin that the standard would not give.
  const none: string[] = [
    "",
    " https://erp.example.com/",
    "erp.example.com",
    "//erp.example.com/",
    "https:erp.example.com",
    "https:/erp.example.com",
    "https:///erp.example.com",
    "https://",
    "https:///",
    "https://user@erp.example.com/",
    "https://user:pw@erp.example.com/",
    "https://erp.example.com\\@evil.net/",
    "https://evil.net\\@erp.example.com/",
    "https://erp.example.com\\.evil.net/",
    "https://erp.exa mple.com/",
    "https://erp.example.com\t/",
    "https://erp.example.com\n.evil.net/",
    "https://erp_example.com/",
    "https://erp.example.com./",
    "https://erp..example.com/",
    "https://.example.com/",
    "https://münchen.example/",
    "https://exаmple.com/",
    "https://127.1/",
    "https://0x7f.1/",
    "https://0x7f000001/",
    "https://2130706433/",
    "https://010.0.0.1/",
    "https://256.1.1.1/",
    "https://1.2.3/",
    "https://1.2.3.4.5/",
    "https://example.123/",
    "https://example.0x1/",
    "https://erp.example.com:65536/",
    "https://erp.example.com:-1/",
    "https://erp.example.com:8a/",
    "https://erp.example.com:80:80/",
    "http://[::1/",
    "http://[::1]x/",
    "http://[::1]:x/",
    "http://[1:2:3:4:5:6:7]/",
    "http://[1:2:3:4:5:6:7:8:9]/",
    "http://[1:2:3:4:5:6:7:8::]/",
    "http://[::1::]/",
    "http://[12345::]/",
    "http://[g::1]/",
    "http://[::ffff:1.2.3.4]/",
    "http://[fe80::1%25eth0]/",
    "http://[]/",
    "about:blank",
    "about:srcdoc",
    "data:text/html,<p>x</p>",
    "file:///sdcard/x.html",
    "content://media/x",
    "javascript:alert(1)",
    "intent://erp.example.com/#Intent;scheme=https;end",
    "blob:https://erp.example.com/3f0c",
    "ftp://erp.example.com/",
    "ws://erp.example.com/",
    "null",
  ];

  test.each(none)("%j has no origin here", (address) => {
    expect(originOf(address)).toBeNull();
  });

  test("whatever is given an origin has the same one by the standard", () => {
    for (const address of [...same.map(([a]) => a), ...foreign, ...none]) {
      const here = originOf(address);
      if (here !== null) expect(standard(address), address).toBe(here);
    }
  });

  test("anything that is not text has none", () => {
    for (const value of [undefined, null, 1, {}, [], true]) {
      expect(originOf(value as unknown as string)).toBeNull();
    }
  });

  test("an origin is its own origin, so a stored one can be compared as text", () => {
    for (const [, origin] of same) expect(originOf(origin)).toBe(origin);
  });

  test("the known limit: an xn-- label is kept as text and never decoded", () => {
    // A broken label is therefore not refused here. Node's URL keeps it too; a browser may refuse
    // it when it loads the address, and then no document comes from it.
    for (const host of ["xn--.example", "xn--a.example", "xn--80ak6aa92e.example"]) {
      expect(originOf(`https://${host}/`), host).toBe(`https://${host}`);
      expect(standard(`https://${host}/`), host).toBe(`https://${host}`);
    }
  });

  test("a very long host is not looked at", () => {
    const label = "a".repeat(60);
    const host = Array.from({ length: 6 }, () => label).join(".");
    expect(host.length).toBeGreaterThan(300);
    expect(originOf(`https://${host}/`)).toBeNull();
  });
});
