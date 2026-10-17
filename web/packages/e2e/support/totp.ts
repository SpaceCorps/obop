// RFC 6238 TOTP (HMAC-SHA-1, 6 digits, 30 s) for the two-factor specs, so the tests compute the codes
// an authenticator app would show. Test code only; the server has its own implementation in Rust.
import { createHmac } from "node:crypto";

const ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/** RFC 4648 base32, spaces and padding ignored (the setup key is shown in groups of four). */
export function base32Decode(text: string): Buffer {
  let bits = 0;
  let value = 0;
  const out: number[] = [];
  for (const c of text.toUpperCase().replace(/[\s=-]/g, "")) {
    const i = ALPHABET.indexOf(c);
    if (i < 0) throw new Error(`not base32: ${c}`);
    value = (value << 5) | i;
    bits += 5;
    if (bits >= 8) {
      out.push((value >>> (bits - 8)) & 0xff);
      bits -= 8;
    }
  }
  return Buffer.from(out);
}

export const STEP = 30;

export function hotp(secret: Buffer, counter: number, digits = 6): string {
  const msg = Buffer.alloc(8);
  msg.writeBigUInt64BE(BigInt(counter));
  const h = createHmac("sha1", secret).update(msg).digest();
  const off = h[h.length - 1]! & 0x0f;
  const bin = ((h[off]! & 0x7f) << 24) | (h[off + 1]! << 16) | (h[off + 2]! << 8) | h[off + 3]!;
  return String(bin % 10 ** digits).padStart(digits, "0");
}

/** The code for the 30 s step that contains `atMs` (default: now) plus `offset` steps. */
export function totp(secretBase32: string, offset = 0, atMs = Date.now()): string {
  return hotp(base32Decode(secretBase32), Math.floor(atMs / 1000 / STEP) + offset);
}

// RFC 6238 appendix B, so the helper itself is checked: SHA-1, secret "12345678901234567890".
const rfc = Buffer.from("12345678901234567890");
if (hotp(rfc, Math.floor(59 / STEP), 8) !== "94287082")
  throw new Error("the TOTP helper does not match RFC 6238");
