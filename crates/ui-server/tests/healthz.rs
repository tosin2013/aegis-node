//! End-to-end test: bind the router on an ephemeral loopback port,
//! make a real HTTP call to `/healthz` and `/api/v1/version`, and
//! verify the responses match what the SPA expects.
//!
//! Uses `axum::serve` against a real TCP listener (rather than
//! `tower::ServiceExt::oneshot`) so we exercise the same code path
//! the CLI runs in production.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::time::Duration;

use aegis_ui_server::{router, Config};
use http_body_util::BodyExt;
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

fn test_config(addr: SocketAddr) -> Config {
    Config {
        version: "0.0.0-test".to_string(),
        features: vec!["llama".to_string()],
        listen: addr,
    }
}

async fn boot(config: Config) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral");
    let bound = listener.local_addr().expect("bound addr");
    let app = router(config);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    // Tiny grace period for the spawn to start accepting.
    tokio::time::sleep(Duration::from_millis(20)).await;
    bound
}

async fn fetch(addr: SocketAddr, path: &str) -> (hyper::StatusCode, Bytes) {
    let client: Client<_, http_body_util::Empty<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();
    let uri = format!("http://{addr}{path}");
    let req = Request::builder()
        .uri(uri)
        .body(http_body_util::Empty::<Bytes>::new())
        .expect("build request");
    let resp = client.request(req).await.expect("request");
    let status = resp.status();
    let body = resp
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    (status, body)
}

#[tokio::test]
async fn healthz_returns_ok_true() {
    let addr = "127.0.0.1:0".parse().expect("test addr");
    let bound = boot(test_config(addr)).await;
    let (status, body) = fetch(bound, "/healthz").await;
    assert_eq!(status, hyper::StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).expect("healthz returns valid JSON");
    assert_eq!(v["ok"], true);
}

#[tokio::test]
async fn version_echoes_config() {
    let addr = "127.0.0.1:0".parse().expect("test addr");
    let bound = boot(test_config(addr)).await;
    let (status, body) = fetch(bound, "/api/v1/version").await;
    assert_eq!(status, hyper::StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).expect("version returns valid JSON");
    assert_eq!(v["version"], "0.0.0-test");
    assert_eq!(v["features"][0], "llama");
}

#[tokio::test]
async fn root_serves_index_html() {
    let addr = "127.0.0.1:0".parse().expect("test addr");
    let bound = boot(test_config(addr)).await;
    let (status, body) = fetch(bound, "/").await;
    assert_eq!(status, hyper::StatusCode::OK);
    let html = std::str::from_utf8(&body).expect("index.html is utf8");
    assert!(
        html.contains("Aegis-Node"),
        "placeholder index.html should mention Aegis-Node, got: {html}"
    );
}

#[tokio::test]
async fn unknown_path_falls_back_to_index() {
    // SPA history fallback — `/some/route` that isn't an asset
    // should serve index.html so the client-side router can take
    // over.
    let addr = "127.0.0.1:0".parse().expect("test addr");
    let bound = boot(test_config(addr)).await;
    let (status, body) = fetch(bound, "/chat/session-123").await;
    assert_eq!(status, hyper::StatusCode::OK);
    let html = std::str::from_utf8(&body).expect("fallback is utf8");
    assert!(html.contains("Aegis-Node"), "{html}");
}
