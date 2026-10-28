//! What a process does to the database while it starts (`Pg::connect_options`), seen from the
//! wire. Behind a pooler in transaction mode (`OBOP_DB_POOLER=transaction`, PgBouncer) every
//! statement that is prepared has to be prepared and run inside one transaction: outside one, the
//! parse and the bind of a statement can be sent to two different server connections, and the
//! second says that the prepared statement does not exist. The start of a process that tracks
//! nothing must not do that, with any database it could start on before (the history of D75 added
//! a lookup that did). Each test returns early, saying so, when `OBOP_TEST_POSTGRES_URL` is not
//! set.

use crate::Ddl;
use crate::pg::Pg;
use crate::pgpool::PgPoolConfig;
use crate::testing::TestDb;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// A proxy in front of a PostgreSQL server that remembers which statements the client prepares
/// or binds, split by whether a transaction was open at that moment (the state the server last
/// reported with ReadyForQuery).
struct Watch {
    /// `host:port` of the proxy.
    addr: String,
    outside: Arc<Mutex<Vec<String>>>,
    inside: Arc<Mutex<Vec<String>>>,
}

/// The text of a zero-terminated string at the start of `b`, and the rest.
fn cstr(b: &[u8]) -> (String, &[u8]) {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    (
        String::from_utf8_lossy(&b[..end]).into_owned(),
        b.get(end + 1..).unwrap_or(&[]),
    )
}

fn watch(upstream: String) -> Watch {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let outside = Arc::new(Mutex::new(Vec::<String>::new()));
    let inside = Arc::new(Mutex::new(Vec::<String>::new()));
    let (seen_out, seen_in) = (outside.clone(), inside.clone());
    thread::spawn(move || {
        for client in listener.incoming() {
            let Ok(client) = client else { break };
            let Ok(server) = TcpStream::connect(&upstream) else {
                break;
            };
            let state = Arc::new(AtomicU8::new(b'I'));
            // Server to client: every message is typed and framed; ReadyForQuery carries the
            // state of the transaction.
            {
                let (mut from, mut to) = (server.try_clone().unwrap(), client.try_clone().unwrap());
                let state = state.clone();
                thread::spawn(move || {
                    loop {
                        let mut head = [0u8; 5];
                        if from.read_exact(&mut head).is_err() {
                            break;
                        }
                        let len = u32::from_be_bytes(head[1..5].try_into().unwrap()) as usize - 4;
                        let mut body = vec![0u8; len];
                        if from.read_exact(&mut body).is_err() {
                            break;
                        }
                        if head[0] == b'Z' {
                            state.store(body[0], Ordering::SeqCst);
                        }
                        if to.write_all(&head).is_err() || to.write_all(&body).is_err() {
                            break;
                        }
                    }
                });
            }
            // Client to server: the start-up message has no type byte, everything after it has.
            let (seen_out, seen_in) = (seen_out.clone(), seen_in.clone());
            thread::spawn(move || {
                let (mut from, mut to) = (client, server);
                let mut len = [0u8; 4];
                if from.read_exact(&mut len).is_err() {
                    return;
                }
                let mut startup = vec![0u8; u32::from_be_bytes(len) as usize - 4];
                if from.read_exact(&mut startup).is_err() {
                    return;
                }
                assert_ne!(
                    startup[..4],
                    80_877_103u32.to_be_bytes(),
                    "the test connects without TLS"
                );
                if to.write_all(&len).is_err() || to.write_all(&startup).is_err() {
                    return;
                }
                loop {
                    let mut head = [0u8; 5];
                    if from.read_exact(&mut head).is_err() {
                        break;
                    }
                    let len = u32::from_be_bytes(head[1..5].try_into().unwrap()) as usize - 4;
                    let mut body = vec![0u8; len];
                    if from.read_exact(&mut body).is_err() {
                        break;
                    }
                    // Parse (a statement is prepared) and Bind (a prepared one is run).
                    let what = match head[0] {
                        b'P' => {
                            let (name, rest) = cstr(&body);
                            Some(format!("parse {name:?}: {}", cstr(rest).0))
                        }
                        b'B' => {
                            let (_portal, rest) = cstr(&body);
                            Some(format!("bind {:?}", cstr(rest).0))
                        }
                        _ => None,
                    };
                    if let Some(what) = what {
                        let open = state.load(Ordering::SeqCst) != b'I';
                        let list = if open { &seen_in } else { &seen_out };
                        list.lock().unwrap().push(what);
                    }
                    if to.write_all(&head).is_err() || to.write_all(&body).is_err() {
                        break;
                    }
                }
            });
        }
    });
    Watch {
        addr,
        outside,
        inside,
    }
}

/// `url` with its host and port replaced by `hostport`, and the `host:port` it had.
fn through(url: &str, hostport: &str) -> (String, String) {
    let at = url
        .find('@')
        .map_or(url.find("://").unwrap() + 3, |i| i + 1);
    let end = at + url[at..].find('/').unwrap();
    (
        format!("{}{hostport}{}", &url[..at], &url[end..]),
        url[at..end].to_string(),
    )
}

#[test]
fn a_start_prepares_and_binds_only_inside_a_transaction() {
    let Some(db) = TestDb::new() else {
        return;
    };
    let (_, upstream) = through(db.url(), "x");
    let w = watch(upstream);
    let (url, _) = through(db.url(), &w.addr);
    let start = |what: &str, ddl: Ddl| {
        let pg = Pg::connect_options(&url, Some(PgPoolConfig::one_shot()), ddl)
            .unwrap_or_else(|e| panic!("{what}: {e}"));
        drop(pg);
        let outside = w.outside.lock().unwrap().clone();
        assert!(
            outside.is_empty(),
            "{what}: the start prepared or bound outside a transaction, which a pooler in \
             transaction mode may route to another server connection: {outside:#?}"
        );
    };
    // A new database (the record store is made), the same one again (the record store is there:
    // an older database), and one that also has the history.
    start("a new database", Ddl::IfMissing);
    let looked_inside = w.inside.lock().unwrap().len();
    assert!(
        looked_inside > 0,
        "the proxy sees the statements of the start"
    );
    start("an older database, IfMissing", Ddl::IfMissing);
    start("an older database, Never", Ddl::Never);
    crate::schema::owner_install(db.url()).unwrap();
    start("a database with the history, IfMissing", Ddl::IfMissing);
    start("a database with the history, Never", Ddl::Never);
}
