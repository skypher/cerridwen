// SPDX-License-Identifier: MIT AND AGPL-3.0-only

//! End-to-end tests for the cerridwen-mcp binary. Each test spawns the
//! binary as a subprocess, pipes JSON-RPC over stdio, and verifies the
//! protocol-level shape of the responses.
//!
//! The binary path comes from CARGO_BIN_EXE_cerridwen-mcp, which Cargo
//! sets automatically for integration tests when the binary is in scope
//! (achieved by enabling the "mcp" feature in the dev-dependency that
//! produces this test binary — but here we just rely on the bin being
//! built ahead of test execution).

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn mcp_bin() -> std::path::PathBuf {
    // Try the standard location relative to the workspace.
    let exe = std::env::var("CARGO_BIN_EXE_cerridwen-mcp")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/cerridwen-mcp")
        });
    if !exe.exists() {
        panic!(
            "cerridwen-mcp binary not found at {}; build it with \
             `cargo build --features mcp --bin cerridwen-mcp` first",
            exe.display()
        );
    }
    exe
}

/// Run a sequence of JSON-RPC requests against the MCP server and return
/// each response as a parsed Value. Notifications (no id) produce no
/// response and are filtered out.
fn ephe_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("CERRIDWEN_EPHE_PATH") {
        return std::path::PathBuf::from(p);
    }
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("sweph")
}

fn talk(requests: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let bin = mcp_bin();
    let mut child = Command::new(&bin)
        .env("CERRIDWEN_EPHE_PATH", ephe_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cerridwen-mcp");

    let stdin = child.stdin.as_mut().expect("stdin");
    for req in requests {
        writeln!(stdin, "{req}").expect("write request");
    }
    drop(child.stdin.take());

    let stdout = child.stdout.take().expect("stdout");
    let reader = BufReader::new(stdout);
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line.expect("read line");
        if line.is_empty() {
            continue;
        }
        out.push(
            serde_json::from_str::<serde_json::Value>(&line)
                .unwrap_or_else(|e| panic!("bad MCP output: {line} ({e})")),
        );
    }
    let _ = child.wait();
    out
}

#[test]
fn initialize_returns_protocol_info() {
    let resps =
        talk(&[serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}})]);
    assert_eq!(resps.len(), 1);
    let r = &resps[0];
    assert_eq!(r["id"], 1);
    assert_eq!(r["result"]["serverInfo"]["name"], "cerridwen");
    assert!(r["result"]["protocolVersion"]
        .as_str()
        .unwrap()
        .starts_with("2024-"));
}

#[test]
fn tools_list_contains_all_tools() {
    let resps = talk(&[
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    ]);
    let list = &resps[1]["result"]["tools"];
    let names: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for need in [
        "get_sun",
        "get_moon",
        "get_body",
        "get_houses",
        "get_eclipses",
        "get_transits",
        "get_return",
        "get_aspects",
        "get_star",
        "get_events",
        "get_declinations",
        "get_stations",
        "get_planetary_hours",
        "get_arabic_parts",
        "get_profections",
        "get_synastry",
        "get_progressions",
        "get_prenatal_eclipse",
        "get_twilight",
        "get_midpoints",
        "get_antiscia",
        "get_decans",
        "get_terms",
        "get_triplicity",
        "get_receptions",
        "get_equation_of_time",
        "get_ingresses",
        "get_lunations",
        "get_zodiacal_releasing",
        "get_natal_chart",
    ] {
        assert!(
            names.contains(&need),
            "missing tool: {need} (have: {names:?})"
        );
    }
}

#[test]
fn tools_call_get_sun_returns_position() {
    let resps = talk(&[
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"get_sun",
            "arguments":{"date":"2026-05-06T12:00:00"}
        }}),
    ]);
    let r = &resps[1]["result"];
    assert_eq!(r["isError"], false);
    let s = &r["structuredContent"];
    let lon = s["position"]["absolute_degrees"].as_f64().unwrap();
    assert!(
        (45.0..47.0).contains(&lon),
        "Sun longitude {lon} not in expected range"
    );
    assert_eq!(s["position"]["sign"], "Taurus");
}

#[test]
fn tools_call_get_aspects() {
    let resps = talk(&[
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"get_aspects",
            "arguments":{"date":"2026-05-06T12:00:00","orb":5}
        }}),
    ]);
    let s = &resps[1]["result"]["structuredContent"];
    assert_eq!(s["orb"], 5.0);
    assert!(s["aspects"].is_array());
}

#[test]
fn tools_call_get_star_sirius() {
    let resps = talk(&[
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"get_star",
            "arguments":{"name":"Sirius","date":"2026-05-06T12:00:00"}
        }}),
    ]);
    let s = &resps[1]["result"]["structuredContent"];
    assert!(s["name"].as_str().unwrap().contains("Sirius"));
    let lon = s["longitude"].as_f64().unwrap();
    assert!(
        lon > 100.0 && lon < 110.0,
        "Sirius lon {lon} not near Cancer"
    );
}

#[test]
fn tools_call_get_declinations() {
    let resps = talk(&[
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"get_declinations",
            "arguments":{"date":"2026-05-06T12:00:00","orb":1.5}
        }}),
    ]);
    let s = &resps[1]["result"]["structuredContent"];
    assert!(s["declinations"].is_array());
    assert!(s["parallels"].is_array());
}

#[test]
fn tools_call_get_stations_mercury() {
    let resps = talk(&[
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"get_stations",
            "arguments":{"body":"Mercury","date":"2026-01-01T00:00:00","lookahead":400}
        }}),
    ]);
    let s = &resps[1]["result"]["structuredContent"];
    assert_eq!(s["body"], "Mercury");
    let arr = s["stations"].as_array().expect("stations array");
    assert!(arr.len() >= 2, "expected ≥2 stations, got {}", arr.len());
}

#[test]
fn tools_call_get_planetary_hours() {
    let resps = talk(&[
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"get_planetary_hours",
            "arguments":{
                "date":"2026-05-11T00:00:00", "latitude":52.5, "longitude":13.4
            }
        }}),
    ]);
    let s = &resps[1]["result"]["structuredContent"];
    let hours = s["hours"].as_array().expect("hours array");
    assert_eq!(hours.len(), 24);
}

#[test]
fn tools_call_get_arabic_parts_lists_seven_lots() {
    let resps = talk(&[
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"get_arabic_parts",
            "arguments":{
                "date":"2026-05-06T12:00:00", "latitude":52.5, "longitude":13.4
            }
        }}),
    ]);
    let s = &resps[1]["result"]["structuredContent"];
    let parts = s["parts"].as_array().expect("parts");
    let names: Vec<String> = parts
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_string())
        .collect();
    for needed in [
        "Fortune",
        "Spirit",
        "Eros",
        "Necessity",
        "Courage",
        "Victory",
        "Nemesis",
    ] {
        assert!(names.contains(&needed.into()), "{needed} missing");
    }
}

#[test]
fn tools_call_get_profections_age0_house1() {
    let resps = talk(&[
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"get_profections",
            "arguments":{
                "natal_date":"1990-06-15T12:00:00",
                "natal_latitude":52.5, "natal_longitude":13.4,
                "age":0
            }
        }}),
    ]);
    let s = &resps[1]["result"]["structuredContent"];
    assert_eq!(s["house"], 1);
}

#[test]
fn tools_call_get_synastry() {
    let resps = talk(&[
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"get_synastry",
            "arguments":{
                "date_a":"2000-01-01T12:00:00",
                "date_b":"2000-04-01T12:00:00",
                "orb":5
            }
        }}),
    ]);
    let s = &resps[1]["result"]["structuredContent"];
    assert!(s["aspects"].is_array());
}

#[test]
fn tools_call_get_progressions_secondary() {
    let resps = talk(&[
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"get_progressions",
            "arguments":{
                "natal_date":"2000-01-01T12:00:00",
                "date":"2026-01-01T12:00:00",
                "method":"secondary"
            }
        }}),
    ]);
    let s = &resps[1]["result"]["structuredContent"];
    assert!(s["progressed_jd"].as_f64().is_some());
    assert!(s["bodies"].is_array());
}

#[test]
fn tools_call_get_prenatal_eclipse_before_natal() {
    let resps = talk(&[
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"get_prenatal_eclipse",
            "arguments":{"natal_date":"2000-01-01T00:00:00"}
        }}),
    ]);
    let s = &resps[1]["result"]["structuredContent"];
    let natal_jd = s["natal_jd"].as_f64().unwrap();
    let solar_jd = s["solar"]["jd"].as_f64().unwrap();
    let lunar_jd = s["lunar"]["jd"].as_f64().unwrap();
    assert!(solar_jd < natal_jd);
    assert!(lunar_jd < natal_jd);
}

#[test]
fn tools_call_get_twilight_returns_layers() {
    let resps = talk(&[
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"get_twilight",
            "arguments":{
                "date":"2026-05-11T00:00:00", "latitude":52.5, "longitude":13.4
            }
        }}),
    ]);
    let s = &resps[1]["result"]["structuredContent"];
    assert!(s["civil"]["start_iso"].is_string());
    assert!(s["nautical"]["end_iso"].is_string());
    assert!(s["astronomical"]["start_iso"].is_string());
}

#[test]
fn tools_call_round9_tools_return_structures() {
    let resps = talk(&[
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"get_midpoints",
            "arguments":{"date":"2026-05-06T12:00:00","orb":1.5}
        }}),
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
            "name":"get_terms",
            "arguments":{"date":"2026-05-06T12:00:00","system":"egyptian"}
        }}),
        serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{
            "name":"get_ingresses",
            "arguments":{"date":"2026-01-01T00:00:00","count":2}
        }}),
        serde_json::json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{
            "name":"get_zodiacal_releasing",
            "arguments":{
                "natal_date":"1990-06-15T12:00:00",
                "natal_latitude":52.5,
                "natal_longitude":13.4,
                "count":3
            }
        }}),
        serde_json::json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{
            "name":"get_natal_chart",
            "arguments":{
                "date":"1990-06-15T12:00:00",
                "latitude":52.5,
                "longitude":13.4
            }
        }}),
    ]);
    assert!(resps[1]["result"]["structuredContent"]["midpoints"].is_array());
    assert_eq!(
        resps[2]["result"]["structuredContent"]["system"],
        "egyptian"
    );
    assert_eq!(
        resps[3]["result"]["structuredContent"]["ingresses"]
            .as_array()
            .expect("ingresses")
            .len(),
        2
    );
    assert_eq!(
        resps[4]["result"]["structuredContent"]["periods"]
            .as_array()
            .expect("periods")
            .len(),
        3
    );
    let natal = &resps[5]["result"]["structuredContent"];
    assert!(natal["bodies"].is_array());
    assert!(natal["lots"].is_array());
}

#[test]
fn tools_call_get_houses_requires_observer() {
    // Houses are only meaningful with lat/lon; without them this should
    // surface an error.
    let resps = talk(&[
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"get_houses",
            "arguments":{}
        }}),
    ]);
    assert!(
        resps[1].get("error").is_some(),
        "expected error response, got: {}",
        resps[1]
    );
}

#[test]
fn tools_call_unknown_tool_errors() {
    let resps = talk(&[
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"frobnicate","arguments":{}
        }}),
    ]);
    assert!(resps[1].get("error").is_some());
    assert_eq!(resps[1]["error"]["code"], -32602);
}

#[test]
fn unknown_method_returns_method_not_found() {
    let resps =
        talk(&[serde_json::json!({"jsonrpc":"2.0","id":1,"method":"this/method/does/not/exist"})]);
    assert_eq!(resps[0]["error"]["code"], -32601);
}

#[test]
fn notification_produces_no_response() {
    let resps = talk(&[
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"ping"}),
    ]);
    // Two requests have ids; the notification has none — so we expect
    // exactly two responses.
    assert_eq!(resps.len(), 2);
    assert_eq!(resps[0]["id"], 1);
    assert_eq!(resps[1]["id"], 2);
}
