import { describe, expect, test } from "vite-plus/test";
import {
  addressText,
  hostAndPort,
  hostOf,
  isCleartext,
  parseAddress,
  startAddress,
} from "./address.ts";
import { isLocalHost } from "./local-host.ts";

const plain = { cleartext: false };
const testBuild = { cleartext: true };

describe("the form of an address", () => {
  test("https with a host, a port if needed and at most one path segment", () => {
    expect(parseAddress("https://shop.example.com", plain)).toEqual({
      ok: true,
      origin: "https://shop.example.com",
      path: "/",
    });
    expect(parseAddress("https://shop.example.com:8443/admin", plain)).toEqual({
      ok: true,
      origin: "https://shop.example.com:8443",
      path: "/admin/",
    });
  });

  test("a harmless spelling gives the same address", () => {
    const same = { ok: true, origin: "https://shop.example.com", path: "/admin/" };
    for (const text of [
      "https://shop.example.com/admin",
      "https://shop.example.com/admin/",
      "  https://shop.example.com/admin  ",
      "HTTPS://Shop.Example.COM/admin",
      "https://shop.example.com:443/admin",
      "shop.example.com/admin",
    ]) {
      expect(parseAddress(text, plain), text).toEqual(same);
    }
    expect(parseAddress("https://shop.example.com/", plain)).toMatchObject({ path: "/" });
  });

  test("a text without a scheme is read as https, never as http", () => {
    expect(parseAddress("localhost:18180", testBuild)).toEqual({
      ok: true,
      origin: "https://localhost:18180",
      path: "/",
    });
    expect(parseAddress("192.168.1.20", testBuild)).toMatchObject({
      origin: "https://192.168.1.20",
    });
  });

  test("the path follows the server's rule for an admin prefix", () => {
    for (const good of ["/admin", "/back-office", "/bo_2", `/${"a".repeat(32)}`]) {
      expect(parseAddress(`https://h.example${good}`, plain), good).toMatchObject({ ok: true });
    }
    for (const bad of ["/Admin", "/a/b", "/admin/x/", "//", `/${"a".repeat(33)}`, "/a.b", "/ä"]) {
      expect(parseAddress(`https://h.example${bad}`, plain), bad).toEqual({
        ok: false,
        problem: "path",
      });
    }
  });

  test("another scheme is refused, and so is a scheme written without its two slashes", () => {
    for (const text of ["ftp://h.example", "file:///etc/hosts", "mailto:a@h.example"]) {
      expect(parseAddress(text, testBuild), text).toEqual({ ok: false, problem: "scheme" });
    }
    expect(parseAddress("https:/h.example", plain)).toEqual({ ok: false, problem: "scheme" });
    expect(parseAddress("javascript:alert(1)", plain)).toEqual({ ok: false, problem: "scheme" });
  });

  test("a user name, a password, a query, a fragment, a space or a backslash is refused", () => {
    for (const text of [
      "https://dave@h.example",
      "https://dave:secret@h.example",
      "https://h.example/admin?x=1",
      "https://h.example/#/crm",
      "https://h.example /admin",
      "https://h.example\\admin",
      "https://h.example@evil.example",
    ]) {
      expect(parseAddress(text, plain), text).toEqual({ ok: false, problem: "extras" });
    }
  });

  test("a host outside ASCII is refused in words of its own; its xn-- form passes", () => {
    expect(parseAddress("https://münchen.example", plain)).toEqual({
      ok: false,
      problem: "non_ascii",
    });
    // A Cyrillic letter that looks like a Latin one cannot be typed.
    expect(parseAddress("https://exаmple.com", plain)).toEqual({ ok: false, problem: "non_ascii" });
    expect(parseAddress("https://xn--mnchen-3ya.example", plain)).toMatchObject({
      ok: true,
      origin: "https://xn--mnchen-3ya.example",
    });
  });

  test("something that is no host or no port is refused", () => {
    for (const text of [
      "https://",
      "https://h_x.example",
      "https://h.example:99999",
      "https://h.example:port",
      "https://127.1",
      "https://[::1",
      "https://.example",
    ]) {
      expect(parseAddress(text, plain), text).toEqual({ ok: false, problem: "host" });
    }
    expect(parseAddress("   ", plain)).toEqual({ ok: false, problem: "empty" });
  });

  test("IPv4 and bracketed IPv6 literals are hosts", () => {
    expect(parseAddress("https://10.0.2.2:8443", plain)).toMatchObject({
      origin: "https://10.0.2.2:8443",
    });
    expect(parseAddress("https://[2001:DB8::1]:8443/admin", plain)).toEqual({
      ok: true,
      origin: "https://[2001:db8::1]:8443",
      path: "/admin/",
    });
  });
});

describe("the scheme rule", () => {
  test("a plain build refuses every http address, the local ones too", () => {
    for (const text of [
      "http://localhost:18180",
      "http://192.168.1.20",
      "http://shop.example.com",
    ]) {
      expect(parseAddress(text, plain), text).toEqual({ ok: false, problem: "cleartext" });
    }
  });

  test("a test build takes http only to this device or the local network", () => {
    for (const text of [
      "http://localhost:18180",
      "http://app.localhost",
      "http://printer.local",
      "http://127.0.0.1:18180",
      "http://10.0.2.2:18180",
      "http://172.16.0.1",
      "http://172.31.255.254",
      "http://192.168.1.20:18180/admin",
      "http://169.254.10.10",
      "http://[::1]:18180",
      "http://[fe80::1]",
      "http://[fd12:3456::1]",
    ]) {
      expect(parseAddress(text, testBuild), text).toMatchObject({ ok: true });
    }
    for (const text of [
      "http://shop.example.com",
      "http://8.8.8.8",
      "http://172.32.0.1",
      "http://172.15.0.1",
      "http://192.169.1.1",
      "http://11.0.0.1",
      "http://[2001:db8::1]",
      "http://localhost.example.com",
      "http://local",
      "http://notlocalhost",
    ]) {
      expect(parseAddress(text, testBuild), text).toEqual({
        ok: false,
        problem: "cleartext_public",
      });
    }
  });

  test("https passes in every build, to every host", () => {
    for (const options of [plain, testBuild]) {
      expect(parseAddress("https://shop.example.com", options)).toMatchObject({ ok: true });
      expect(parseAddress("https://localhost:18180", options)).toMatchObject({ ok: true });
    }
  });
});

describe("the table of local hosts", () => {
  test("names", () => {
    expect(isLocalHost("localhost")).toBe(true);
    expect(isLocalHost("a.b.localhost")).toBe(true);
    expect(isLocalHost("nas.local")).toBe(true);
    expect(isLocalHost("local")).toBe(false);
    expect(isLocalHost("localhost.evil.net")).toBe(false);
    expect(isLocalHost("mylocalhost")).toBe(false);
  });

  test("IPv4: 127/8, 10/8, 172.16/12, 192.168/16, 169.254/16", () => {
    for (const host of ["127.0.0.1", "127.255.0.9", "10.1.2.3", "172.16.0.0", "172.31.9.9"]) {
      expect(isLocalHost(host), host).toBe(true);
    }
    for (const host of ["192.168.0.1", "169.254.1.1"]) expect(isLocalHost(host), host).toBe(true);
    for (const host of ["126.0.0.1", "128.0.0.1", "172.15.0.1", "172.32.0.1", "192.167.1.1"]) {
      expect(isLocalHost(host), host).toBe(false);
    }
    for (const host of ["169.253.1.1", "9.255.255.255", "1.2.3"]) {
      expect(isLocalHost(host), host).toBe(false);
    }
  });

  test("IPv6: ::1, fe80::/10, fc00::/7, and nothing written as an IPv4 address inside", () => {
    for (const host of ["[::1]", "[fe80::1]", "[febf::1]", "[fc00::1]", "[fdff:1::2]"]) {
      expect(isLocalHost(host), host).toBe(true);
    }
    for (const host of ["[::2]", "[fec0::1]", "[fe7f::1]", "[fbff::1]", "[fe00::1]"]) {
      expect(isLocalHost(host), host).toBe(false);
    }
    for (const host of ["[2001:db8::1]", "[::ffff:7f00:1]", "[::]"]) {
      expect(isLocalHost(host), host).toBe(false);
    }
  });
});

describe("the texts of an address", () => {
  const root = { origin: "http://localhost:18180", path: "/" };
  const prefixed = { origin: "https://[2001:db8::1]:8443", path: "/admin/" };

  test("as a person reads it, and as the WebView is started on it", () => {
    expect(addressText(root)).toBe("http://localhost:18180");
    expect(startAddress(root)).toBe("http://localhost:18180/");
    expect(addressText(prefixed)).toBe("https://[2001:db8::1]:8443/admin");
    expect(startAddress(prefixed)).toBe("https://[2001:db8::1]:8443/admin/");
  });

  test("what is typed back gives the same address", () => {
    for (const address of [root, prefixed]) {
      expect(parseAddress(addressText(address), testBuild)).toEqual({ ok: true, ...address });
    }
  });

  test("the host, the host with its port, and whether it is plain http", () => {
    expect(hostOf(root.origin)).toBe("localhost");
    expect(hostAndPort(root.origin)).toBe("localhost:18180");
    expect(hostOf(prefixed.origin)).toBe("[2001:db8::1]");
    expect(hostAndPort(prefixed.origin)).toBe("[2001:db8::1]:8443");
    expect(hostOf("https://shop.example.com")).toBe("shop.example.com");
    expect(isCleartext(root.origin)).toBe(true);
    expect(isCleartext(prefixed.origin)).toBe(false);
  });
});
