//! The address of the client, in one place for every limiter and every record that names one: the
//! signer door, the OAuth pages, passkeys, Microsoft sign-in and the public site.
//!
//! Without `--trust-proxy` it is the peer of the socket. Behind a trusted proxy it is the last
//! entry of the last `X-Forwarded-For` line: a proxy that extends the client's line or adds a line
//! of its own writes its entry last, and what comes before it is whatever the client sent. When
//! that entry cannot be used (there is no line, the line is not text, the entry is empty, longer
//! than 64 characters or not an address) the peer is used, as if there had been no header: a
//! client cannot blank its address or choose its limiter bucket by writing something odd. An
//! `ip:port` or `[ipv6]:port` entry is read as its address, because the port changes with every
//! connection.

use axum::extract::ConnectInfo;
use axum::http::{Extensions, HeaderMap};
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

/// The address (canonical text of an IP address), or an empty text when there is none (a call
/// that did not come over a socket).
pub(crate) fn client_address(
    trust_proxy: bool,
    headers: &HeaderMap,
    extensions: &Extensions,
) -> String {
    if trust_proxy && let Some(a) = forwarded_entry(headers) {
        return a;
    }
    extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip().to_string())
        .unwrap_or_default()
}

/// The entry the proxy wrote, when it is an address.
fn forwarded_entry(headers: &HeaderMap) -> Option<String> {
    let line = headers
        .get_all("x-forwarded-for")
        .iter()
        .next_back()?
        .to_str()
        .ok()?;
    let last = line.rsplit(',').next()?.trim();
    if last.is_empty() || last.len() > 64 {
        return None;
    }
    IpAddr::from_str(last)
        .or_else(|_| SocketAddr::from_str(last).map(|s| s.ip()))
        .ok()
        .map(|a| a.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(lines: &[&[u8]]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for l in lines {
            h.append("x-forwarded-for", HeaderValue::from_bytes(l).unwrap());
        }
        h
    }

    fn with_peer(peer: &str) -> Extensions {
        let mut e = Extensions::new();
        e.insert(ConnectInfo(SocketAddr::from_str(peer).unwrap()));
        e
    }

    #[test]
    fn behind_a_trusted_proxy_the_last_entry_of_the_last_line_is_the_client() {
        let peer = with_peer("10.0.0.5:4000");
        for (lines, want) in [
            (vec![&b"198.51.100.7"[..]], "198.51.100.7"),
            (vec![&b"203.0.113.9, 198.51.100.7"[..]], "198.51.100.7"),
            (
                vec![&b"192.0.2.1"[..], &b"203.0.113.9,198.51.100.8"[..]],
                "198.51.100.8",
            ),
            // The port changes with every connection; it is not part of the address.
            (vec![&b"198.51.100.7:51000"[..]], "198.51.100.7"),
            (vec![&b"[2001:db8::7]:51000"[..]], "2001:db8::7"),
            (vec![&b"2001:0db8:0:0:0:0:0:7"[..]], "2001:db8::7"),
            (vec![&b"  198.51.100.7  "[..]], "198.51.100.7"),
        ] {
            assert_eq!(
                client_address(true, &headers(&lines), &peer),
                want,
                "{lines:?}"
            );
        }
    }

    #[test]
    fn an_entry_that_cannot_be_used_is_the_peer_and_never_a_blank_or_a_bucket_of_the_clients_choosing()
     {
        let peer = with_peer("10.0.0.5:4000");
        let long = format!("198.51.100.7, {}", "1".repeat(80));
        for (what, lines) in [
            ("an empty entry", vec![&b"198.51.100.7,"[..]]),
            ("a blank line", vec![&b""[..]]),
            ("a name", vec![&b"203.0.113.9, client-a"[..]]),
            ("a zone id", vec![&b"fe80::1%eth0"[..]]),
            ("an entry that is too long", vec![long.as_bytes()]),
            // The proxy appended after a byte that is not text: the line cannot be read at all.
            ("a line that is not text", vec![&b"\xe9, 198.51.100.7"[..]]),
            ("a port without an address", vec![&b":51000"[..]]),
        ] {
            assert_eq!(
                client_address(true, &headers(&lines), &peer),
                "10.0.0.5",
                "{what}"
            );
        }
    }

    #[test]
    fn without_a_trusted_proxy_or_a_header_it_is_the_peer() {
        let peer = with_peer("10.0.0.5:4000");
        let h = headers(&[b"198.51.100.7"]);
        assert_eq!(client_address(false, &h, &peer), "10.0.0.5");
        assert_eq!(client_address(true, &HeaderMap::new(), &peer), "10.0.0.5");
        // No socket at all (a test rig): the header, when trusted; else nothing.
        assert_eq!(client_address(true, &h, &Extensions::new()), "198.51.100.7");
        assert_eq!(client_address(false, &h, &Extensions::new()), "");
        assert_eq!(
            client_address(true, &headers(&[b"junk"]), &Extensions::new()),
            ""
        );
    }
}
