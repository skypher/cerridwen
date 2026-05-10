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
fn talk(requests: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let bin = mcp_bin();
    let mut child = Command::new(&bin)
        .env("CERRIDWEN_EPHE_PATH", "/home/sky/cerridwen/sweph")
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
