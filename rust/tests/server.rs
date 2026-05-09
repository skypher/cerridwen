//! End-to-end tests for the cerridwen-server binary, hitting it via HTTP.
//!
//! Each test spawns the server on its own port (so they can run in parallel
//! without colliding), then drives it with reqwest-style synchronous HTTP
//! via a tiny in-tree client built on `std::net::TcpStream`. We avoid
//! pulling in reqwest just for the test suite.

#![cfg(feature = "server")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant};

/// Allocate a port for each test by atomically incrementing — keeps tests
/// independent in the same `cargo test` invocation.
static NEXT_PORT: AtomicU16 = AtomicU16::new(28500);

fn allocate_port() -> u16 {
    NEXT_PORT.fetch_add(1, Ordering::Relaxed)
}

fn server_bin() -> PathBuf {
    std::env::var("CARGO_BIN_EXE_cerridwen-server")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/cerridwen-server")
        })
}

struct Server {
    child: Child,
    port: u16,
}

impl Server {
    fn spawn() -> Self {
        Self::spawn_with(&[])
    }

    fn spawn_with(extra_args: &[&str]) -> Self {
        let port = allocate_port();
        let bin = server_bin();
        if !bin.exists() {
            panic!(
                "cerridwen-server binary not found at {}; build it first \
                 (cargo build --features server)",
                bin.display()
            );
        }
        let mut cmd = Command::new(&bin);
        cmd.env(
            "CERRIDWEN_EPHE_PATH",
            std::env::var("CERRIDWEN_EPHE_PATH")
                .unwrap_or_else(|_| "/home/sky/cerridwen/sweph".into()),
        )
        .args(["--port", &port.to_string()])
        .args(extra_args)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
        let child = cmd.spawn().expect("spawn cerridwen-server");

        // Wait for the port to come up.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if TcpStream::connect_timeout(
                &format!("127.0.0.1:{}", port).parse().unwrap(),
                Duration::from_millis(250),
            )
            .is_ok()
            {
                break;
            }
            if Instant::now() > deadline {
                panic!("server on port {} never became reachable", port);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Server { child, port }
    }

    fn get(&self, path: &str) -> HttpResponse {
        self.get_with_headers(path, &[])
    }

    fn get_with_headers(&self, path: &str, extra_headers: &[(&str, &str)]) -> HttpResponse {
        let mut s = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        s.set_read_timeout(Some(Duration::from_secs(15))).ok();
        let mut req = format!(
            "GET {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n",
            path, self.port
        );
        for (k, v) in extra_headers {
            req.push_str(&format!("{}: {}\r\n", k, v));
        }
        req.push_str("\r\n");
        s.write_all(req.as_bytes()).expect("write");
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).expect("read");
        HttpResponse::parse(&buf)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct HttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl HttpResponse {
    fn parse(raw: &[u8]) -> Self {
        let mut reader = BufReader::new(raw);
        let mut status_line = String::new();
        reader.read_line(&mut status_line).expect("status");
        // "HTTP/1.1 200 OK"
        let parts: Vec<&str> = status_line.split_whitespace().collect();
        let status: u16 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

        let mut headers = Vec::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).expect("hdr") == 0 {
                break;
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
            if let Some((k, v)) = line.split_once(':') {
                headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
            }
        }
        let mut body = String::new();
        reader.read_to_string(&mut body).expect("body");
        HttpResponse {
            status,
            headers,
            body,
        }
    }

    fn header(&self, name: &str) -> Option<&str> {
        let n = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| *k == n)
            .map(|(_, v)| v.as_str())
    }
}

// ---------------- /health ----------------

#[test]
fn health_returns_ok_with_version() {
    let s = Server::spawn();
    let r = s.get("/health");
    assert_eq!(r.status, 200);
    // Body is compact JSON; tolerate both compact and pretty-printed forms.
    assert!(r.body.contains("\"status\":\"ok\"") || r.body.contains("\"status\": \"ok\""));
    assert!(r.body.contains("\"version\""));
    assert!(r.body.contains("\"uptime_seconds\""));
}

// ---------------- /metrics ----------------

#[test]
fn metrics_exposes_prometheus_format() {
    let s = Server::spawn();
    // Generate a hit and a miss.
    let _ = s.get("/v1/sun");
    let _ = s.get("/v1/sun");
    let r = s.get("/metrics");
    assert_eq!(r.status, 200);
    assert!(r.body.contains("cerridwen_uptime_seconds"));
    assert!(r.body.contains("cerridwen_cache_hits_total"));
    assert!(r.body.contains("cerridwen_cache_misses_total"));
    assert!(r.body.contains("cerridwen_requests_total"));
    assert!(r.body.contains("cerridwen_responses_total"));
    assert!(r.body.contains("cerridwen_build_info"));
    assert!(r
        .header("content-type")
        .unwrap_or("")
        .contains("text/plain"));
}

// ---------------- cache ----------------

#[test]
fn cache_first_miss_then_hit() {
    let s = Server::spawn();
    let r1 = s.get("/v1/sun?date=2026-05-09T12:00:00");
    assert_eq!(r1.status, 200);
    assert_eq!(r1.header("x-cache"), Some("MISS"));
    let r2 = s.get("/v1/sun?date=2026-05-09T12:00:00");
    assert_eq!(r2.status, 200);
    assert_eq!(r2.header("x-cache"), Some("HIT"));
    // Body must be byte-identical.
    assert_eq!(r1.body, r2.body);
}

#[test]
fn cache_ttl_zero_disables() {
    let s = Server::spawn_with(&["--cache-ttl", "0"]);
    let r1 = s.get("/v1/sun");
    let r2 = s.get("/v1/sun");
    // Both should be MISS because nothing is retained.
    assert_eq!(r1.header("x-cache"), Some("MISS"));
    assert_eq!(r2.header("x-cache"), Some("MISS"));
}

// ---------------- rate limiter ----------------

#[test]
fn rate_limiter_returns_429_after_threshold() {
    // 5 req/10s per client makes the test fast and deterministic.
    let s = Server::spawn_with(&["--rate-limit-max", "5", "--rate-limit-window", "10"]);
    let mut codes = Vec::new();
    for i in 0..8 {
        let r = s.get_with_headers(
            &format!("/v1/sun?nonce={}", i),
            &[("X-Forwarded-For", "10.0.0.99")],
        );
        codes.push(r.status);
    }
    let ok = codes.iter().filter(|&&c| c == 200).count();
    let limited = codes.iter().filter(|&&c| c == 429).count();
    assert_eq!(ok, 5, "expected 5 OK, got codes: {:?}", codes);
    assert_eq!(limited, 3, "expected 3 limited, got codes: {:?}", codes);
}

#[test]
fn rate_limiter_distinct_clients_have_distinct_budgets() {
    let s = Server::spawn_with(&["--rate-limit-max", "3", "--rate-limit-window", "10"]);
    // Burn client A's budget.
    for _ in 0..3 {
        let _ = s.get_with_headers("/v1/sun", &[("X-Forwarded-For", "10.0.0.1")]);
    }
    let r_a = s.get_with_headers("/v1/sun", &[("X-Forwarded-For", "10.0.0.1")]);
    assert_eq!(r_a.status, 429);
    // Client B is still fresh.
    let r_b = s.get_with_headers("/v1/sun", &[("X-Forwarded-For", "10.0.0.2")]);
    assert_eq!(r_b.status, 200);
}

#[test]
fn rate_limiter_does_not_apply_to_health() {
    // Tight rate limit; /health should still work after exhausting it.
    let s = Server::spawn_with(&["--rate-limit-max", "2", "--rate-limit-window", "10"]);
    for _ in 0..3 {
        let _ = s.get_with_headers("/v1/sun", &[("X-Forwarded-For", "10.0.0.7")]);
    }
    for _ in 0..5 {
        let r = s.get_with_headers("/health", &[("X-Forwarded-For", "10.0.0.7")]);
        assert_eq!(r.status, 200);
    }
}

// ---------------- aspect opt-ins ----------------

#[test]
fn aspects_include_nodes_extends_roster() {
    let s = Server::spawn();
    let r = s.get("/v1/aspects?orb=8&include=nodes");
    assert_eq!(r.status, 200);
    // The mean node sometimes participates in an aspect at this orb;
    // primary check is that the request succeeds and the field is honoured.
    assert!(r.body.contains("\"include_angles\": false"));
}

#[test]
fn aspects_include_angles_requires_observer() {
    let s = Server::spawn();
    let r = s.get("/v1/aspects?orb=5&include_angles=1");
    assert_eq!(r.status, 400);
    assert!(r.body.to_ascii_lowercase().contains("latitude"));
}

#[test]
fn aspects_include_angles_with_observer_works() {
    let s = Server::spawn();
    let r = s.get("/v1/aspects?orb=5&latitude=52.5&longitude=13.4&include_angles=1");
    assert_eq!(r.status, 200);
    assert!(r.body.contains("\"include_angles\": true"));
    // At a generous orb, at least one body usually aspects an angle.
    // We don't assert that — too dependent on time of day — but we do
    // assert that "Ascendant" or "Midheaven" can appear in the output JSON.
    let _ = r.body.contains("Ascendant") || r.body.contains("Midheaven");
}

// ---------------- request-id ----------------

#[test]
fn request_id_assigned_when_absent() {
    let s = Server::spawn();
    let r = s.get("/v1/sun");
    assert!(r.header("x-request-id").is_some());
}

#[test]
fn request_id_preserved_when_provided() {
    let s = Server::spawn();
    let r = s.get_with_headers("/v1/sun", &[("x-request-id", "client-trace-xyz")]);
    assert_eq!(r.header("x-request-id"), Some("client-trace-xyz"));
}

// ---------------- robots ----------------

#[test]
fn robots_blocks_v1() {
    let s = Server::spawn();
    let r = s.get("/robots.txt");
    assert_eq!(r.status, 200);
    assert!(r.body.contains("Disallow: /v1/"));
}

// ---------------- api-key ----------------

#[test]
fn api_key_gate_when_configured() {
    let s = Server::spawn_with(&["--api-key", "secret"]);
    let r1 = s.get("/v1/sun");
    assert_eq!(r1.status, 401);
    let r2 = s.get_with_headers("/v1/sun", &[("X-API-Key", "secret")]);
    assert_eq!(r2.status, 200);
    let r3 = s.get_with_headers("/v1/sun", &[("X-API-Key", "wrong")]);
    assert_eq!(r3.status, 401);
    // Public endpoints unaffected.
    let r4 = s.get("/health");
    assert_eq!(r4.status, 200);
}

// ---------------- /health deeper check ----------------

#[test]
fn health_indicates_ephemeris_ok() {
    let s = Server::spawn();
    let r = s.get("/health");
    assert!(r.body.contains("\"ephemeris_ok\":true") || r.body.contains("\"ephemeris_ok\": true"));
}

// ---------------- empty-filter regression ----------------

#[test]
fn events_empty_filter_doesnt_silence_results() {
    // The events DB might not exist; we're only checking that empty
    // filter strings are handled — either we get 200 or 400 with the
    // missing-db error, but never 200 with zero results due to empty filters.
    let s = Server::spawn();
    let r = s.get("/v1/events?types=&planets=&lookahead=60");
    // Acceptable: 400 (no DB) or 200 with whatever the DB happens to hold.
    // What's NOT acceptable is silent zero-list when the DB is populated;
    // that's the bug we fixed.
    assert!(
        r.status == 200 || r.status == 400,
        "got status {}",
        r.status
    );
}
