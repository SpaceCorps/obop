//! O38: the listener lets go of a client that stalls in the head or the body of a request.

use axum::{Router, routing::post};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const SHORT: Duration = Duration::from_millis(300);

async fn start() -> std::net::SocketAddr {
    let app = Router::new().route(
        "/echo",
        post(
            |axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
             body: axum::body::Bytes| async move { format!("{} {}", peer.ip(), body.len()) },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(crate::serve_on(listener, app, SHORT, SHORT));
    addr
}

/// Write `sent`, then read until the server closes the connection; what was read and how long it
/// took. Gives up (and fails the test) after five seconds.
async fn stall(addr: std::net::SocketAddr, sent: &[u8]) -> (String, Duration) {
    let mut c = tokio::net::TcpStream::connect(addr).await.unwrap();
    c.write_all(sent).await.unwrap();
    let started = Instant::now();
    let mut got = Vec::new();
    let read = tokio::time::timeout(Duration::from_secs(5), c.read_to_end(&mut got)).await;
    assert!(
        read.is_ok(),
        "the server still holds the connection after 5 s"
    );
    (
        String::from_utf8_lossy(&got).into_owned(),
        started.elapsed(),
    )
}

#[tokio::test]
async fn a_request_whose_body_stalls_is_ended() {
    let addr = start().await;
    let (answer, took) = stall(
        addr,
        b"POST /echo HTTP/1.1\r\nhost: x\r\ncontent-length: 100\r\n\r\nhello",
    )
    .await;
    assert!(took < Duration::from_secs(3), "{took:?}");
    assert!(!answer.contains("200 OK"), "{answer}");
}

#[tokio::test]
async fn a_request_whose_head_stalls_is_ended() {
    let addr = start().await;
    let (answer, took) = stall(addr, b"POST /echo HTTP/1.1\r\nhost: x\r\n").await;
    assert!(took < Duration::from_secs(3), "{took:?}");
    assert!(!answer.contains("200 OK"), "{answer}");
}

#[tokio::test]
async fn a_whole_request_is_answered_with_the_peer_address() {
    let addr = start().await;
    let (answer, _) = stall(
        addr,
        b"POST /echo HTTP/1.1\r\nhost: x\r\nconnection: close\r\ncontent-length: 5\r\n\r\nhello",
    )
    .await;
    assert!(answer.starts_with("HTTP/1.1 200 OK"), "{answer}");
    assert!(answer.ends_with("127.0.0.1 5"), "{answer}");
}
