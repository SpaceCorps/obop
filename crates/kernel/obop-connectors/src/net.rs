//! The one outbound HTTP client of the connectors (D33: a single SSRF-hardened egress client).
//!
//! - https only. Plain http is allowed for a literal loopback host and only when the operator
//!   turned loopback on (`OBOP_CONNECTORS_ALLOW_LOOPBACK=1`, for tests and local development).
//! - The host must be on the allow-list of the provider or the connection (and, if the operator
//!   keeps one, of the instance).
//! - The name is resolved once, every address is checked, and the request connects to exactly
//!   those addresses (the resolution is pinned for the request), so a name that answers with a
//!   public address first and a private one later cannot redirect the connection (DNS
//!   rebinding). One non-public address among the answers refuses the request.
//! - Loopback, private, link-local, carrier-grade NAT, documentation, multicast, reserved and
//!   cloud-metadata ranges (IPv4 and IPv6, including IPv4-mapped and NAT64 forms) are refused.
//! - Redirects are never followed: a 3xx is an error, so a redirect cannot lead to another host.
//! - Timeouts and size caps on both directions; no URL query, header or body is ever put in an
//!   error or a log line.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

pub const USER_AGENT: &str = concat!("obop-connectors/", env!("CARGO_PKG_VERSION"));

/// Whether an address is one a connector may talk to (a public, routable unicast address).
pub fn ip_is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4_is_public(v4),
        IpAddr::V6(v6) => v6_is_public(v6),
    }
}

fn v4_is_public(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 88 && c == 99)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224)
}

fn v6_is_public(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return v4_is_public(v4);
    }
    let s = ip.segments();
    // NAT64 (64:ff9b::/96) and 6to4 (2002::/16) embed an IPv4 address: judge that one.
    if s[0] == 0x64 && s[1] == 0xff9b && s[2..6].iter().all(|x| *x == 0) {
        let o = ip.octets();
        return v4_is_public(Ipv4Addr::new(o[12], o[13], o[14], o[15]));
    }
    if s[0] == 0x2002 {
        let o = ip.octets();
        return v4_is_public(Ipv4Addr::new(o[2], o[3], o[4], o[5]));
    }
    !(ip.is_unspecified()
        || ip.is_loopback()
        || (s[0] & 0xfe00) == 0xfc00 // fc00::/7 unique local (includes fd00:ec2::254)
        || (s[0] & 0xffc0) == 0xfe80 // fe80::/10 link-local
        || (s[0] & 0xffc0) == 0xfec0 // fec0::/10 site-local (deprecated)
        || (s[0] & 0xff00) == 0xff00 // multicast
        || (s[0] == 0x2001 && s[1] == 0x0db8) // documentation
        || (s[0] == 0x2001 && s[1] == 0) // Teredo
        || (s[0] == 0x0100 && s[1..4].iter().all(|x| *x == 0))) // discard-only
}

fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_loopback(),
        IpAddr::V6(v) => v.is_loopback(),
    }
}

/// Name resolution, replaceable in tests.
pub trait Lookup: Send + Sync {
    fn lookup(&self, host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>>;
}

pub struct SystemLookup;

impl Lookup for SystemLookup {
    fn lookup(&self, host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
        Ok((host, port).to_socket_addrs()?.collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    /// The URL is not acceptable (scheme, syntax, credentials in it).
    BadUrl(&'static str),
    /// The host is not on an allow-list.
    HostNotAllowed(String),
    /// A resolved address is not public.
    BlockedAddress(String),
    Resolve(String),
    Redirect(u16),
    TooLarge(&'static str),
    Timeout,
    Transport(String),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadUrl(why) => write!(f, "url refused: {why}"),
            Self::HostNotAllowed(h) => write!(f, "host {h} is not on the allow-list"),
            Self::BlockedAddress(h) => {
                write!(f, "host {h} resolves to an address that is not public")
            }
            Self::Resolve(h) => write!(f, "host {h} could not be resolved"),
            Self::Redirect(s) => write!(f, "the server answered {s}; redirects are not followed"),
            Self::TooLarge(what) => write!(f, "{what} is larger than the limit"),
            Self::Timeout => write!(f, "the request timed out"),
            Self::Transport(m) => write!(f, "transport error: {m}"),
        }
    }
}

impl std::error::Error for NetError {}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Allow a literal loopback host over plain http (tests, local development).
    pub allow_loopback: bool,
    pub timeout: Duration,
    pub max_request: usize,
    pub max_response: usize,
    /// Instance-wide host allow-list kept by the operator; empty means no extra restriction.
    pub instance_hosts: Vec<String>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            allow_loopback: false,
            timeout: Duration::from_secs(20),
            max_request: 1024 * 1024,
            max_response: 2 * 1024 * 1024,
            instance_hosts: Vec::new(),
        }
    }
}

pub struct Request<'a> {
    pub method: &'a str,
    pub url: &'a str,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    /// Hosts this request may reach: exact names or `*.suffix`.
    pub allowed_hosts: &'a [String],
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Clone)]
pub struct Client {
    cfg: ClientConfig,
    lookup: Arc<dyn Lookup>,
    /// The operator's host allow-list; replaceable while the server runs.
    hosts: Arc<std::sync::RwLock<Vec<String>>>,
}

/// A host matches an allow-list entry exactly, or `*.suffix` matches any subdomain of suffix.
pub fn host_matches(host: &str, pattern: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let pattern = pattern.trim().to_ascii_lowercase();
    match pattern.strip_prefix("*.") {
        Some(suffix) => host.len() > suffix.len() + 1 && host.ends_with(&format!(".{suffix}")),
        None => !pattern.is_empty() && host == pattern,
    }
}

/// The result of vetting a URL before any connection: the pinned addresses.
#[derive(Debug)]
pub struct Vetted {
    pub url: url::Url,
    pub host: String,
    pub addrs: Vec<SocketAddr>,
}

impl Client {
    pub fn new(cfg: ClientConfig) -> Self {
        Self::with_lookup(cfg, Arc::new(SystemLookup))
    }

    pub fn with_lookup(cfg: ClientConfig, lookup: Arc<dyn Lookup>) -> Self {
        let hosts = Arc::new(std::sync::RwLock::new(cfg.instance_hosts.clone()));
        Self { cfg, lookup, hosts }
    }

    /// Replace the operator's host allow-list (empty means no extra restriction).
    pub fn set_instance_hosts(&self, hosts: Vec<String>) {
        *self.hosts.write().unwrap() = hosts;
    }

    pub fn config(&self) -> &ClientConfig {
        &self.cfg
    }

    /// The checks that need no network: syntax, no credentials in the URL, the scheme rule, and
    /// the host against the allow-lists. Returns the lowercase host.
    pub fn check_shape(
        &self,
        url: &str,
        allowed_hosts: &[String],
    ) -> Result<(url::Url, String), NetError> {
        let u = url::Url::parse(url).map_err(|_| NetError::BadUrl("not a valid URL"))?;
        if !u.username().is_empty() || u.password().is_some() {
            return Err(NetError::BadUrl("credentials in the URL are not accepted"));
        }
        let host = u
            .host_str()
            .ok_or(NetError::BadUrl("no host"))?
            .trim_matches(['[', ']'])
            .to_ascii_lowercase();
        let literal: Option<IpAddr> = host.parse().ok();
        match u.scheme() {
            "https" => {}
            "http" if self.cfg.allow_loopback && literal.is_some_and(is_loopback) => {}
            _ => return Err(NetError::BadUrl("only https is accepted")),
        }
        if allowed_hosts.is_empty() || !allowed_hosts.iter().any(|p| host_matches(&host, p)) {
            return Err(NetError::HostNotAllowed(host));
        }
        let operator = self.hosts.read().unwrap();
        if !operator.is_empty() && !operator.iter().any(|p| host_matches(&host, p)) {
            return Err(NetError::HostNotAllowed(host));
        }
        Ok((u, host))
    }

    /// Check a URL against the rules and resolve it once. Used by `send` and by the connection
    /// test, which reports why a URL would be refused without sending anything.
    pub fn vet(&self, url: &str, allowed_hosts: &[String]) -> Result<Vetted, NetError> {
        let (u, host) = self.check_shape(url, allowed_hosts)?;
        let literal: Option<IpAddr> = host.parse().ok();
        let port = u
            .port_or_known_default()
            .ok_or(NetError::BadUrl("no port"))?;
        let addrs: Vec<SocketAddr> = match literal {
            Some(ip) => vec![SocketAddr::new(ip, port)],
            None => self
                .lookup
                .lookup(&host, port)
                .map_err(|_| NetError::Resolve(host.clone()))?,
        };
        if addrs.is_empty() {
            return Err(NetError::Resolve(host));
        }
        for a in &addrs {
            let ok = ip_is_public(a.ip()) || (self.cfg.allow_loopback && is_loopback(a.ip()));
            if !ok {
                return Err(NetError::BlockedAddress(host));
            }
        }
        Ok(Vetted {
            url: u,
            host,
            addrs,
        })
    }

    pub fn send(&self, req: Request) -> Result<Response, NetError> {
        if req
            .body
            .as_ref()
            .is_some_and(|b| b.len() > self.cfg.max_request)
        {
            return Err(NetError::TooLarge("the request body"));
        }
        let v = self.vet(req.url, req.allowed_hosts)?;
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .https_only(false)
            .timeout_global(Some(self.cfg.timeout))
            .user_agent(USER_AGENT)
            .build();
        let agent = ureq::Agent::with_parts(
            config,
            ureq::unversioned::transport::DefaultConnector::default(),
            Pinned(v.addrs.clone()),
        );
        let mut b = ureq::http::Request::builder()
            .method(req.method)
            .uri(v.url.as_str());
        for (k, val) in &req.headers {
            if val.contains(['\r', '\n']) || k.contains(['\r', '\n', ':']) {
                return Err(NetError::BadUrl("header contains a line break"));
            }
            b = b.header(k.as_str(), val.as_str());
        }
        let request = b
            .body(req.body.unwrap_or_default())
            .map_err(|_| NetError::BadUrl("request could not be built"))?;
        let mut resp = agent.run(request).map_err(map_err)?;
        let status = resp.status().as_u16();
        if (300..400).contains(&status) {
            return Err(NetError::Redirect(status));
        }
        let headers = resp
            .headers()
            .iter()
            .filter_map(|(k, v)| Some((k.to_string(), v.to_str().ok()?.to_string())))
            .collect();
        let body = resp
            .body_mut()
            .with_config()
            .limit(self.cfg.max_response as u64)
            .read_to_vec()
            .map_err(|e| match e {
                ureq::Error::BodyExceedsLimit(_) => NetError::TooLarge("the response"),
                other => map_err(other),
            })?;
        Ok(Response {
            status,
            headers,
            body,
        })
    }
}

fn map_err(e: ureq::Error) -> NetError {
    match e {
        ureq::Error::Timeout(_) => NetError::Timeout,
        ureq::Error::BodyExceedsLimit(_) => NetError::TooLarge("the response"),
        // The text of a transport error can carry the URL; keep only the kind.
        other => NetError::Transport(
            format!("{other:?}")
                .split(['(', '{', ' '])
                .next()
                .unwrap_or("error")
                .to_string(),
        ),
    }
}

/// A resolver that hands out only the addresses vetted for this request.
#[derive(Debug)]
struct Pinned(Vec<SocketAddr>);

impl ureq::unversioned::resolver::Resolver for Pinned {
    fn resolve(
        &self,
        _uri: &ureq::http::Uri,
        _config: &ureq::config::Config,
        _timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
        let mut out = self.empty();
        for a in self.0.iter().take(16) {
            out.push(*a);
        }
        if out.is_empty() {
            return Err(ureq::Error::HostNotFound);
        }
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod testserver {
    //! A tiny one-shot HTTP server on loopback for tests: it answers each connection with the
    //! canned reply and records the raw request.
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    pub struct Server {
        pub port: u16,
        pub requests: Arc<Mutex<Vec<String>>>,
    }

    pub fn serve(reply: impl Fn(&str) -> String + Send + Sync + 'static) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { break };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                // Read headers, then as much body as Content-Length says.
                loop {
                    let n = s.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(h) = text.find("\r\n\r\n") {
                        let len = text[..h]
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if buf.len() >= h + 4 + len {
                            break;
                        }
                    }
                }
                let req = String::from_utf8_lossy(&buf).to_string();
                let out = reply(&req);
                seen.lock().unwrap().push(req);
                let _ = s.write_all(out.as_bytes());
            }
        });
        Server { port, requests }
    }

    pub fn ok(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::testserver::*;
    use super::*;
    use std::sync::Mutex;

    struct Fake(Mutex<Vec<Vec<SocketAddr>>>, Mutex<usize>);
    impl Lookup for Fake {
        fn lookup(&self, _: &str, _: u16) -> std::io::Result<Vec<SocketAddr>> {
            *self.1.lock().unwrap() += 1;
            let mut q = self.0.lock().unwrap();
            Ok(if q.len() > 1 {
                q.remove(0)
            } else {
                q[0].clone()
            })
        }
    }

    fn sa(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    fn hosts() -> Vec<String> {
        vec!["hooks.example.com".into(), "*.example.org".into()]
    }

    #[test]
    fn private_and_special_ranges_are_not_public() {
        for ip in [
            "0.0.0.0",
            "10.1.2.3",
            "127.0.0.1",
            "127.255.255.254",
            "100.64.0.1",
            "169.254.169.254",
            "172.16.0.1",
            "172.31.255.255",
            "192.0.0.1",
            "192.0.2.5",
            "192.168.1.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.9",
            "224.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "fe80::1",
            "fc00::1",
            "fd00:ec2::254",
            "ff02::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:169.254.169.254",
            "64:ff9b::7f00:1",
            "2002:7f00:1::1",
            "fec0::1",
        ] {
            assert!(!ip_is_public(ip.parse().unwrap()), "{ip} must be refused");
        }
        for ip in [
            "8.8.8.8",
            "93.184.216.34",
            "172.32.0.1",
            "100.63.0.1",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            assert!(ip_is_public(ip.parse().unwrap()), "{ip} must be allowed");
        }
    }

    #[test]
    fn schemes_credentials_and_hosts_are_vetted() {
        let c = Client::with_lookup(
            ClientConfig::default(),
            Arc::new(Fake(
                Mutex::new(vec![vec![sa("93.184.216.34:443")]]),
                Mutex::new(0),
            )),
        );
        let h = hosts();
        assert!(c.vet("https://hooks.example.com/x", &h).is_ok());
        assert!(c.vet("https://a.example.org/x", &h).is_ok());
        for (url, want) in [
            (
                "http://hooks.example.com/x",
                NetError::BadUrl("only https is accepted"),
            ),
            (
                "ftp://hooks.example.com/x",
                NetError::BadUrl("only https is accepted"),
            ),
            ("file:///etc/passwd", NetError::BadUrl("no host")),
            (
                "https://user:pw@hooks.example.com/x",
                NetError::BadUrl("credentials in the URL are not accepted"),
            ),
            (
                "https://evil.com/x",
                NetError::HostNotAllowed("evil.com".into()),
            ),
            (
                "https://example.org/x",
                NetError::HostNotAllowed("example.org".into()),
            ),
            (
                "https://hooks.example.com.evil.com/x",
                NetError::HostNotAllowed("hooks.example.com.evil.com".into()),
            ),
        ] {
            assert_eq!(c.vet(url, &h).unwrap_err(), want, "{url}");
        }
        // No allow-list means nothing is reachable.
        assert!(matches!(
            c.vet("https://hooks.example.com/x", &[]),
            Err(NetError::HostNotAllowed(_))
        ));
    }

    #[test]
    fn literal_addresses_in_odd_notations_are_judged_after_parsing() {
        let c = Client::new(ClientConfig::default());
        let any = vec!["*".to_string()];
        // The allow-list matches by name, so a literal needs to be listed to get as far as the range check.
        for lit in [
            "127.0.0.1",
            "2130706433",
            "0x7f.1",
            "[::1]",
            "169.254.169.254",
            "[::ffff:7f00:1]",
        ] {
            let url = format!("https://{lit}/");
            let host = url::Url::parse(&url)
                .unwrap()
                .host_str()
                .unwrap()
                .trim_matches(['[', ']'])
                .to_string();
            let e = c.vet(&url, &[host]).unwrap_err();
            assert!(matches!(e, NetError::BlockedAddress(_)), "{lit}: {e:?}");
        }
        assert!(c.vet("https://127.0.0.1/", &any).is_err());
    }

    #[test]
    fn dns_answers_are_all_checked_and_resolved_once() {
        let h = hosts();
        // A public answer mixed with a private one refuses the request.
        let mixed = Fake(
            Mutex::new(vec![vec![sa("93.184.216.34:443"), sa("10.0.0.7:443")]]),
            Mutex::new(0),
        );
        let c = Client::with_lookup(ClientConfig::default(), Arc::new(mixed));
        assert!(matches!(
            c.vet("https://hooks.example.com/", &h),
            Err(NetError::BlockedAddress(_))
        ));
        // Rebinding: the first answer is public, the next one private. One vetting resolves once
        // and pins that answer; a second vetting sees the private one and is refused.
        let rebind = Arc::new(Fake(
            Mutex::new(vec![
                vec![sa("93.184.216.34:443")],
                vec![sa("127.0.0.1:443")],
            ]),
            Mutex::new(0),
        ));
        let c = Client::with_lookup(ClientConfig::default(), rebind.clone());
        let v = c.vet("https://hooks.example.com/", &h).unwrap();
        assert_eq!(v.addrs, vec![sa("93.184.216.34:443")]);
        assert_eq!(*rebind.1.lock().unwrap(), 1, "one lookup per request");
        assert!(matches!(
            c.vet("https://hooks.example.com/", &h),
            Err(NetError::BlockedAddress(_))
        ));
        // A name that does not resolve.
        struct None_;
        impl Lookup for None_ {
            fn lookup(&self, _: &str, _: u16) -> std::io::Result<Vec<SocketAddr>> {
                Ok(vec![])
            }
        }
        let c = Client::with_lookup(ClientConfig::default(), Arc::new(None_));
        assert!(matches!(
            c.vet("https://hooks.example.com/", &h),
            Err(NetError::Resolve(_))
        ));
    }

    #[test]
    fn loopback_needs_the_operator_switch_and_only_allows_loopback() {
        let h = vec!["127.0.0.1".to_string()];
        let off = Client::new(ClientConfig::default());
        assert!(off.vet("http://127.0.0.1:9/", &h).is_err());
        let on = Client::new(ClientConfig {
            allow_loopback: true,
            ..ClientConfig::default()
        });
        assert!(on.vet("http://127.0.0.1:9/", &h).is_ok());
        // Plain http to a name is still refused, and other private ranges stay blocked.
        assert!(
            on.vet("http://localhost:9/", &["localhost".to_string()])
                .is_err()
        );
        assert!(
            on.vet("http://10.0.0.1:9/", &["10.0.0.1".to_string()])
                .is_err()
        );
        assert!(
            on.vet("http://169.254.169.254/", &["169.254.169.254".to_string()])
                .is_err()
        );
    }

    fn local() -> Client {
        Client::new(ClientConfig {
            allow_loopback: true,
            timeout: Duration::from_secs(5),
            ..ClientConfig::default()
        })
    }

    #[test]
    fn sends_and_reads_a_response_and_never_follows_redirects() {
        let s = serve(|req| {
            if req.starts_with("POST /redirect") {
                "HTTP/1.1 302 Found\r\nlocation: http://169.254.169.254/latest\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".into()
            } else {
                ok("{\"ok\":true}")
            }
        });
        let c = local();
        let allowed = vec!["127.0.0.1".to_string()];
        let r = c
            .send(Request {
                method: "POST",
                url: &format!("http://127.0.0.1:{}/hook?token=abc", s.port),
                headers: vec![("content-type".into(), "application/json".into())],
                body: Some(b"{\"a\":1}".to_vec()),
                allowed_hosts: &allowed,
            })
            .unwrap();
        assert_eq!((r.status, r.body.as_slice()), (200, &b"{\"ok\":true}"[..]));
        let seen = s.requests.lock().unwrap()[0].clone();
        assert!(seen.contains("obop-connectors/") && seen.ends_with("{\"a\":1}"));
        let e = c
            .send(Request {
                method: "POST",
                url: &format!("http://127.0.0.1:{}/redirect", s.port),
                headers: vec![],
                body: Some(vec![]),
                allowed_hosts: &allowed,
            })
            .unwrap_err();
        assert_eq!(e, NetError::Redirect(302));
        assert_eq!(
            s.requests.lock().unwrap().len(),
            2,
            "the redirect target was never contacted"
        );
    }

    #[test]
    fn size_caps_and_header_injection() {
        let big = "x".repeat(5000);
        let s = serve(move |_| ok(&big));
        let c = Client::new(ClientConfig {
            allow_loopback: true,
            max_response: 1000,
            max_request: 10,
            ..ClientConfig::default()
        });
        let allowed = vec!["127.0.0.1".to_string()];
        let url = format!("http://127.0.0.1:{}/", s.port);
        let e = c
            .send(Request {
                method: "GET",
                url: &url,
                headers: vec![],
                body: None,
                allowed_hosts: &allowed,
            })
            .unwrap_err();
        assert!(matches!(e, NetError::TooLarge(_)), "{e:?}");
        let e = c
            .send(Request {
                method: "POST",
                url: &url,
                headers: vec![],
                body: Some(vec![0; 11]),
                allowed_hosts: &allowed,
            })
            .unwrap_err();
        assert!(matches!(e, NetError::TooLarge(_)));
        let e = c
            .send(Request {
                method: "GET",
                url: &url,
                headers: vec![("x".into(), "a\r\nb: c".into())],
                body: None,
                allowed_hosts: &allowed,
            })
            .unwrap_err();
        assert!(matches!(e, NetError::BadUrl(_)));
    }

    #[test]
    fn errors_never_contain_the_url_query_or_secrets() {
        let c = local();
        let allowed = vec!["127.0.0.1".to_string()];
        // Nothing listens on port 1.
        let e = c
            .send(Request {
                method: "GET",
                url: "http://127.0.0.1:1/path?token=SECRET123",
                headers: vec![("authorization".into(), "Bearer SECRET456".into())],
                body: None,
                allowed_hosts: &allowed,
            })
            .unwrap_err();
        let text = format!("{e} {e:?}");
        assert!(!text.contains("SECRET"), "{text}");
    }

    #[test]
    fn host_patterns() {
        assert!(host_matches("a.example.com", "*.example.com"));
        assert!(!host_matches("example.com", "*.example.com"));
        assert!(!host_matches("badexample.com", "*.example.com"));
        assert!(host_matches("EXAMPLE.com", "example.COM"));
        assert!(!host_matches("x", ""));
    }
}
