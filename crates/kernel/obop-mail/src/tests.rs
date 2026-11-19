//! The mail module against stand-ins on loopback: an SMTP server (plain, STARTTLS and implicit TLS
//! with a certificate made for the run) and a fake HTTP provider. Nothing leaves the machine.

use super::*;
use obop_commands::{Principal, Surface, Vault};
use obop_connectors::{Config as RtConfig, SecretBox};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};

// ---- a certificate made for the run --------------------------------------------------------------

/// DER: a tag, the length, the content.
fn tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let n = content.len();
    if n < 0x80 {
        out.push(n as u8);
    } else if n < 0x100 {
        out.extend_from_slice(&[0x81, n as u8]);
    } else {
        out.extend_from_slice(&[0x82, (n >> 8) as u8, n as u8]);
    }
    out.extend_from_slice(content);
    out
}

fn seq(parts: &[Vec<u8>]) -> Vec<u8> {
    tlv(0x30, &parts.concat())
}

/// A self-signed P-256 certificate for `localhost` and 127.0.0.1 and its PKCS#8 key, made once per
/// test process with `ring`, so that no key is kept in the repository.
fn test_cert() -> &'static (Vec<u8>, Vec<u8>) {
    static CERT: std::sync::OnceLock<(Vec<u8>, Vec<u8>)> = std::sync::OnceLock::new();
    CERT.get_or_init(|| {
        use ring::signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, KeyPair};
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
        let key = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng)
            .unwrap();
        let ecdsa_sha256 = seq(&[tlv(0x06, &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x02])]);
        let name = seq(&[tlv(
            0x31,
            &seq(&[tlv(0x06, &[0x55, 0x04, 0x03]), tlv(0x0c, b"localhost")]),
        )]);
        let spki = seq(&[
            seq(&[
                tlv(0x06, &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01]),
                tlv(0x06, &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07]),
            ]),
            tlv(0x03, &[&[0u8][..], key.public_key().as_ref()].concat()),
        ]);
        let san = seq(&[
            tlv(0x06, &[0x55, 0x1d, 0x11]),
            tlv(
                0x04,
                &seq(&[tlv(0x82, b"localhost"), tlv(0x87, &[127, 0, 0, 1])]),
            ),
        ]);
        let basic = seq(&[
            tlv(0x06, &[0x55, 0x1d, 0x13]),
            tlv(0x01, &[0xff]),
            tlv(0x04, &seq(&[])),
        ]);
        let tbs = seq(&[
            tlv(0xa0, &tlv(0x02, &[2])),
            tlv(0x02, &[0x01, 0x02, 0x03, 0x04]),
            ecdsa_sha256.clone(),
            name.clone(),
            seq(&[tlv(0x17, b"200101000000Z"), tlv(0x18, b"20991231235959Z")]),
            name,
            spki,
            tlv(0xa3, &seq(&[san, basic])),
        ]);
        let sig = key.sign(&rng, &tbs).unwrap();
        let cert = seq(&[
            tbs,
            ecdsa_sha256,
            tlv(0x03, &[&[0u8][..], sig.as_ref()].concat()),
        ]);
        (cert, pkcs8.as_ref().to_vec())
    })
}
const PASSWORD: &str = "relay-password-test-0001";

// ---- the SMTP stand-in -------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Plain,
    StartTls,
    ImplicitTls,
}

#[derive(Clone)]
struct Script {
    mode: Mode,
    /// Offer STARTTLS in EHLO.
    offer_starttls: bool,
    /// Code for RCPT TO.
    rcpt: u16,
    /// Code after the end of DATA.
    data_end: u16,
}

impl Default for Script {
    fn default() -> Self {
        Self {
            mode: Mode::Plain,
            offer_starttls: false,
            rcpt: 250,
            data_end: 250,
        }
    }
}

#[derive(Default)]
struct Seen {
    /// Every command line, with ` [tls]` when it came over TLS.
    lines: Vec<String>,
    data: Vec<String>,
}

struct Smtp {
    port: u16,
    seen: Arc<Mutex<Seen>>,
}

fn server_tls() -> Arc<rustls::ServerConfig> {
    let (cert, pkcs8) = test_cert();
    let certs = vec![CertificateDer::from(cert.clone())];
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(pkcs8.clone()));
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    Arc::new(
        rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .unwrap(),
    )
}

trait Rw: Read + Write {}
impl<T: Read + Write> Rw for T {}

fn serve_one(tcp: TcpStream, script: &Script, seen: &Arc<Mutex<Seen>>) {
    let mut tls = false;
    let mut stream: Box<dyn Rw> = if script.mode == Mode::ImplicitTls {
        tls = true;
        let conn = rustls::ServerConnection::new(server_tls()).unwrap();
        Box::new(rustls::StreamOwned::new(conn, tcp.try_clone().unwrap()))
    } else {
        Box::new(tcp.try_clone().unwrap())
    };
    let say = |s: &mut Box<dyn Rw>, t: &str| {
        let _ = s.write_all(format!("{t}\r\n").as_bytes());
        let _ = s.flush();
    };
    say(&mut stream, "220 stand-in ready");
    let mut in_data = false;
    let mut data = String::new();
    loop {
        let mut line = Vec::new();
        let mut b = [0u8; 1];
        loop {
            match stream.read(&mut b) {
                Ok(1) => {
                    line.push(b[0]);
                    if b[0] == b'\n' {
                        break;
                    }
                }
                _ => return,
            }
        }
        let l = String::from_utf8_lossy(&line).trim_end().to_string();
        if in_data {
            if l == "." {
                in_data = false;
                seen.lock().unwrap().data.push(std::mem::take(&mut data));
                let code = script.data_end;
                say(
                    &mut stream,
                    &format!(
                        "{code} {}",
                        if code == 250 {
                            "2.0.0 Ok: queued as ABC123"
                        } else {
                            "no"
                        }
                    ),
                );
            } else {
                data.push_str(&l);
                data.push('\n');
            }
            continue;
        }
        seen.lock()
            .unwrap()
            .lines
            .push(format!("{l}{}", if tls { " [tls]" } else { "" }));
        let up = l.to_ascii_uppercase();
        if up.starts_with("EHLO") {
            let mut r = vec!["250-stand-in".to_string()];
            if script.offer_starttls && !tls {
                r.push("250-STARTTLS".into());
            }
            r.push("250 AUTH PLAIN LOGIN".into());
            say(&mut stream, &r.join("\r\n"));
        } else if up == "STARTTLS" {
            say(&mut stream, "220 go ahead");
            let conn = rustls::ServerConnection::new(server_tls()).unwrap();
            stream = Box::new(rustls::StreamOwned::new(conn, tcp.try_clone().unwrap()));
            tls = true;
        } else if up.starts_with("AUTH PLAIN ") {
            let token = &l[11..];
            let raw = base64::engine::general_purpose::STANDARD
                .decode(token)
                .unwrap();
            let ok = raw == format!("\0relay-user\0{PASSWORD}").as_bytes();
            say(
                &mut stream,
                if ok {
                    "235 ok"
                } else {
                    "535 5.7.8 bad credentials"
                },
            );
        } else if up.starts_with("MAIL FROM") {
            say(&mut stream, "250 ok");
        } else if up.starts_with("RCPT TO") {
            let c = script.rcpt;
            say(
                &mut stream,
                &format!("{c} {}", if c < 300 { "ok" } else { "recipient refused" }),
            );
        } else if up == "DATA" {
            in_data = true;
            say(&mut stream, "354 end with .");
        } else if up == "QUIT" {
            say(&mut stream, "221 bye");
            return;
        } else {
            say(&mut stream, "500 what");
        }
    }
}

fn smtp(script: Script) -> Smtp {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Seen::default()));
    let s2 = seen.clone();
    std::thread::spawn(move || {
        for c in listener.incoming() {
            let Ok(c) = c else { break };
            serve_one(c, &script, &s2);
        }
    });
    Smtp { port, seen }
}

use base64::Engine as _;

// ---- the rig ----------------------------------------------------------------------------------

struct Rig {
    d: Arc<Dispatcher>,
    mail: Arc<Mail>,
    clock: Arc<AtomicU64>,
}

const T0: u64 = 1_795_000_000;

fn rig_with(allow_loopback: bool) -> Rig {
    let store = Store::new();
    let d = Arc::new(Dispatcher::new());
    let rt = Runtime::with_providers(
        store.backend(),
        d.clone(),
        SecretBox::for_tests(),
        RtConfig {
            allow_loopback,
            public_url: None,
        },
        obop_connectors::catalog::providers(),
        None,
    );
    let clock = Arc::new(AtomicU64::new(T0));
    let c2 = clock.clone();
    let mail = Mail::with_clock(
        rt,
        Config::default(),
        Arc::new(move || c2.load(Ordering::SeqCst)),
    );
    mail.trust_certificate(test_cert().0.clone());
    mail.register(&d, &store);
    Rig { d, mail, clock }
}

fn rig() -> Rig {
    rig_with(true)
}

fn admin() -> Principal {
    Principal::user("ada", &[ADMIN, SENDER])
}

fn v(pairs: &[(&str, J)]) -> Value {
    Value::from_json(&J::Object(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    ))
}

use obop_commands::Value;

impl Rig {
    fn call(&self, p: &Principal, id: &str, input: Value) -> Result<J, String> {
        let mut vault = Vault::new();
        self.d
            .call(Surface::Rest, p, &mut vault, id, &input)
            .map(|r| r.value.to_json())
            .map_err(|e| format!("{e:?}"))
    }

    fn server(&self, extra: &[(&str, J)]) -> String {
        let mut f: Vec<(&str, J)> = vec![
            ("name", "Relay".into()),
            ("kind", "smtp".into()),
            ("enabled", true.into()),
            ("is_default", true.into()),
            ("host", "127.0.0.1".into()),
            ("security", "none".into()),
            ("from_address", "rechnung@nordlicht.example".into()),
            ("from_name", "Nordlicht Werkzeuge GmbH".into()),
        ];
        for (k, val) in extra {
            f.retain(|(x, _)| x != k);
            f.push((k, val.clone()));
        }
        let out = self.call(&admin(), "mail_server.create", v(&f)).unwrap();
        out["id"].as_str().unwrap().to_string()
    }

    fn send(&self, extra: &[(&str, J)]) -> Result<String, String> {
        let mut f: Vec<(&str, J)> = vec![
            ("to", "kunde@example.org".into()),
            ("subject", "Ihre Rechnung".into()),
            ("body", "Guten Tag,\n.anbei die Rechnung.".into()),
        ];
        for (k, val) in extra {
            f.retain(|(x, _)| x != k);
            f.push((k, val.clone()));
        }
        self.call(&admin(), "outbound_mail.send", v(&f))
            .map(|o| o["id"].as_str().unwrap().to_string())
    }

    fn msg(&self, id: &str) -> Fields {
        self.mail.backend.get(OUTBOUND, id).unwrap().unwrap()
    }

    fn at(&self, secs: u64) {
        self.clock.store(secs, Ordering::SeqCst);
    }
}

fn decoded_body(data: &str) -> String {
    let (_, b) = data.split_once("\n\n").unwrap();
    String::from_utf8(
        base64::engine::general_purpose::STANDARD
            .decode(b.replace('\n', ""))
            .unwrap(),
    )
    .unwrap()
}

// ---- SMTP --------------------------------------------------------------------------------------

#[test]
fn a_queued_message_goes_out_through_the_relay_and_its_body_is_erased() {
    let r = rig();
    let s = smtp(Script::default());
    let srv = r.server(&[
        ("port", J::from(s.port)),
        ("footer", "Nordlicht Werkzeuge GmbH, Kiel".into()),
    ]);
    let id = r
        .send(&[("bcc", "archiv@nordlicht.example".into())])
        .unwrap();
    let m = r.msg(&id);
    assert_eq!(text(&m, "status"), "queued");
    assert_eq!(text(&m, "server"), srv);
    assert!(
        !text(&m, "body").contains("Rechnung"),
        "the body is stored sealed"
    );
    let run = r.mail.process(T0).unwrap();
    assert_eq!(
        run,
        Run {
            sent: 1,
            retrying: 0,
            failed: 0
        }
    );
    let m = r.msg(&id);
    assert_eq!(text(&m, "status"), "sent", "{m:?}");
    assert_eq!(text(&m, "body"), "", "the body is erased once sent");
    assert!(text(&m, "server_reply").contains("ABC123"));
    let seen = s.seen.lock().unwrap();
    assert!(
        seen.lines
            .iter()
            .any(|l| l == "MAIL FROM:<rechnung@nordlicht.example>")
    );
    assert!(
        seen.lines
            .iter()
            .any(|l| l == "RCPT TO:<archiv@nordlicht.example>"),
        "bcc is an envelope recipient"
    );
    let data = &seen.data[0];
    assert!(!data.contains("archiv@"), "bcc is never a header");
    assert!(data.contains(&format!("Message-ID: <{id}@nordlicht.example>")));
    let body = decoded_body(data);
    assert!(body.starts_with("Guten Tag,\r\n.anbei"), "{body}");
    assert!(
        body.ends_with("-- \r\nNordlicht Werkzeuge GmbH, Kiel"),
        "{body}"
    );
    // Nothing is due any more.
    assert_eq!(r.mail.process(T0 + 10).unwrap(), Run::default());
}

#[test]
fn starttls_then_auth_and_the_password_only_ever_travels_over_tls() {
    let r = rig();
    let s = smtp(Script {
        mode: Mode::StartTls,
        offer_starttls: true,
        ..Script::default()
    });
    r.server(&[
        ("port", J::from(s.port)),
        ("host", "localhost".into()),
        ("security", "starttls".into()),
        ("username", "relay-user".into()),
        ("password", PASSWORD.into()),
    ]);
    let id = r.send(&[]).unwrap();
    r.mail.process(T0).unwrap();
    assert_eq!(text(&r.msg(&id), "status"), "sent", "{:?}", r.msg(&id));
    let seen = s.seen.lock().unwrap();
    let auth = seen.lines.iter().find(|l| l.starts_with("AUTH")).unwrap();
    assert!(
        auth.ends_with("[tls]"),
        "AUTH came before TLS: {:?}",
        seen.lines
    );
    assert!(seen.lines.iter().any(|l| l == "STARTTLS"));
}

#[test]
fn implicit_tls_works_and_a_wrong_password_fails_without_retrying() {
    let r = rig();
    let s = smtp(Script {
        mode: Mode::ImplicitTls,
        ..Script::default()
    });
    let srv = r.server(&[
        ("port", J::from(s.port)),
        ("security", "tls".into()),
        ("username", "relay-user".into()),
        ("password", PASSWORD.into()),
    ]);
    let id = r.send(&[]).unwrap();
    r.mail.process(T0).unwrap();
    assert_eq!(text(&r.msg(&id), "status"), "sent", "{:?}", r.msg(&id));
    r.call(
        &admin(),
        "mail_server.update",
        v(&[("id", srv.clone().into()), ("password", "wrong".into())]),
    )
    .unwrap();
    let id = r.send(&[]).unwrap();
    r.mail.process(T0).unwrap();
    let m = r.msg(&id);
    assert_eq!(text(&m, "status"), "failed");
    assert_eq!(
        text(&m, "last_error"),
        "the server refused the user name or password"
    );
    assert!(!format!("{m:?}").contains("wrong"));
}

#[test]
fn starttls_is_never_given_up_for_plain_text() {
    let r = rig();
    let s = smtp(Script::default());
    r.server(&[
        ("port", J::from(s.port)),
        ("security", "starttls".into()),
        ("username", "relay-user".into()),
        ("password", PASSWORD.into()),
    ]);
    let id = r.send(&[]).unwrap();
    r.mail.process(T0).unwrap();
    let m = r.msg(&id);
    assert_eq!(text(&m, "status"), "failed");
    assert!(text(&m, "last_error").contains("does not offer STARTTLS"));
    assert!(
        !s.seen
            .lock()
            .unwrap()
            .lines
            .iter()
            .any(|l| l.starts_with("AUTH"))
    );
}

#[test]
fn a_temporary_refusal_is_retried_with_backoff_and_fails_after_the_last_attempt() {
    let r = rig();
    let s = smtp(Script {
        data_end: 451,
        ..Script::default()
    });
    r.server(&[("port", J::from(s.port))]);
    let id = r.send(&[]).unwrap();
    let mut t = T0;
    for (n, wait) in BACKOFF_SECS.iter().enumerate() {
        r.at(t);
        let run = r.mail.process(t).unwrap();
        assert_eq!(run.retrying, 1, "attempt {}", n + 1);
        let m = r.msg(&id);
        assert_eq!(text(&m, "status"), "queued");
        assert_eq!(num(&m, "attempts"), n as u64 + 1);
        assert!(text(&m, "last_error").contains("451"));
        // Not due a second earlier.
        assert_eq!(r.mail.process(t + wait - 1).unwrap(), Run::default());
        t += wait;
    }
    r.at(t);
    assert_eq!(r.mail.process(t).unwrap().failed, 1);
    let m = r.msg(&id);
    assert_eq!(text(&m, "status"), "failed");
    assert_eq!(num(&m, "attempts"), 6);
    // An administrator sends it again; the text was kept for that.
    r.call(
        &admin(),
        "outbound_mail.retry",
        v(&[("id", id.clone().into())]),
    )
    .unwrap();
    assert_eq!(text(&r.msg(&id), "status"), "queued");
    assert_eq!(num(&r.msg(&id), "attempts"), 0);
}

#[test]
fn a_refused_recipient_fails_at_once() {
    let r = rig();
    let s = smtp(Script {
        rcpt: 550,
        ..Script::default()
    });
    r.server(&[("port", J::from(s.port))]);
    let id = r.send(&[]).unwrap();
    assert_eq!(r.mail.process(T0).unwrap().failed, 1);
    let m = r.msg(&id);
    assert!(text(&m, "last_error").contains("550"), "{m:?}");
    assert_eq!(num(&m, "attempts"), 1);
}

#[test]
fn a_loopback_relay_is_refused_unless_the_test_setting_allows_it() {
    let r = rig_with(false);
    let s = smtp(Script::default());
    r.server(&[("port", J::from(s.port))]);
    let id = r.send(&[]).unwrap();
    r.mail.process(T0).unwrap();
    let m = r.msg(&id);
    assert_eq!(text(&m, "status"), "failed");
    assert!(
        s.seen.lock().unwrap().lines.is_empty(),
        "nothing reached the relay"
    );
}

#[test]
fn a_message_claimed_by_a_live_worker_is_not_taken_twice() {
    let r = rig();
    let s = smtp(Script::default());
    r.server(&[("port", J::from(s.port))]);
    let id = r.send(&[]).unwrap();
    // Another process claimed it a moment ago.
    let mut p = Fields::new();
    p.insert("status".into(), "sending".into());
    p.insert("lease_until".into(), (T0 + LEASE_SECS).into());
    r.mail.backend.update(OUTBOUND, &id, &p).unwrap();
    assert_eq!(r.mail.process(T0 + 1).unwrap(), Run::default());
    // It died: after the lease another worker takes it.
    assert_eq!(r.mail.process(T0 + LEASE_SECS + 1).unwrap().sent, 1);
    assert_eq!(s.seen.lock().unwrap().data.len(), 1);
}

#[test]
fn the_test_message_is_sent_now_and_recorded_on_the_server_and_in_the_log() {
    let r = rig();
    let s = smtp(Script::default());
    let srv = r.server(&[("port", J::from(s.port))]);
    let out = r
        .call(
            &admin(),
            "mail_server.test_send",
            v(&[
                ("id", srv.clone().into()),
                ("test_recipient", "ada@example.org".into()),
            ]),
        )
        .unwrap();
    assert!(
        out["last_test_result"]
            .as_str()
            .unwrap()
            .starts_with("sent")
    );
    let f = r.mail.backend.get(SERVER, &srv).unwrap().unwrap();
    assert!(text(&f, "last_test_result").starts_with("sent"));
    assert!(
        f.get("test_recipient").is_none(),
        "the recipient of a test is not stored"
    );
    let log = r
        .mail
        .backend
        .find(OUTBOUND, "kind", &J::from("test"))
        .unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(text(&log[0].1, "status"), "sent");
    assert_eq!(s.seen.lock().unwrap().data.len(), 1);
}

// ---- the commands' rules -------------------------------------------------------------------------

#[test]
fn the_password_is_sealed_and_never_returned() {
    let r = rig();
    let srv = r.server(&[
        ("security", "tls".into()),
        ("username", "relay-user".into()),
        ("password", PASSWORD.into()),
    ]);
    let stored = r.mail.backend.get(SERVER, &srv).unwrap().unwrap();
    assert!(!text(&stored, "password").is_empty());
    assert!(!format!("{stored:?}").contains(PASSWORD));
    assert_eq!(stored.get("password_set"), Some(&J::Bool(true)));
    let got = r
        .call(
            &admin(),
            "mail_server.get",
            v(&[("id", srv.clone().into())]),
        )
        .unwrap();
    assert!(!got.to_string().contains(PASSWORD));
    assert!(
        got.get("password").is_none_or(|p| p.is_null() || p == ""),
        "{got}"
    );
    // An empty password in an update keeps the stored one.
    r.call(
        &admin(),
        "mail_server.update",
        v(&[("id", srv.clone().into()), ("password", "".into())]),
    )
    .unwrap();
    let again = r.mail.backend.get(SERVER, &srv).unwrap().unwrap();
    assert_eq!(text(&again, "password"), text(&stored, "password"));
}

#[test]
fn settings_are_checked_and_a_password_without_tls_is_refused() {
    let r = rig();
    let base = |extra: &[(&str, J)]| {
        let mut f: Vec<(&str, J)> = vec![
            ("name", "Relay".into()),
            ("kind", "smtp".into()),
            ("host", "smtp.example.com".into()),
            ("security", "none".into()),
            ("from_address", "a@example.com".into()),
        ];
        f.extend(extra.iter().cloned());
        r.call(&admin(), "mail_server.create", v(&f))
    };
    let e = base(&[("username", "u".into()), ("password", "p".into())]).unwrap_err();
    assert!(e.contains("never sent without TLS"), "{e}");
    assert!(base(&[("from_address", "not an address".into())]).is_err());
    assert!(base(&[("reply_to", "x@y.com\r\nBcc: z@evil.test".into())]).is_err());
    assert!(base(&[("port", J::from(70000))]).is_err());
    assert!(base(&[("kinds", "Invoice!".into())]).is_err());
    let e = r
        .call(
            &admin(),
            "mail_server.create",
            v(&[
                ("name", "P".into()),
                ("kind", "provider".into()),
                ("from_address", "a@example.com".into()),
            ]),
        )
        .unwrap_err();
    assert!(e.contains("connection"), "{e}");
    base(&[]).unwrap();
}

#[test]
fn only_send_writes_a_message_and_addresses_are_checked() {
    let r = rig();
    r.server(&[("port", J::from(25))]);
    let e = r
        .call(
            &admin(),
            "outbound_mail.create",
            v(&[("to", "a@example.com".into())]),
        )
        .unwrap_err();
    assert!(e.contains("outbound_mail.send"), "{e}");
    assert!(
        r.send(&[("to", "a@example.com\r\nRCPT TO:<x@evil.test>".into())])
            .is_err()
    );
    assert!(
        r.send(&[("subject", "Hi\r\nBcc: x@evil.test".into())])
            .is_err()
    );
    assert!(r.send(&[("to", "".into())]).is_err());
    // A sender may queue; it may not read the log.
    let sender = Principal::user("bo", &[SENDER]);
    r.call(
        &sender,
        "outbound_mail.send",
        v(&[
            ("to", "a@example.com".into()),
            ("subject", "S".into()),
            ("body", "B".into()),
        ]),
    )
    .unwrap();
    assert!(r.call(&sender, "outbound_mail.list", v(&[])).is_err());
}

#[test]
fn templates_render_in_the_language_asked_for_and_refuse_a_missing_value() {
    let r = rig();
    let s = smtp(Script::default());
    r.server(&[("port", J::from(s.port))]);
    for (lang, subject, body) in [
        (
            "en",
            "Invoice {{number}}",
            "Hello {{name}}, invoice {{number}} is attached.",
        ),
        (
            "de",
            "Rechnung {{number}}",
            "Hallo {{name}}, anbei Rechnung {{number}}.",
        ),
    ] {
        r.call(
            &admin(),
            "mail_template.create",
            v(&[
                ("key", "invoice".into()),
                ("language", lang.into()),
                ("subject", subject.into()),
                ("body", body.into()),
            ]),
        )
        .unwrap();
    }
    let vars = r#"{"name":"Frau Kühn","number":"RE-2026-0042"}"#;
    let id = r
        .send(&[
            ("template", "invoice".into()),
            ("language", "de".into()),
            ("vars", vars.into()),
            ("subject", "".into()),
            ("body", "".into()),
        ])
        .unwrap();
    assert_eq!(text(&r.msg(&id), "subject"), "Rechnung RE-2026-0042");
    // A language without a template falls back to English.
    let id = r
        .send(&[
            ("template", "invoice".into()),
            ("language", "fi".into()),
            ("vars", vars.into()),
        ])
        .unwrap();
    assert_eq!(text(&r.msg(&id), "subject"), "Invoice RE-2026-0042");
    let e = r
        .send(&[
            ("template", "invoice".into()),
            ("vars", r#"{"name":"x"}"#.into()),
        ])
        .unwrap_err();
    assert!(e.contains("needs a value for number"), "{e}");
    r.mail.process(T0).unwrap();
    let data = s.seen.lock().unwrap().data[0].clone();
    assert!(decoded_body(&data).contains("Hallo Frau Kühn"));
    let bad = r.call(
        &admin(),
        "mail_template.create",
        v(&[
            ("key", "x".into()),
            ("language", "en".into()),
            ("subject", "{{ bad name }}".into()),
            ("body", "b".into()),
        ]),
    );
    assert!(bad.is_err());
}

#[test]
fn the_server_is_chosen_by_kind_then_the_default() {
    let r = rig();
    let general = r.server(&[("name", "General".into())]);
    let billing = r.server(&[
        ("name", "Billing".into()),
        ("is_default", false.into()),
        ("kinds", "invoice, reminder".into()),
    ]);
    r.server(&[
        ("name", "Off".into()),
        ("enabled", false.into()),
        ("is_default", false.into()),
        ("kinds", "news".into()),
    ]);
    let id = r.send(&[("kind", "invoice".into())]).unwrap();
    assert_eq!(text(&r.msg(&id), "server"), billing);
    let id = r.send(&[("kind", "news".into())]).unwrap();
    assert_eq!(text(&r.msg(&id), "server"), general);
    let id = r.send(&[]).unwrap();
    assert_eq!(text(&r.msg(&id), "kind"), "general");
}

#[test]
fn nothing_is_queued_without_a_server() {
    let r = rig();
    let e = r.send(&[]).unwrap_err();
    assert!(e.contains("no mail server is set up"), "{e}");
}

#[test]
fn cancel_erases_the_body_and_retention_removes_old_entries() {
    let r = rig();
    r.server(&[("port", J::from(25))]);
    let id = r.send(&[]).unwrap();
    r.call(
        &admin(),
        "outbound_mail.cancel",
        v(&[("id", id.clone().into())]),
    )
    .unwrap();
    let m = r.msg(&id);
    assert_eq!(text(&m, "status"), "cancelled");
    assert_eq!(text(&m, "body"), "");
    assert!(
        r.call(
            &admin(),
            "outbound_mail.cancel",
            v(&[("id", id.clone().into())])
        )
        .is_err()
    );
    assert_eq!(r.mail.purge(T0 + 89 * 86_400).unwrap(), 0);
    assert_eq!(r.mail.purge(T0 + 91 * 86_400).unwrap(), 1);
}

// ---- providers ------------------------------------------------------------------------------------

/// A fake HTTP provider: answers `status` with `body` and keeps the requests.
fn http(status: u16, body: &'static str) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s2 = seen.clone();
    std::thread::spawn(move || {
        for c in listener.incoming() {
            let Ok(mut c) = c else { break };
            let mut r = BufReader::new(c.try_clone().unwrap());
            let mut head = String::new();
            let mut len = 0usize;
            loop {
                let mut l = String::new();
                if r.read_line(&mut l).unwrap_or(0) == 0 {
                    break;
                }
                if let Some(v) = l.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
                head.push_str(&l);
                if l == "\r\n" {
                    break;
                }
            }
            let mut b = vec![0u8; len];
            let _ = r.read_exact(&mut b);
            head.push_str(&String::from_utf8_lossy(&b));
            s2.lock().unwrap().push(head);
            let _ = write!(
                c,
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (port, seen)
}

/// A connected connection of `provider` with a legal basis and the mail fields on its allowlist.
fn connection(r: &Rig, provider: &str, allow: &[&str], creds: &str, config: &str) -> String {
    connection_with(r, provider, allow, creds, config, &[])
}

/// As [`connection`], with more fields on the connection record (its caps).
fn connection_with(
    r: &Rig,
    provider: &str,
    allow: &[&str],
    creds: &str,
    config: &str,
    extra: &[(&str, J)],
) -> String {
    let rt = &r.mail.rt;
    let id = uuid::Uuid::now_v7().to_string();
    let mut sealed = String::new();
    if !creds.is_empty() {
        let J::Object(o) = serde_json::from_str::<J>(creds).unwrap() else {
            panic!()
        };
        sealed = rt.seal_credentials(&o);
    }
    let f = json!({
        "name": provider, "provider": provider, "owner_scope": "instance", "owner_user": "ada",
        "state": "connected", "config": config, "credentials": sealed,
        "field_allowlist": json!({"outbound_mail": allow}).to_string(),
        // Addresses, subject and text leave as they are, never pseudonymised: an explicit raise.
        "policy": json!({"fields": {
            "outbound_mail.to": "allow", "outbound_mail.cc": "allow", "outbound_mail.bcc": "allow",
            "outbound_mail.reply_to": "allow", "outbound_mail.subject": "allow", "outbound_mail.body": "allow",
        }}).to_string(),
    });
    let J::Object(mut f) = f else { panic!() };
    for (k, val) in extra {
        f.insert(k.to_string(), val.clone());
    }
    rt.backend.insert("connection", &id, &f).unwrap();
    let consent = json!({
        "connection": id, "owner_user": "ada", "state": "active", "subject_kind": "instance",
        "purpose": "outbound mail", "legal_basis": "contract", "classes": "none,internal",
    });
    let J::Object(consent) = consent else {
        panic!()
    };
    rt.backend
        .insert("consent", &uuid::Uuid::now_v7().to_string(), &consent)
        .unwrap();
    // The agreement with the provider, which the instance requires for personal data by default.
    let policy = json!({"provider": provider, "enabled": true, "dpa_recorded": true});
    let J::Object(policy) = policy else { panic!() };
    if rt
        .backend
        .get("provider_policy", provider)
        .unwrap()
        .is_none()
    {
        rt.backend
            .insert("provider_policy", provider, &policy)
            .unwrap();
    }
    id
}

#[test]
fn without_a_recorded_agreement_nothing_goes_to_a_provider() {
    let r = rig();
    let (port, seen) = http(200, "{}");
    r.mail
        .use_api_base("postmark", &format!("http://127.0.0.1:{port}"))
        .unwrap();
    let conn = connection(
        &r,
        "postmark",
        &["to", "subject", "body"],
        r#"{"api_key":"pm"}"#,
        "{}",
    );
    r.mail
        .backend
        .delete("provider_policy", "postmark")
        .unwrap();
    r.server(&[("kind", "provider".into()), ("connection", conn.into())]);
    let id = r.send(&[]).unwrap();
    assert_eq!(r.mail.process(T0).unwrap().failed, 1);
    assert!(
        text(&r.msg(&id), "last_error").contains("no agreement"),
        "{:?}",
        r.msg(&id)
    );
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn postmark_gets_one_request_after_the_gate_and_the_flow_is_logged_without_the_payload() {
    let r = rig();
    let (port, seen) = http(200, r#"{"MessageID":"pm-1","ErrorCode":0}"#);
    r.mail
        .use_api_base("postmark", &format!("http://127.0.0.1:{port}"))
        .unwrap();
    let conn = connection(
        &r,
        "postmark",
        &["to", "reply_to", "subject", "body"],
        r#"{"api_key":"pm-server-token-test"}"#,
        "{}",
    );
    r.server(&[
        ("kind", "provider".into()),
        ("connection", conn.clone().into()),
    ]);
    let id = r
        .send(&[("reply_to", "fragen@nordlicht.example".into())])
        .unwrap();
    assert_eq!(r.mail.process(T0).unwrap().sent, 1, "{:?}", r.msg(&id));
    assert!(text(&r.msg(&id), "server_reply").contains("pm-1"));
    let req = seen.lock().unwrap()[0].clone();
    assert!(req.starts_with("POST /email "), "{req}");
    assert!(
        req.to_ascii_lowercase()
            .contains("x-postmark-server-token: pm-server-token-test")
    );
    assert!(req.contains("\"ReplyTo\":\"fragen@nordlicht.example\""));
    let log = r
        .mail
        .backend
        .find("data_flow_log", "connection", &J::from(conn.as_str()))
        .unwrap();
    assert_eq!(log.len(), 1);
    let row = format!("{:?}", log[0].1);
    assert!(
        row.contains("released") && !row.contains("Guten Tag"),
        "{row}"
    );
}

#[test]
fn a_provider_connection_that_does_not_release_the_body_sends_nothing() {
    let r = rig();
    let (port, seen) = http(200, "{}");
    r.mail
        .use_api_base("sendgrid", &format!("http://127.0.0.1:{port}"))
        .unwrap();
    let conn = connection(
        &r,
        "sendgrid",
        &["to", "subject"],
        r#"{"api_key":"sg-test"}"#,
        "{}",
    );
    r.server(&[("kind", "provider".into()), ("connection", conn.into())]);
    let id = r.send(&[]).unwrap();
    assert_eq!(r.mail.process(T0).unwrap().failed, 1);
    assert!(
        text(&r.msg(&id), "last_error").contains("body"),
        "{:?}",
        r.msg(&id)
    );
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn a_provider_429_is_retried_and_a_400_fails() {
    let r = rig();
    let (port, _) = http(429, r#"{"message":"slow down"}"#);
    r.mail
        .use_api_base("mailgun", &format!("http://127.0.0.1:{port}"))
        .unwrap();
    let conn = connection(
        &r,
        "mailgun",
        &["to", "subject", "body"],
        r#"{"api_key":"mg-test"}"#,
        r#"{"domain":"mg.nordlicht.example"}"#,
    );
    r.server(&[("kind", "provider".into()), ("connection", conn.into())]);
    let id = r.send(&[]).unwrap();
    assert_eq!(r.mail.process(T0).unwrap().retrying, 1);
    assert!(text(&r.msg(&id), "last_error").contains("429"));
    let (port, seen) = http(400, r#"{"message":"bad"}"#);
    r.mail
        .use_api_base("mailgun", &format!("http://127.0.0.1:{port}"))
        .unwrap();
    assert_eq!(r.mail.process(T0 + 60).unwrap().failed, 1);
    let req = seen.lock().unwrap()[0].clone();
    assert!(
        req.starts_with("POST /v3/mg.nordlicht.example/messages "),
        "{req}"
    );
    assert!(req.contains("authorization: Basic ") || req.contains("Authorization: Basic "));
}

#[test]
fn graph_and_gmail_send_with_the_connection_s_oauth_token() {
    let r = rig();
    for (provider, path, reply) in [
        ("microsoft_graph_mail", "POST /v1.0/me/sendMail ", 202u16),
        ("gmail_send", "POST /gmail/v1/users/me/messages/send ", 200),
    ] {
        let (port, seen) = http(reply, r#"{"id":"g-1"}"#);
        r.mail
            .use_api_base(provider, &format!("http://127.0.0.1:{port}"))
            .unwrap();
        let conn = connection(&r, provider, &["to", "bcc", "subject", "body"], "", "{}");
        let (c, _) = r.mail.rt.connection(&conn).unwrap();
        let tokens = obop_connectors::oauth::Tokens::from_json(&json!({
            "access_token": "at-test-123", "refresh_token": "rt-test", "expires_at": T0 * 2, "scope": "x",
        }))
        .unwrap();
        r.mail.rt.store_tokens(&c, &tokens).unwrap();
        let srv = r.server(&[
            ("name", provider.into()),
            ("kind", "provider".into()),
            ("connection", conn.into()),
        ]);
        let id = r
            .send(&[
                ("server", srv.into()),
                ("bcc", "archiv@nordlicht.example".into()),
            ])
            .unwrap();
        assert_eq!(
            r.mail.process(T0).unwrap().sent,
            1,
            "{provider}: {:?}",
            r.msg(&id)
        );
        let req = seen.lock().unwrap()[0].clone();
        assert!(req.starts_with(path), "{req}");
        assert!(
            req.to_ascii_lowercase()
                .contains("authorization: bearer at-test-123"),
            "{req}"
        );
        if provider == "microsoft_graph_mail" {
            assert!(req.contains("\"bccRecipients\""), "{req}");
        } else {
            let raw = serde_json::from_str::<J>(req.split_once("\r\n\r\n").unwrap().1).unwrap();
            let mime = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(raw["raw"].as_str().unwrap())
                .unwrap();
            let mime = String::from_utf8(mime).unwrap();
            assert!(
                mime.starts_with("Bcc: archiv@nordlicht.example\r\n"),
                "{mime}"
            );
        }
    }
}

mod private_network;
mod review;
mod second_review;
