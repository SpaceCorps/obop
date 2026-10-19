//! A small, strict CBOR reader (RFC 8949) for the two things WebAuthn sends in CBOR: the
//! attestation object and the COSE public key. It reads definite lengths only, caps the depth and
//! never allocates more than the input can hold, so a hostile body cannot make it do much.

#[derive(Debug, Clone, PartialEq)]
pub enum Cbor {
    Int(i128),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<Cbor>),
    Map(Vec<(Cbor, Cbor)>),
    Bool(bool),
    Null,
    /// A float or a simple value: read, never used.
    Other,
}

const MAX_DEPTH: usize = 12;

impl Cbor {
    pub fn get(&self, key: &str) -> Option<&Cbor> {
        match self {
            Cbor::Map(m) => m
                .iter()
                .find(|(k, _)| matches!(k, Cbor::Text(t) if t == key))
                .map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn get_int(&self, key: i128) -> Option<&Cbor> {
        match self {
            Cbor::Map(m) => m
                .iter()
                .find(|(k, _)| matches!(k, Cbor::Int(i) if *i == key))
                .map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i128> {
        match self {
            Cbor::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Cbor::Bytes(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Cbor::Text(t) => Some(t),
            _ => None,
        }
    }
}

/// Decode one item from the start of `b`; returns it and the number of bytes it took.
pub fn decode_prefix(b: &[u8]) -> Result<(Cbor, usize), String> {
    let mut pos = 0;
    let v = item(b, &mut pos, 0)?;
    Ok((v, pos))
}

/// Decode `b` as exactly one item.
pub fn decode(b: &[u8]) -> Result<Cbor, String> {
    let (v, n) = decode_prefix(b)?;
    if n != b.len() {
        return Err("trailing bytes after the CBOR item".into());
    }
    Ok(v)
}

fn take<'a>(b: &'a [u8], pos: &mut usize, n: usize) -> Result<&'a [u8], String> {
    let end = pos.checked_add(n).filter(|e| *e <= b.len());
    let end = end.ok_or("CBOR input ends early")?;
    let s = &b[*pos..end];
    *pos = end;
    Ok(s)
}

fn argument(b: &[u8], pos: &mut usize, info: u8) -> Result<u64, String> {
    Ok(match info {
        0..=23 => u64::from(info),
        24 => u64::from(take(b, pos, 1)?[0]),
        25 => u64::from(u16::from_be_bytes(take(b, pos, 2)?.try_into().unwrap())),
        26 => u64::from(u32::from_be_bytes(take(b, pos, 4)?.try_into().unwrap())),
        27 => u64::from_be_bytes(take(b, pos, 8)?.try_into().unwrap()),
        _ => return Err("indefinite or reserved CBOR length".into()),
    })
}

fn item(b: &[u8], pos: &mut usize, depth: usize) -> Result<Cbor, String> {
    if depth > MAX_DEPTH {
        return Err("CBOR nests too deeply".into());
    }
    let head = take(b, pos, 1)?[0];
    let (major, info) = (head >> 5, head & 31);
    if major == 7 {
        return match info {
            20 => Ok(Cbor::Bool(false)),
            21 => Ok(Cbor::Bool(true)),
            22 => Ok(Cbor::Null),
            23 | 0..=19 => Ok(Cbor::Other),
            24 => take(b, pos, 1).map(|_| Cbor::Other),
            25 => take(b, pos, 2).map(|_| Cbor::Other),
            26 => take(b, pos, 4).map(|_| Cbor::Other),
            27 => take(b, pos, 8).map(|_| Cbor::Other),
            _ => Err("unsupported CBOR simple value".into()),
        };
    }
    let n = argument(b, pos, info)?;
    let remaining = b.len() - *pos;
    match major {
        0 => Ok(Cbor::Int(i128::from(n))),
        1 => Ok(Cbor::Int(-1 - i128::from(n))),
        2 | 3 => {
            let len = usize::try_from(n).map_err(|_| "CBOR string too long")?;
            let bytes = take(b, pos, len)?;
            if major == 2 {
                Ok(Cbor::Bytes(bytes.to_vec()))
            } else {
                String::from_utf8(bytes.to_vec())
                    .map(Cbor::Text)
                    .map_err(|_| "CBOR text is not UTF-8".into())
            }
        }
        4 | 5 => {
            // An element takes at least one byte (two in a map): refuse counts the input cannot hold.
            let per = if major == 4 { 1 } else { 2 };
            if n > (remaining / per) as u64 {
                return Err("CBOR count exceeds the input".into());
            }
            let n = n as usize;
            if major == 4 {
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(item(b, pos, depth + 1)?);
                }
                Ok(Cbor::Array(v))
            } else {
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    let k = item(b, pos, depth + 1)?;
                    let val = item(b, pos, depth + 1)?;
                    v.push((k, val));
                }
                Ok(Cbor::Map(v))
            }
        }
        6 => item(b, pos, depth + 1),
        _ => Err("unsupported CBOR type".into()),
    }
}

// ---- a tiny encoder, for the software authenticator of the tests --------------------------------

#[cfg(any(test, feature = "test-support"))]
pub mod enc {
    fn head(out: &mut Vec<u8>, major: u8, n: u64) {
        let m = major << 5;
        match n {
            0..=23 => out.push(m | n as u8),
            24..=0xff => out.extend([m | 24, n as u8]),
            0x100..=0xffff => {
                out.push(m | 25);
                out.extend((n as u16).to_be_bytes());
            }
            0x1_0000..=0xffff_ffff => {
                out.push(m | 26);
                out.extend((n as u32).to_be_bytes());
            }
            _ => {
                out.push(m | 27);
                out.extend(n.to_be_bytes());
            }
        }
    }
    pub fn int(out: &mut Vec<u8>, n: i64) {
        if n >= 0 {
            head(out, 0, n as u64);
        } else {
            head(out, 1, (-1 - n) as u64);
        }
    }
    pub fn bytes(out: &mut Vec<u8>, b: &[u8]) {
        head(out, 2, b.len() as u64);
        out.extend(b);
    }
    pub fn text(out: &mut Vec<u8>, t: &str) {
        head(out, 3, t.len() as u64);
        out.extend(t.as_bytes());
    }
    pub fn map(out: &mut Vec<u8>, n: usize) {
        head(out, 5, n as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_shapes_webauthn_uses() {
        // {"fmt": "none", "attStmt": {}, "authData": h'0102'}
        let mut b = Vec::new();
        enc::map(&mut b, 3);
        enc::text(&mut b, "fmt");
        enc::text(&mut b, "none");
        enc::text(&mut b, "attStmt");
        enc::map(&mut b, 0);
        enc::text(&mut b, "authData");
        enc::bytes(&mut b, &[1, 2]);
        let v = decode(&b).unwrap();
        assert_eq!(v.get("fmt").unwrap().as_text(), Some("none"));
        assert_eq!(v.get("authData").unwrap().as_bytes(), Some(&[1u8, 2][..]));
        // Negative integer keys, as in a COSE key.
        let mut k = Vec::new();
        enc::map(&mut k, 1);
        enc::int(&mut k, -7);
        enc::int(&mut k, -258);
        assert_eq!(
            decode(&k).unwrap().get_int(-7).unwrap().as_int(),
            Some(-258)
        );
    }

    #[test]
    fn prefix_decoding_reports_how_much_it_read() {
        let mut b = Vec::new();
        enc::text(&mut b, "ab");
        let n = b.len();
        b.extend([0xff, 0xff]);
        assert_eq!(decode_prefix(&b).unwrap().1, n);
        assert!(decode(&b).is_err());
    }

    #[test]
    fn hostile_input_is_refused_not_followed() {
        // An array that claims a billion elements.
        assert!(decode(&[0x9a, 0x3b, 0x9a, 0xca, 0x00]).is_err());
        // Indefinite length.
        assert!(decode(&[0x9f, 0xff]).is_err());
        // Truncated string.
        assert!(decode(&[0x65, b'a']).is_err());
        // Deeply nested arrays.
        assert!(decode(&[0x81; 64]).is_err());
        assert!(decode(&[]).is_err());
        // Invalid UTF-8 text.
        assert!(decode(&[0x62, 0xff, 0xfe]).is_err());
    }
}
