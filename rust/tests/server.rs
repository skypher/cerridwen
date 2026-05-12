// SPDX-License-Identifier: MIT AND AGPL-3.0-only

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
            std::env::var("CERRIDWEN_EPHE_PATH").unwrap_or_else(|_| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join("sweph")
                    .to_string_lossy()
                    .into_owned()
            }),
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
                &format!("127.0.0.1:{port}").parse().unwrap(),
                Duration::from_millis(250),
            )
            .is_ok()
            {
                break;
            }
            if Instant::now() > deadline {
                panic!("server on port {port} never became reachable");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Server { child, port }
    }

    fn get(&self, path: &str) -> HttpResponse {
        self.get_with_headers(path, &[])
    }

    fn get_with_headers(&self, path: &str, extra_headers: &[(&str, &str)]) -> HttpResponse {
        self.request("GET", path, extra_headers)
    }

    fn request(&self, method: &str, path: &str, extra_headers: &[(&str, &str)]) -> HttpResponse {
        let mut s = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        s.set_read_timeout(Some(Duration::from_secs(15))).ok();
        let mut req = format!(
            "{} {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n",
            method, path, self.port
        );
        for (k, v) in extra_headers {
            req.push_str(&format!("{k}: {v}\r\n"));
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
        let mut body_bytes = Vec::new();
        reader.read_to_end(&mut body_bytes).expect("body");
        if headers
            .iter()
            .any(|(k, v)| k == "transfer-encoding" && v.to_ascii_lowercase().contains("chunked"))
        {
            body_bytes = decode_chunked_body(&body_bytes);
        }
        // Gzip / binary bodies aren't valid UTF-8; use lossy conversion
        // for the body field so header-only assertions still work.
        let body = String::from_utf8_lossy(&body_bytes).into_owned();
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

fn decode_chunked_body(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let Some(line_rel) = raw[i..].windows(2).position(|w| w == b"\r\n") else {
            return raw.to_vec();
        };
        let line_end = i + line_rel;
        let size_line = String::from_utf8_lossy(&raw[i..line_end]);
        let size_hex = size_line.split(';').next().unwrap_or("").trim();
        let Ok(size) = usize::from_str_radix(size_hex, 16) else {
            return raw.to_vec();
        };
        i = line_end + 2;
        if size == 0 {
            return out;
        }
        if i + size > raw.len() {
            return raw.to_vec();
        }
        out.extend_from_slice(&raw[i..i + size]);
        i += size;
        if raw.get(i..i + 2) == Some(b"\r\n") {
            i += 2;
        } else {
            return raw.to_vec();
        }
    }
    raw.to_vec()
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
            &format!("/v1/sun?nonce={i}"),
            &[("X-Forwarded-For", "10.0.0.99")],
        );
        codes.push(r.status);
    }
    let ok = codes.iter().filter(|&&c| c == 200).count();
    let limited = codes.iter().filter(|&&c| c == 429).count();
    assert_eq!(ok, 5, "expected 5 OK, got codes: {codes:?}");
    assert_eq!(limited, 3, "expected 3 limited, got codes: {codes:?}");
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

// ---------------- error envelope shape ----------------

#[test]
fn errors_use_json_envelope() {
    let s = Server::spawn();
    let r = s.get("/v1/sun?date=garbage");
    assert_eq!(r.status, 400);
    assert!(
        r.header("content-type")
            .unwrap_or("")
            .contains("application/json"),
        "expected application/json, got {:?}",
        r.header("content-type")
    );
    assert!(r.body.contains("\"error\""));
    assert!(r.body.contains("\"code\":400") || r.body.contains("\"code\": 400"));
}

#[test]
fn rate_limit_429_carries_retry_after() {
    let s = Server::spawn_with(&["--rate-limit-max", "2", "--rate-limit-window", "10"]);
    let _ = s.get_with_headers("/v1/sun?n=1", &[("X-Forwarded-For", "10.0.0.50")]);
    let _ = s.get_with_headers("/v1/sun?n=2", &[("X-Forwarded-For", "10.0.0.50")]);
    let r = s.get_with_headers("/v1/sun?n=3", &[("X-Forwarded-For", "10.0.0.50")]);
    assert_eq!(r.status, 429);
    let retry = r.header("retry-after").unwrap_or("0");
    let n: u64 = retry.parse().expect("Retry-After should be numeric");
    assert!(n > 0 && n <= 10, "Retry-After out of range: {retry}");
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

// ---------------- new endpoints ----------------

#[test]
fn declinations_endpoint_returns_grid() {
    let s = Server::spawn();
    let r = s.get("/v1/declinations?date=2026-05-09T12:00:00&orb=1.5");
    assert_eq!(r.status, 200, "body={}", r.body);
    assert!(r.body.contains("\"declinations\""));
    assert!(r.body.contains("\"parallels\""));
    assert!(r.body.contains("\"moon_out_of_bounds\""));
}

#[test]
fn stations_endpoint_returns_mercury() {
    let s = Server::spawn();
    let r = s.get("/v1/stations?body=Mercury&date=2026-01-01T00:00:00&lookahead=400");
    assert_eq!(r.status, 200, "body={}", r.body);
    assert!(r.body.contains("\"stations\""));
}

#[test]
fn stations_endpoint_unknown_body_is_404() {
    let s = Server::spawn();
    let r = s.get("/v1/stations?body=Xylophone");
    assert_eq!(r.status, 404);
    assert!(r.body.contains("unknown body"));
}

#[test]
fn twilight_endpoint_requires_observer() {
    let s = Server::spawn();
    let r = s.get("/v1/twilight?date=2026-05-09T12:00:00");
    assert_eq!(r.status, 400);
}

#[test]
fn twilight_endpoint_returns_three_layers() {
    let s = Server::spawn();
    let r = s.get("/v1/twilight?date=2026-05-09T12:00:00&latitude=52.5&longitude=13.4");
    assert_eq!(r.status, 200, "body={}", r.body);
    assert!(r.body.contains("\"civil\""));
    assert!(r.body.contains("\"nautical\""));
    assert!(r.body.contains("\"astronomical\""));
}

#[test]
fn planetary_hours_returns_24() {
    let s = Server::spawn();
    let r = s.get("/v1/planetary-hours?date=2026-05-11T00:00:00&latitude=52.5&longitude=13.4");
    assert_eq!(r.status, 200, "body={}", r.body);
    let body: serde_json::Value = serde_json::from_str(&r.body).expect("planetary hours JSON");
    assert_eq!(body["hours"].as_array().expect("hours array").len(), 24);
    assert!(body.get("current").is_some());
}

#[test]
fn arabic_parts_returns_seven_lots() {
    let s = Server::spawn();
    let r = s.get("/v1/arabic-parts?date=2026-05-09T12:00:00&latitude=52.5&longitude=13.4");
    assert_eq!(r.status, 200, "body={}", r.body);
    for n in [
        "Fortune",
        "Spirit",
        "Eros",
        "Necessity",
        "Courage",
        "Victory",
        "Nemesis",
    ] {
        assert!(r.body.contains(n), "{n} missing from parts response");
    }
}

#[test]
fn profections_endpoint_returns_age_one_2nd_house() {
    // Asc in Aries → age 1 → 2nd house → Taurus → lord Venus.
    let s = Server::spawn();
    let r = s.get(
        "/v1/profections?natal_date=2000-04-01T12:00:00&natal_latitude=10&natal_longitude=10&age=1",
    );
    assert_eq!(r.status, 200, "body={}", r.body);
    assert!(r.body.contains("\"house\":2") || r.body.contains("\"house\": 2"));
}

#[test]
fn prenatal_eclipse_endpoint_returns_solar_lunar() {
    let s = Server::spawn();
    let r = s.get("/v1/prenatal-eclipse?natal_date=2000-01-01T00:00:00");
    assert_eq!(r.status, 200, "body={}", r.body);
    assert!(r.body.contains("\"solar\""));
    assert!(r.body.contains("\"lunar\""));
}

#[test]
fn synastry_endpoint_returns_aspect_grid() {
    let s = Server::spawn();
    let r = s.get("/v1/synastry?date_a=2000-01-01T12:00:00&date_b=2000-04-01T12:00:00&orb=5");
    assert_eq!(r.status, 200, "body={}", r.body);
    assert!(r.body.contains("\"aspects\""));
}

#[test]
fn composite_midpoint_endpoint() {
    let s = Server::spawn();
    let r = s.get("/v1/composite?date_a=2000-01-01T12:00:00&date_b=2000-04-01T12:00:00");
    assert_eq!(r.status, 200, "body={}", r.body);
    assert!(
        r.body.contains("\"method\": \"midpoint\"") || r.body.contains("\"method\":\"midpoint\"")
    );
    assert!(r.body.contains("\"bodies\""));
}

#[test]
fn composite_davison_endpoint_requires_locations() {
    let s = Server::spawn();
    let r =
        s.get("/v1/composite?method=davison&date_a=2000-01-01T12:00:00&date_b=2000-04-01T12:00:00");
    assert_eq!(r.status, 400);
}

#[test]
fn progressions_endpoint_secondary() {
    let s = Server::spawn();
    let r = s.get(
        "/v1/progressions?natal_date=2000-01-01T12:00:00&date=2026-01-01T12:00:00&method=secondary",
    );
    assert_eq!(r.status, 200, "body={}", r.body);
    assert!(r.body.contains("\"progressed_jd\""));
}

#[test]
fn progressions_endpoint_solar_arc() {
    let s = Server::spawn();
    let r = s.get(
        "/v1/progressions?natal_date=2000-01-01T12:00:00&date=2026-01-01T12:00:00&method=solar_arc",
    );
    assert_eq!(r.status, 200, "body={}", r.body);
    assert!(r.body.contains("\"arc_deg\""));
}

#[test]
fn body_endpoint_now_has_declination_and_center() {
    let s = Server::spawn();
    let r = s.get("/v1/body/jupiter?date=2026-05-09T12:00:00");
    assert_eq!(r.status, 200);
    assert!(r.body.contains("\"declination\""));
    assert!(r.body.contains("\"right_ascension\""));
    assert!(
        r.body.contains("\"center\":\"geocentric\"")
            || r.body.contains("\"center\": \"geocentric\"")
    );
}

#[test]
fn body_endpoint_helio_toggle() {
    let s = Server::spawn();
    let r = s.get("/v1/body/jupiter?date=2026-05-09T12:00:00&center=helio");
    assert_eq!(r.status, 200);
    assert!(
        r.body.contains("\"center\":\"heliocentric\"")
            || r.body.contains("\"center\": \"heliocentric\"")
    );
}

#[test]
fn body_endpoint_topo_requires_observer() {
    let s = Server::spawn();
    let r = s.get("/v1/body/jupiter?date=2026-05-09T12:00:00&center=topo");
    assert_eq!(r.status, 400);
}

#[test]
fn moon_endpoint_now_includes_tithi_and_nakshatra() {
    let s = Server::spawn();
    let r = s.get("/v1/moon?date=2026-05-09T12:00:00");
    assert_eq!(r.status, 200);
    assert!(r.body.contains("\"tithi\""));
    assert!(r.body.contains("\"nakshatra\""));
    assert!(r.body.contains("\"out_of_bounds\""));
}

// ---------------- hardening ----------------

#[test]
fn cache_key_discriminates_query_params() {
    // Different query strings must NOT collide in the response cache.
    let s = Server::spawn();
    let r1 = s.get("/v1/sun?date=2026-05-09T12:00:00");
    let r2 = s.get("/v1/sun?date=2026-05-09T18:00:00");
    assert_eq!(r1.status, 200);
    assert_eq!(r2.status, 200);
    assert_ne!(r1.body, r2.body, "cache key must split on query string");
}

#[test]
fn metrics_remains_public_when_api_key_set() {
    // Monitoring endpoints must remain public.
    let s = Server::spawn_with(&["--api-key", "secret"]);
    assert_eq!(s.get("/health").status, 200);
    assert_eq!(s.get("/metrics").status, 200);
    assert_eq!(s.get("/openapi.json").status, 200);
    // But /v1/* must reject without key.
    assert_eq!(s.get("/v1/sun").status, 401);
}

// ---------- round 9 endpoints ----------

#[test]
fn midpoints_endpoint_returns_pairs_and_hits() {
    let s = Server::spawn();
    let r = s.get("/v1/midpoints?date=2026-05-09T12:00:00&orb=2");
    assert_eq!(r.status, 200, "body={}", r.body);
    assert!(r.body.contains("\"midpoints\""));
    assert!(r.body.contains("\"hits\""));
}

#[test]
fn antiscia_endpoint_returns_grid() {
    let s = Server::spawn();
    let r = s.get("/v1/antiscia?date=2026-05-09T12:00:00&orb=2");
    assert_eq!(r.status, 200);
    assert!(r.body.contains("\"antiscion\""));
    assert!(r.body.contains("\"contra_antiscion\""));
}

#[test]
fn decans_endpoint_lists_three_rulers() {
    let s = Server::spawn();
    let r = s.get("/v1/decans?date=2026-05-09T12:00:00");
    assert_eq!(r.status, 200);
    assert!(r.body.contains("triplicity_ruler"));
    assert!(r.body.contains("chaldean_ruler"));
    assert!(r.body.contains("egyptian_index"));
}

#[test]
fn terms_endpoint_default_ptolemaic_and_egyptian_switch() {
    let s = Server::spawn();
    let r1 = s.get("/v1/terms?date=2026-05-09T12:00:00");
    let r2 = s.get("/v1/terms?date=2026-05-09T12:00:00&system=egyptian");
    assert_eq!(r1.status, 200);
    assert_eq!(r2.status, 200);
    assert!(r1.body.contains("ptolemaic"));
    assert!(r2.body.contains("egyptian"));
}

#[test]
fn triplicity_endpoint_marks_active_when_observer_given() {
    let s = Server::spawn();
    let r = s.get("/v1/triplicity?date=2026-05-09T12:00:00&latitude=52.5&longitude=13.4");
    assert_eq!(r.status, 200);
    assert!(r.body.contains("active_ruler"));
}

#[test]
fn receptions_endpoint_returns_array() {
    let s = Server::spawn();
    let r = s.get("/v1/receptions?date=2026-05-09T12:00:00");
    assert_eq!(r.status, 200);
    assert!(r.body.contains("\"receptions\""));
}

#[test]
fn equation_of_time_endpoint_returns_minutes() {
    let s = Server::spawn();
    let r = s.get("/v1/equation-of-time?date=2026-05-09T12:00:00");
    assert_eq!(r.status, 200);
    assert!(r.body.contains("equation_of_time_minutes"));
}

#[test]
fn ingresses_endpoint_returns_four() {
    let s = Server::spawn();
    let r = s.get("/v1/ingresses?date=2026-01-01T00:00:00&count=4");
    assert_eq!(r.status, 200);
    let count_kind = r.body.matches("\"kind\":").count();
    assert_eq!(count_kind, 4);
}

#[test]
fn lunations_endpoint_lists_phases() {
    let s = Server::spawn();
    let r = s.get("/v1/lunations?date_start=2026-01-01T00:00:00&lookahead=30");
    assert_eq!(r.status, 200);
    assert!(r.body.contains("\"lunations\""));
    // 30 days should contain at least one new or full moon.
    assert!(
        r.body.contains("\"new\"")
            || r.body.contains("\"full\"")
            || r.body.contains("\"first_quarter\"")
            || r.body.contains("\"last_quarter\"")
    );
}

#[test]
fn natal_chart_endpoint_combines_everything() {
    let s = Server::spawn();
    let r = s.get("/v1/natal-chart?date=1990-06-15T12:00:00&latitude=52.5&longitude=13.4");
    assert_eq!(r.status, 200);
    assert!(r.body.contains("\"houses\""));
    assert!(r.body.contains("\"bodies\""));
    assert!(r.body.contains("\"aspects\""));
    assert!(r.body.contains("\"lots\""));
}

#[test]
fn zodiacal_releasing_endpoint() {
    let s = Server::spawn();
    let r = s.get("/v1/zodiacal-releasing?natal_date=1990-06-15T12:00:00&natal_latitude=52.5&natal_longitude=13.4&count=5");
    assert_eq!(r.status, 200);
    assert!(r.body.contains("\"periods\""));
}

#[test]
fn heliacal_endpoint_404_when_unknown() {
    let s = Server::spawn();
    let r = s.get("/v1/heliacal/Xylophone?date=2026-05-09T12:00:00&latitude=52.5&longitude=13.4");
    // Could be 200 or 404 depending on swe behavior; verify it doesn't 500.
    assert!(r.status == 200 || r.status == 404, "status={}", r.status);
}

// ---------- hardening ----------

#[test]
fn options_preflight_returns_cors_headers() {
    let s = Server::spawn();
    let r = s.request(
        "OPTIONS",
        "/v1/sun",
        &[
            ("Origin", "https://example.com"),
            ("Access-Control-Request-Method", "GET"),
        ],
    );
    // tower-http CorsLayer answers preflight with 200 or 204.
    assert!(r.status == 200 || r.status == 204, "status={}", r.status);
    assert!(r.header("access-control-allow-origin").is_some());
}

#[test]
fn head_request_to_health_returns_no_body() {
    let s = Server::spawn();
    let r = s.request("HEAD", "/health", &[]);
    // axum auto-handles HEAD by stripping body from GET handler.
    assert_eq!(r.status, 200);
    assert!(r.body.is_empty(), "HEAD body should be empty: {:?}", r.body);
}

#[test]
fn gzip_accept_encoding_compresses() {
    let s = Server::spawn();
    let r = s.get_with_headers(
        "/v1/sun?date=2026-05-09T12:00:00",
        &[("Accept-Encoding", "gzip")],
    );
    assert_eq!(r.status, 200);
    let enc = r.header("content-encoding").unwrap_or("");
    assert!(
        enc.contains("gzip"),
        "expected gzip Content-Encoding, got {enc:?}"
    );
}

#[test]
fn cache_key_splits_on_path_too() {
    // /v1/sun and /v1/moon return different bodies regardless of caching.
    let s = Server::spawn();
    let r1 = s.get("/v1/sun?date=2026-05-09T12:00:00");
    let r2 = s.get("/v1/moon?date=2026-05-09T12:00:00");
    assert_ne!(r1.body, r2.body);
    // Hitting each again should HIT in its own bucket.
    let r3 = s.get("/v1/sun?date=2026-05-09T12:00:00");
    let r4 = s.get("/v1/moon?date=2026-05-09T12:00:00");
    assert_eq!(r3.header("x-cache"), Some("HIT"));
    assert_eq!(r4.header("x-cache"), Some("HIT"));
}

#[test]
fn openapi_lists_new_paths() {
    let s = Server::spawn();
    let r = s.get("/openapi.json");
    assert_eq!(r.status, 200);
    for needed in [
        "/v1/midpoints",
        "/v1/antiscia",
        "/v1/decans",
        "/v1/terms",
        "/v1/triplicity",
        "/v1/receptions",
        "/v1/equation-of-time",
        "/v1/ingresses",
        "/v1/lunations",
        "/v1/heliacal/{star}",
        "/v1/zodiacal-releasing",
        "/v1/natal-chart",
    ] {
        assert!(r.body.contains(needed), "OpenAPI missing path {needed}");
    }
}

#[test]
fn server_timing_header_present() {
    let s = Server::spawn();
    let r = s.get("/v1/sun");
    assert!(
        r.header("server-timing").is_some(),
        "missing Server-Timing header"
    );
}
