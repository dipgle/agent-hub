//! Regression lock for the web console's gate, driven over real HTTP.
//!
//! The DNS-rebinding hole this covers was invisible to unit tests: the old
//! comment in web.rs claimed the token header stopped it, and the smoke test
//! only proved "no token ⇒ 401". These cases go through the socket with the
//! headers an attacker would actually send.

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use hub::config::Config;

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

/// Raw request so the Host header can be anything — no HTTP client will let you
/// lie about it this easily.
fn request(port: u16, path: &str, host: &str, extra: &[(&str, &str)]) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut req = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    for (k, v) in extra {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    stream.write_all(req.as_bytes()).unwrap();

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).unwrap();
    let code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);
    let mut rest = String::new();
    let _ = reader.read_to_string(&mut rest);
    (code, format!("{status_line}{rest}"))
}

fn start_server(cfg_json: &str) -> (u16, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("hub.config.json");
    std::fs::write(&file, cfg_json).unwrap();

    let mut cfg: Config = hub::config::load(Some(&file)).unwrap();
    let port = free_port();
    cfg.web.port = port;

    std::thread::spawn(move || {
        let _ = hub::web::serve(cfg, port);
    });
    // Wait for the listener rather than guessing.
    for _ in 0..100 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    (port, dir)
}

#[test]
fn a_rebound_host_is_refused_even_on_loopback() {
    let (port, _dir) = start_server(r#"{ "web": { "enabled": true } }"#);

    // The owner's own browser: allowed.
    let (ok_code, body) = request(port, "/", &format!("127.0.0.1:{port}"), &[]);
    assert_eq!(ok_code, 200, "the real console must load");
    assert!(body.contains("hub"), "page body expected");

    // DNS-rebinding: same socket, attacker's hostname in Host.
    for host in ["evil.tld", "attacker.example.com", "hub.local"] {
        let (code, _) = request(port, "/", &format!("{host}:{port}"), &[]);
        assert_eq!(
            code, 403,
            "Host {host} must be refused, not served the token page"
        );
    }
}

#[test]
fn the_token_page_is_not_framable_and_not_cached() {
    let (port, _dir) = start_server(r#"{ "web": { "enabled": true } }"#);
    let (code, resp) = request(port, "/", &format!("localhost:{port}"), &[]);
    assert_eq!(code, 200);

    let lower = resp.to_lowercase();
    assert!(
        lower.contains("x-frame-options: deny"),
        "clickjacking guard missing:\n{resp}"
    );
    assert!(
        lower.contains("frame-ancestors 'none'"),
        "CSP frame-ancestors missing"
    );
    assert!(
        lower.contains("cache-control: no-store"),
        "the page carries a live token; it must not be cached"
    );
}

#[test]
fn api_requires_the_token_and_a_wrong_one_is_refused() {
    let (port, _dir) = start_server(r#"{ "web": { "enabled": true } }"#);
    let host = format!("127.0.0.1:{port}");

    let (no_token, _) = request(port, "/api/inbox", &host, &[]);
    assert_eq!(no_token, 401);

    let (bad_token, _) = request(
        port,
        "/api/inbox",
        &host,
        &[("x-hub-token", "0".repeat(32).as_str())],
    );
    assert_eq!(bad_token, 401);

    // …and the rebinding check runs before the token check, so a wrong Host
    // cannot even probe token validity.
    let (bad_host, _) = request(
        port,
        "/api/inbox",
        "evil.tld",
        &[("x-hub-token", "0".repeat(32).as_str())],
    );
    assert_eq!(bad_host, 403);
}

#[test]
fn the_vendored_asset_is_gated_too() {
    let (port, _dir) = start_server(r#"{ "web": { "enabled": true } }"#);
    let (code, _) = request(port, "/echarts.js", "evil.tld", &[]);
    assert_eq!(
        code, 403,
        "a route that skips the gate teaches the next one to skip it"
    );

    let (ok, _) = request(port, "/echarts.js", &format!("127.0.0.1:{port}"), &[]);
    assert_eq!(ok, 200);
}

#[test]
fn binding_off_loopback_without_a_password_refuses_to_start() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("hub.config.json");
    std::env::remove_var("HUB_TEST_NO_PW");
    std::fs::write(
        &file,
        r#"{ "web": { "enabled": true, "bind": "0.0.0.0", "password_env": "HUB_TEST_NO_PW" } }"#,
    )
    .unwrap();

    // It is rejected at load time, before anything listens.
    let err = hub::config::load(Some(&file)).unwrap_err().to_string();
    assert!(err.contains("not loopback"), "{err}");
}
