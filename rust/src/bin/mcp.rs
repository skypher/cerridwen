// SPDX-License-Identifier: MIT AND AGPL-3.0-only

//! cerridwen-mcp — Model Context Protocol server speaking JSON-RPC 2.0 over
//! stdio. Lets LLM agents (Claude Code, IDE clients, etc.) call cerridwen
//! as a tool.
//!
//! Each line on stdin is one JSON-RPC request; each line on stdout is one
//! response. Diagnostics go to stderr.

use std::io::{self, BufRead, Write};

use cerridwen::events::{get_events, EventFilter};
use cerridwen::planets::Planet;
use cerridwen::{
    apply_ayanamsha, compute_aspects_at, compute_ayanamsha, compute_houses, compute_moon_data_with,
    compute_sun_data, compute_transits, default_transit_bodies, eclipses_within_period, fixed_star,
    jd2iso, jd_now, next_return, parse_ayanamsha, parse_house_system, parse_jd_or_iso_date_in_tz,
    ActiveTransit, Eclipse, Houses, LatLong, MoonData, MoonOptions, MoonPhaseData, PlanetEvent,
    PlanetLongitude, SunData, VoidOfCourseData,
};
use serde_json::{json, Value};

const PROTOCOL_VERSION: &str = "2024-11-05";

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdin_lock = stdin.lock();
    let mut stdout_lock = stdout.lock();
    let mut line = String::new();

    eprintln!("cerridwen-mcp: ready (protocol {PROTOCOL_VERSION})");

    loop {
        line.clear();
        match stdin_lock.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("cerridwen-mcp: stdin error: {e}");
                break;
            }
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("cerridwen-mcp: invalid JSON: {e}");
                continue;
            }
        };
        if let Some(resp) = handle(&req) {
            let s = resp.to_string();
            if let Err(e) = writeln!(stdout_lock, "{s}") {
                eprintln!("cerridwen-mcp: stdout write failed: {e}");
                break;
            }
            let _ = stdout_lock.flush();
        }
    }
}

fn handle(req: &Value) -> Option<Value> {
    let id = req.get("id").cloned();
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(Value::Null);

    // Notifications carry no id and expect no response.
    let is_notification = id.is_none();

    let result = match method {
        "initialize" => Ok(initialize_response()),
        "notifications/initialized" => return None,
        "tools/list" => Ok(tools_list()),
        "tools/call" => tools_call(&params),
        "ping" => Ok(json!({})),
        other => Err((-32601, format!("method not found: {other}"))),
    };

    if is_notification {
        return None;
    }

    Some(match result {
        Ok(value) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": value,
        }),
        Err((code, message)) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message },
        }),
    })
}

fn initialize_response() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "cerridwen",
            "version": env!("CARGO_PKG_VERSION"),
        }
    })
}

fn tools_list() -> Value {
    json!({
        "tools": [
            tool_def("get_sun", "Sun position, dignity, rise/set if observer given.", json!({
                "type": "object",
                "properties": {
                    "date": { "type": "string", "description": "ISO date or JD; default now" },
                    "tz":   { "type": "string", "description": "IANA timezone name" },
                    "latitude":  { "type": "number" },
                    "longitude": { "type": "number" },
                    "zodiac":    { "type": "string", "enum": ["tropical", "sidereal"] },
                    "ayanamsha": { "type": "string", "description": "lahiri/krishnamurti/..." }
                }
            })),
            tool_def("get_moon", "Moon position, phase, void-of-course, lunation, etc.", json!({
                "type": "object",
                "properties": {
                    "date": { "type": "string" },
                    "tz":   { "type": "string" },
                    "latitude":  { "type": "number" },
                    "longitude": { "type": "number" },
                    "voc_traditional_only": { "type": "boolean" },
                    "zodiac":    { "type": "string" },
                    "ayanamsha": { "type": "string" }
                }
            })),
            tool_def("get_body", "Per-body data (any of Sun..Pluto, lunar nodes, Lilith, Chiron, asteroids).", json!({
                "type": "object",
                "required": ["name"],
                "properties": {
                    "name": { "type": "string", "description": "Sun/moon/mercury/.../north_node/lilith/chiron/ceres/pallas/juno/vesta" },
                    "date": { "type": "string" },
                    "tz":   { "type": "string" },
                    "latitude":  { "type": "number" },
                    "longitude": { "type": "number" },
                    "zodiac":    { "type": "string" },
                    "ayanamsha": { "type": "string" }
                }
            })),
            tool_def("get_houses", "House cusps and angle points for a moment+observer.", json!({
                "type": "object",
                "required": ["latitude", "longitude"],
                "properties": {
                    "date": { "type": "string" },
                    "tz":   { "type": "string" },
                    "latitude":  { "type": "number" },
                    "longitude": { "type": "number" },
                    "house_system": { "type": "string", "description": "P/K/W/O/R/C/A/E/M/T/V/B/U/Y/X/H/N/D" }
                }
            })),
            tool_def("get_eclipses", "Solar/lunar eclipse predictions in a date range.", json!({
                "type": "object",
                "properties": {
                    "date_start": { "type": "string" },
                    "date_end":   { "type": "string" },
                    "lookahead":  { "type": "number", "description": "Days forward from date_start" },
                    "type":       { "type": "string", "enum": ["solar", "lunar", "both"] },
                    "limit":      { "type": "integer" },
                    "tz":         { "type": "string" }
                }
            })),
            tool_def("get_transits", "Active major aspects from transiting planets to a natal chart.", json!({
                "type": "object",
                "required": ["natal_date"],
                "properties": {
                    "natal_date":   { "type": "string" },
                    "transit_date": { "type": "string", "description": "Default now" },
                    "orb":          { "type": "number", "description": "Degrees, default 1.5" },
                    "tz":           { "type": "string" }
                }
            })),
            tool_def("get_return", "Next solar/lunar/planetary return.", json!({
                "type": "object",
                "required": ["body", "natal_date"],
                "properties": {
                    "body":       { "type": "string" },
                    "natal_date": { "type": "string" },
                    "start_date": { "type": "string", "description": "Default now" },
                    "tz":         { "type": "string" }
                }
            })),
            tool_def("get_aspects", "Instantaneous major aspects between every pair of planets at the requested moment.", json!({
                "type": "object",
                "properties": {
                    "date": { "type": "string" },
                    "tz":   { "type": "string" },
                    "orb":  { "type": "number", "description": "Degrees, default 5" }
                }
            })),
            tool_def("get_star", "Fixed-star position (Sirius, Vega, Spica, Regulus, Algol, ...) from the bundled catalog.", json!({
                "type": "object",
                "required": ["name"],
                "properties": {
                    "name": { "type": "string", "description": "Sirius, Vega, Spica, Regulus, Algol, Polaris, Aldebaran, Antares, ..." },
                    "date": { "type": "string" },
                    "tz":   { "type": "string" },
                    "zodiac": { "type": "string" },
                    "ayanamsha": { "type": "string" }
                }
            })),
            tool_def("get_events", "Database-backed events (aspects, ingresses, retrogrades). Requires CERRIDWEN_EVENTS_DB env var.", json!({
                "type": "object",
                "properties": {
                    "date_start": { "type": "string" },
                    "date_end":   { "type": "string" },
                    "lookahead":  { "type": "number" },
                    "types":      { "type": "string", "description": "Comma-separated" },
                    "planets":    { "type": "string", "description": "Comma-separated" },
                    "limit":      { "type": "integer" },
                    "tz":         { "type": "string" }
                }
            })),
            tool_def("get_declinations", "Declinations of all major bodies plus parallel/contraparallel grid.", json!({
                "type": "object",
                "properties": {
                    "date": { "type": "string" },
                    "tz":   { "type": "string" },
                    "orb":  { "type": "number", "description": "Degrees, default 1" },
                    "include_nodes": { "type": "boolean" },
                    "include_asteroids": { "type": "boolean" }
                }
            })),
            tool_def("get_stations", "Upcoming retrograde / direct stations for one body.", json!({
                "type": "object",
                "required": ["body"],
                "properties": {
                    "body":      { "type": "string" },
                    "date":      { "type": "string", "description": "Search start; default now" },
                    "lookahead": { "type": "number", "description": "Days; default 730" },
                    "limit":     { "type": "integer", "description": "Max results; default 8" },
                    "tz":        { "type": "string" }
                }
            })),
            tool_def("get_planetary_hours", "Chaldean planetary hours for the day at a given moment+observer.", json!({
                "type": "object",
                "required": ["latitude", "longitude"],
                "properties": {
                    "date":      { "type": "string" },
                    "tz":        { "type": "string" },
                    "latitude":  { "type": "number" },
                    "longitude": { "type": "number" }
                }
            })),
            tool_def("get_arabic_parts", "Hellenistic lots (Fortune, Spirit, Eros, Necessity, Courage, Victory, Nemesis).", json!({
                "type": "object",
                "required": ["latitude", "longitude"],
                "properties": {
                    "date":      { "type": "string" },
                    "tz":        { "type": "string" },
                    "latitude":  { "type": "number" },
                    "longitude": { "type": "number" },
                    "house_system": { "type": "string" }
                }
            })),
            tool_def("get_profections", "Annual profections — house, sign, time-lord for a given age.", json!({
                "type": "object",
                "required": ["natal_date", "natal_latitude", "natal_longitude", "age"],
                "properties": {
                    "natal_date":      { "type": "string" },
                    "natal_latitude":  { "type": "number" },
                    "natal_longitude": { "type": "number" },
                    "age":             { "type": "integer" },
                    "tz":              { "type": "string" }
                }
            })),
            tool_def("get_synastry", "Inter-aspect grid between two charts.", json!({
                "type": "object",
                "required": ["date_a", "date_b"],
                "properties": {
                    "date_a": { "type": "string" },
                    "date_b": { "type": "string" },
                    "orb":    { "type": "number", "description": "Default 4" },
                    "tz":     { "type": "string" }
                }
            })),
            tool_def("get_progressions", "Secondary progressions or solar arc directions to a target date.", json!({
                "type": "object",
                "required": ["natal_date"],
                "properties": {
                    "natal_date": { "type": "string" },
                    "date":       { "type": "string", "description": "Target; default now" },
                    "method":     { "type": "string", "enum": ["secondary", "solar_arc"] },
                    "tz":         { "type": "string" }
                }
            })),
            tool_def("get_prenatal_eclipse", "Last solar + lunar eclipse before a natal date.", json!({
                "type": "object",
                "required": ["natal_date"],
                "properties": {
                    "natal_date": { "type": "string" },
                    "tz":         { "type": "string" }
                }
            })),
            tool_def("get_twilight", "Sunrise, sunset, and civil/nautical/astronomical twilight start/end for a day+observer.", json!({
                "type": "object",
                "required": ["latitude", "longitude"],
                "properties": {
                    "date":      { "type": "string" },
                    "tz":        { "type": "string" },
                    "latitude":  { "type": "number" },
                    "longitude": { "type": "number" }
                }
            })),
            tool_def("get_midpoints", "Every pairwise midpoint plus hits to other planets at harmonics 0/45/90/135/180.", json!({
                "type": "object",
                "properties": {
                    "date": { "type": "string" },
                    "tz":   { "type": "string" },
                    "orb":  { "type": "number", "description": "Default 1.5" }
                }
            })),
            tool_def("get_antiscia", "Antiscia + contra-antiscia per body plus any hits in the chart.", json!({
                "type": "object",
                "properties": {
                    "date": { "type": "string" },
                    "tz":   { "type": "string" },
                    "orb":  { "type": "number", "description": "Default 1" }
                }
            })),
            tool_def("get_decans", "Per-body decan with Triplicity, Chaldean, and Egyptian rulers/indices.", json!({
                "type": "object",
                "properties": {
                    "date": { "type": "string" },
                    "tz":   { "type": "string" }
                }
            })),
            tool_def("get_terms", "Per-body bound ruler (Ptolemaic by default; system=egyptian for the older system).", json!({
                "type": "object",
                "properties": {
                    "date":   { "type": "string" },
                    "tz":     { "type": "string" },
                    "system": { "type": "string", "enum": ["ptolemaic", "egyptian"] }
                }
            })),
            tool_def("get_triplicity", "Dorothean triplicity rulers per body. With observer, marks the active (day/night) ruler.", json!({
                "type": "object",
                "properties": {
                    "date":      { "type": "string" },
                    "tz":        { "type": "string" },
                    "latitude":  { "type": "number" },
                    "longitude": { "type": "number" }
                }
            })),
            tool_def("get_receptions", "Mutual receptions by traditional rulership across the chart.", json!({
                "type": "object",
                "properties": {
                    "date": { "type": "string" },
                    "tz":   { "type": "string" }
                }
            })),
            tool_def("get_equation_of_time", "Apparent solar time minus mean solar time at a JD, in minutes.", json!({
                "type": "object",
                "properties": {
                    "date": { "type": "string" },
                    "tz":   { "type": "string" }
                }
            })),
            tool_def("get_ingresses", "Upcoming cardinal-sign ingresses (equinoxes / solstices).", json!({
                "type": "object",
                "properties": {
                    "date":  { "type": "string" },
                    "tz":    { "type": "string" },
                    "count": { "type": "integer", "description": "Default 4" }
                }
            })),
            tool_def("get_lunations", "New / first-quarter / full / last-quarter moons in a window.", json!({
                "type": "object",
                "properties": {
                    "date_start": { "type": "string" },
                    "date_end":   { "type": "string" },
                    "lookahead":  { "type": "number", "description": "Days forward; default 90" },
                    "tz":         { "type": "string" }
                }
            })),
            tool_def("get_zodiacal_releasing", "Zodiacal Releasing L1 periods from the Lot of Spirit.", json!({
                "type": "object",
                "required": ["natal_date", "natal_latitude", "natal_longitude"],
                "properties": {
                    "natal_date":      { "type": "string" },
                    "natal_latitude":  { "type": "number" },
                    "natal_longitude": { "type": "number" },
                    "count":           { "type": "integer", "description": "Default 12" },
                    "tz":              { "type": "string" }
                }
            })),
            tool_def("get_natal_chart", "Combined natal chart: houses + bodies-with-houses + aspects + Hellenistic lots.", json!({
                "type": "object",
                "required": ["latitude", "longitude"],
                "properties": {
                    "date":         { "type": "string" },
                    "tz":           { "type": "string" },
                    "latitude":     { "type": "number" },
                    "longitude":    { "type": "number" },
                    "house_system": { "type": "string" },
                    "orb":          { "type": "number", "description": "Aspect orb (default 5)" }
                }
            })),
        ]
    })
}

fn tool_def(name: &str, description: &str, schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": schema,
    })
}

fn tools_call(params: &Value) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or((-32602, "missing tool name".to_string()))?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    let result = match name {
        "get_sun" => tool_get_sun(&args)?,
        "get_moon" => tool_get_moon(&args)?,
        "get_body" => tool_get_body(&args)?,
        "get_houses" => tool_get_houses(&args)?,
        "get_eclipses" => tool_get_eclipses(&args)?,
        "get_transits" => tool_get_transits(&args)?,
        "get_return" => tool_get_return(&args)?,
        "get_events" => tool_get_events(&args)?,
        "get_star" => tool_get_star(&args)?,
        "get_aspects" => tool_get_aspects(&args)?,
        "get_declinations" => tool_get_declinations(&args)?,
        "get_stations" => tool_get_stations(&args)?,
        "get_planetary_hours" => tool_get_planetary_hours(&args)?,
        "get_arabic_parts" => tool_get_arabic_parts(&args)?,
        "get_profections" => tool_get_profections(&args)?,
        "get_synastry" => tool_get_synastry(&args)?,
        "get_progressions" => tool_get_progressions(&args)?,
        "get_prenatal_eclipse" => tool_get_prenatal_eclipse(&args)?,
        "get_twilight" => tool_get_twilight(&args)?,
        "get_midpoints" => tool_get_midpoints(&args)?,
        "get_antiscia" => tool_get_antiscia(&args)?,
        "get_decans" => tool_get_decans(&args)?,
        "get_terms" => tool_get_terms(&args)?,
        "get_triplicity" => tool_get_triplicity(&args)?,
        "get_receptions" => tool_get_receptions(&args)?,
        "get_equation_of_time" => tool_get_equation_of_time(&args)?,
        "get_ingresses" => tool_get_ingresses(&args)?,
        "get_lunations" => tool_get_lunations(&args)?,
        "get_zodiacal_releasing" => tool_get_zodiacal_releasing(&args)?,
        "get_natal_chart" => tool_get_natal_chart(&args)?,
        other => return Err((-32602, format!("unknown tool: {other}"))),
    };
    // MCP wraps tool output in a `content` array of blocks.
    Ok(json!({
        "content": [
            { "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default() }
        ],
        "isError": false,
        "structuredContent": result
    }))
}

// -----------------------------------------------------------------------------
// Argument helpers
// -----------------------------------------------------------------------------

fn arg_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}
fn arg_num(args: &Value, key: &str) -> Option<f64> {
    args.get(key).and_then(|v| v.as_f64())
}
fn arg_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(|v| v.as_bool())
}
fn parse_date_arg(args: &Value, key: &str) -> Result<Option<f64>, (i64, String)> {
    let tz = arg_str(args, "tz");
    match arg_str(args, key) {
        Some(s) => parse_jd_or_iso_date_in_tz(s, tz)
            .map(Some)
            .map_err(|e| (-32602, e)),
        None => Ok(None),
    }
}
fn parse_observer(args: &Value) -> Result<Option<LatLong>, (i64, String)> {
    let lat = arg_num(args, "latitude");
    let long = arg_num(args, "longitude");
    match (lat, long) {
        (Some(la), Some(lo)) => Ok(Some(
            LatLong::new(la, lo).map_err(|e| (-32602, e.to_string()))?,
        )),
        (None, None) => Ok(None),
        _ => Err((
            -32602,
            "must specify both latitude and longitude or neither".to_string(),
        )),
    }
}
fn parse_zodiac(args: &Value, jd: f64) -> Result<(f64, &'static str), (i64, String)> {
    let z = arg_str(args, "zodiac").map(|s| s.to_ascii_lowercase());
    match z.as_deref() {
        None | Some("") | Some("tropical") => Ok((0.0, "tropical")),
        Some("sidereal") => {
            let name = arg_str(args, "ayanamsha").unwrap_or("lahiri");
            let (mode, label) = parse_ayanamsha(name)
                .ok_or_else(|| (-32602, format!("unknown ayanamsha: {name}")))?;
            Ok((compute_ayanamsha(jd, mode), label))
        }
        Some(other) => Err((
            -32602,
            format!("zodiac must be tropical or sidereal: {other}"),
        )),
    }
}

fn shift_longitude(p: &PlanetLongitude, ayanamsha_deg: f64) -> PlanetLongitude {
    if ayanamsha_deg == 0.0 {
        *p
    } else {
        PlanetLongitude::new(apply_ayanamsha(p.absolute_degrees, ayanamsha_deg))
    }
}

// -----------------------------------------------------------------------------
// Tool implementations
// -----------------------------------------------------------------------------

fn tool_get_sun(args: &Value) -> Result<Value, (i64, String)> {
    let jd = parse_date_arg(args, "date")?;
    let observer = parse_observer(args)?;
    let data = compute_sun_data(jd, observer);
    let (ayan, ayan_name) = parse_zodiac(args, data.jd)?;
    Ok(sun_data_to_json(&data, ayan, ayan_name))
}

fn tool_get_moon(args: &Value) -> Result<Value, (i64, String)> {
    let jd = parse_date_arg(args, "date")?;
    let observer = parse_observer(args)?;
    let opts = MoonOptions {
        voc_traditional_only: arg_bool(args, "voc_traditional_only").unwrap_or(false),
    };
    let data = compute_moon_data_with(jd, observer, opts);
    let (ayan, ayan_name) = parse_zodiac(args, data.jd)?;
    Ok(moon_data_to_json(&data, ayan, ayan_name))
}

fn tool_get_body(args: &Value) -> Result<Value, (i64, String)> {
    let name_in = arg_str(args, "name").ok_or((-32602, "missing 'name'".to_string()))?;
    let canonical =
        canonical_body_name(name_in).ok_or_else(|| (-32602, format!("unknown body: {name_in}")))?;
    let jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let observer = parse_observer(args)?;
    let planet =
        body_for(canonical, jd).ok_or_else(|| (-32602, format!("unknown body: {name_in}")))?;
    let (ayan, ayan_name) = parse_zodiac(args, jd)?;
    let trop_lon = planet.longitude_at(jd);
    let lon = if ayan != 0.0 {
        apply_ayanamsha(trop_lon, ayan)
    } else {
        trop_lon
    };
    let pos = PlanetLongitude::new(lon);
    let mut o = serde_json::Map::new();
    o.insert("jd".into(), json!(jd));
    o.insert("iso_date".into(), json!(jd2iso(jd)));
    o.insert("zodiac".into(), json!(ayan_name));
    if ayan != 0.0 {
        o.insert("ayanamsha_degrees".into(), json!(ayan));
    }
    o.insert("name".into(), json!(planet.name()));
    o.insert("position".into(), planet_longitude_to_json(&pos));
    o.insert("longitude".into(), json!(lon));
    o.insert("latitude".into(), json!(planet.latitude(None)));
    o.insert("distance".into(), json!(planet.distance(None)));
    o.insert("speed".into(), json!(planet.speed(None)));
    o.insert("is_rx".into(), json!(planet.is_rx(None)));
    o.insert("is_stationing".into(), json!(planet.is_stationing(None)));
    o.insert("illumination".into(), json!(planet.illumination(None)));
    o.insert(
        "mean_orbital_period".into(),
        json!(planet.mean_orbital_period()),
    );
    o.insert(
        "relative_orbital_velocity".into(),
        json!(planet.relative_orbital_velocity()),
    );
    if let Some(ev) = planet.next_event() {
        o.insert("next_event".into(), planet_event_to_json(&ev));
    }
    if let Some(ll) = observer {
        let with_observer = Planet::new(planet.id, Some(jd), Some(ll));
        o.insert(
            "next_rise".into(),
            planet_event_to_json(&with_observer.next_rise()),
        );
        o.insert(
            "next_set".into(),
            planet_event_to_json(&with_observer.next_set()),
        );
    }
    Ok(Value::Object(o))
}

fn tool_get_houses(args: &Value) -> Result<Value, (i64, String)> {
    let observer =
        parse_observer(args)?.ok_or((-32602, "latitude and longitude are required".to_string()))?;
    let jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let system = match arg_str(args, "house_system") {
        Some(s) => {
            parse_house_system(s).ok_or_else(|| (-32602, format!("unknown house_system: {s}")))?
        }
        None => 'P',
    };
    let h = compute_houses(jd, observer.lat, observer.long, system);
    Ok(houses_to_json(&h, jd))
}

fn tool_get_eclipses(args: &Value) -> Result<Value, (i64, String)> {
    let jd_start = parse_date_arg(args, "date_start")?.unwrap_or_else(jd_now);
    let jd_end = match (arg_str(args, "date_end"), arg_num(args, "lookahead")) {
        (Some(_), Some(_)) => {
            return Err((
                -32602,
                "specify date_end or lookahead, not both".to_string(),
            ))
        }
        (Some(s), None) => {
            parse_jd_or_iso_date_in_tz(s, arg_str(args, "tz")).map_err(|e| (-32602, e))?
        }
        (None, Some(d)) if d >= 0.0 => jd_start + d,
        (None, Some(_)) => return Err((-32602, "lookahead must be non-negative".to_string())),
        (None, None) => jd_start + 365.0,
    };
    let kind = arg_str(args, "type").map(|s| s.to_ascii_lowercase());
    let (solar, lunar) = match kind.as_deref() {
        None | Some("both") | Some("any") => (true, true),
        Some("solar") => (true, false),
        Some("lunar") => (false, true),
        Some(other) => return Err((-32602, format!("type must be solar/lunar/both: {other}"))),
    };
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
    let eclipses = eclipses_within_period(jd_start, jd_end, solar, lunar, limit);
    Ok(json!(eclipses
        .iter()
        .map(eclipse_to_json)
        .collect::<Vec<_>>()))
}

fn tool_get_transits(args: &Value) -> Result<Value, (i64, String)> {
    let natal_jd = parse_date_arg(args, "natal_date")?
        .ok_or((-32602, "natal_date is required".to_string()))?;
    let transit_jd = parse_date_arg(args, "transit_date")?.unwrap_or_else(jd_now);
    let orb = arg_num(args, "orb").unwrap_or(1.5);
    if orb <= 0.0 || orb >= 30.0 {
        return Err((-32602, "orb must be in (0, 30)".to_string()));
    }
    let bodies = default_transit_bodies();
    let active = compute_transits(natal_jd, transit_jd, &bodies, orb);
    Ok(json!({
        "natal_jd": natal_jd,
        "natal_iso": jd2iso(natal_jd),
        "transit_jd": transit_jd,
        "transit_iso": jd2iso(transit_jd),
        "orb": orb,
        "active": active.iter().map(transit_to_json).collect::<Vec<_>>(),
    }))
}

fn tool_get_return(args: &Value) -> Result<Value, (i64, String)> {
    let body_name = arg_str(args, "body").ok_or((-32602, "body is required".to_string()))?;
    let canonical = canonical_body_name(body_name)
        .ok_or_else(|| (-32602, format!("unknown body: {body_name}")))?;
    let body_id = body_for(canonical, 0.0)
        .ok_or_else(|| (-32602, format!("unknown body: {body_name}")))?
        .id;
    let natal_jd = parse_date_arg(args, "natal_date")?
        .ok_or((-32602, "natal_date is required".to_string()))?;
    let start_jd = parse_date_arg(args, "start_date")?.unwrap_or_else(jd_now);
    let return_jd = next_return(body_id, natal_jd, start_jd)
        .ok_or_else(|| (-32603, format!("no return found for {canonical}")))?;
    Ok(json!({
        "body": canonical,
        "natal_jd": natal_jd,
        "natal_iso": jd2iso(natal_jd),
        "search_from_jd": start_jd,
        "return_jd": return_jd,
        "return_iso": jd2iso(return_jd),
        "delta_days": return_jd - start_jd,
    }))
}

fn tool_get_aspects(args: &Value) -> Result<Value, (i64, String)> {
    let jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let orb = arg_num(args, "orb").unwrap_or(5.0);
    if orb <= 0.0 || orb >= 30.0 {
        return Err((-32602, "orb must be in (0, 30)".to_string()));
    }
    let bodies = default_transit_bodies();
    let aspects = compute_aspects_at(jd, &bodies, orb);
    Ok(json!({
        "jd": jd,
        "iso_date": jd2iso(jd),
        "orb": orb,
        "aspects": aspects.iter().map(|a| json!({
            "body_a": a.body_a,
            "body_b": a.body_b,
            "aspect": a.aspect_name,
            "mode": a.aspect_mode,
            "exact_angle": a.exact_angle,
            "orb_distance": a.orb_distance,
            "applying": a.applying,
        })).collect::<Vec<_>>()
    }))
}

fn tool_get_star(args: &Value) -> Result<Value, (i64, String)> {
    let name = arg_str(args, "name").ok_or((-32602, "name is required".to_string()))?;
    let jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let star = fixed_star(name, jd).map_err(|e| (-32603, e))?;
    let (ayan, ayan_name) = parse_zodiac(args, jd)?;
    let lon = if ayan != 0.0 {
        apply_ayanamsha(star.longitude, ayan)
    } else {
        star.longitude
    };
    let pos = PlanetLongitude::new(lon);
    Ok(json!({
        "name": star.name,
        "jd": jd,
        "iso_date": jd2iso(jd),
        "zodiac": ayan_name,
        "ayanamsha_degrees": if ayan != 0.0 { Some(ayan) } else { None },
        "position": planet_longitude_to_json(&pos),
        "longitude": lon,
        "ecliptic_latitude": star.latitude,
        "distance_au": star.distance,
        "speed": star.speed,
        "magnitude": star.magnitude,
    }))
}

fn tool_get_events(args: &Value) -> Result<Value, (i64, String)> {
    let dbfile = std::env::var("CERRIDWEN_EVENTS_DB").unwrap_or_else(|_| "events.db".into());
    let jd_start = parse_date_arg(args, "date_start")?.unwrap_or_else(jd_now);
    let jd_end = match (arg_str(args, "date_end"), arg_num(args, "lookahead")) {
        (Some(_), Some(_)) => {
            return Err((
                -32602,
                "specify date_end or lookahead, not both".to_string(),
            ))
        }
        (Some(s), None) => {
            parse_jd_or_iso_date_in_tz(s, arg_str(args, "tz")).map_err(|e| (-32602, e))?
        }
        (None, Some(d)) if d >= 0.0 => jd_start + d,
        (None, Some(_)) => return Err((-32602, "lookahead must be non-negative".to_string())),
        (None, None) => jd_start + 30.0,
    };
    let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(30);
    let split = |key: &str| -> Option<Vec<String>> {
        arg_str(args, key).map(|s| s.split(',').map(|x| x.to_string()).collect())
    };
    let filter = EventFilter {
        types: split("types"),
        subtypes: split("subtypes"),
        planets: split("planets"),
        datas: split("datas"),
    };
    let events = get_events(&dbfile, jd_start, jd_end, limit, &filter)
        .map_err(|e| (-32603, format!("event query failed: {e}")))?;
    let arr: Vec<Value> = events
        .iter()
        .map(|e| {
            json!({
                "jd": e.jd,
                "iso_date": e.iso_date,
                "type": e.r#type,
                "subtype": e.subtype,
                "planet": e.planet,
                "data": e.data,
                "delta_days": e.delta_days,
            })
        })
        .collect();
    Ok(Value::Array(arr))
}

// -----------------------------------------------------------------------------
// Astrology-tool helpers
// -----------------------------------------------------------------------------

fn snapshot_longitudes(jd: f64) -> Vec<(String, f64)> {
    use cerridwen::planets::*;
    [
        ("Sun", SE_SUN),
        ("Moon", SE_MOON),
        ("Mercury", SE_MERCURY),
        ("Venus", SE_VENUS),
        ("Mars", SE_MARS),
        ("Jupiter", SE_JUPITER),
        ("Saturn", SE_SATURN),
        ("Uranus", SE_URANUS),
        ("Neptune", SE_NEPTUNE),
        ("Pluto", SE_PLUTO),
    ]
    .into_iter()
    .map(|(n, id)| {
        let lon = swisseph::swe::calc_ut(jd, id as u32, 2)
            .map(|r| r.out[0])
            .unwrap_or(f64::NAN);
        (n.to_string(), lon)
    })
    .collect()
}

fn tool_get_declinations(args: &Value) -> Result<Value, (i64, String)> {
    let jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let orb = arg_num(args, "orb").unwrap_or(1.0);
    use cerridwen::planets::*;
    let mut bodies: Vec<(String, i32)> = vec![
        ("Sun".into(), SE_SUN),
        ("Moon".into(), SE_MOON),
        ("Mercury".into(), SE_MERCURY),
        ("Venus".into(), SE_VENUS),
        ("Mars".into(), SE_MARS),
        ("Jupiter".into(), SE_JUPITER),
        ("Saturn".into(), SE_SATURN),
        ("Uranus".into(), SE_URANUS),
        ("Neptune".into(), SE_NEPTUNE),
        ("Pluto".into(), SE_PLUTO),
    ];
    if arg_bool(args, "include_nodes").unwrap_or(false) {
        bodies.push(("Mean Node".into(), SE_MEAN_NODE));
        bodies.push(("True Node".into(), SE_TRUE_NODE));
    }
    if arg_bool(args, "include_asteroids").unwrap_or(false) {
        for (n, id) in [
            ("Chiron", SE_CHIRON),
            ("Ceres", SE_CERES),
            ("Pallas", SE_PALLAS),
            ("Juno", SE_JUNO),
            ("Vesta", SE_VESTA),
        ] {
            bodies.push((n.into(), id));
        }
    }
    let decs: Vec<Value> = bodies
        .iter()
        .map(|(name, id)| {
            json!({"body": name, "declination": cerridwen::astrology::declination(*id, jd)})
        })
        .collect();
    let parallels = cerridwen::astrology::declination_aspects(&bodies, jd, orb);
    let pj: Vec<Value> = parallels
        .iter()
        .map(|p| json!({"a": p.a, "b": p.b, "kind": p.kind.label(), "orb": p.orb}))
        .collect();
    Ok(json!({
        "jd": jd, "iso_date": jd2iso(jd),
        "orb": orb,
        "declinations": decs,
        "parallels": pj,
        "moon_out_of_bounds": cerridwen::astrology::moon_out_of_bounds(jd),
    }))
}

fn tool_get_stations(args: &Value) -> Result<Value, (i64, String)> {
    let name_in = arg_str(args, "body").ok_or((-32602, "missing 'body'".to_string()))?;
    let canonical =
        canonical_body_name(name_in).ok_or_else(|| (-32602, format!("unknown body: {name_in}")))?;
    let planet =
        body_for(canonical, 0.0).ok_or_else(|| (-32602, format!("unknown body: {name_in}")))?;
    let start_jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let lookahead = arg_num(args, "lookahead").unwrap_or(730.0);
    let max = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(8) as usize;
    let stations = cerridwen::astrology::upcoming_stations(planet.id, start_jd, lookahead, max);
    let arr: Vec<Value> = stations
        .iter()
        .map(|s| {
            json!({
                "jd": s.jd, "iso_date": s.iso_date,
                "kind": s.kind.label(),
                "longitude": s.longitude,
            })
        })
        .collect();
    Ok(json!({
        "body": canonical,
        "start_jd": start_jd,
        "stations": arr,
    }))
}

fn tool_get_planetary_hours(args: &Value) -> Result<Value, (i64, String)> {
    let jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let observer = parse_observer(args)?.ok_or((-32602, "latitude/longitude required".into()))?;
    let hours = cerridwen::astrology::planetary_hours(jd, &observer);
    let arr: Vec<Value> = hours
        .iter()
        .map(|h| {
            json!({
                "index": h.index, "kind": h.kind, "ruler": h.ruler,
                "start_iso": jd2iso(h.start_jd), "end_iso": jd2iso(h.end_jd),
            })
        })
        .collect();
    Ok(json!({"jd": jd, "iso_date": jd2iso(jd), "hours": arr}))
}

fn tool_get_arabic_parts(args: &Value) -> Result<Value, (i64, String)> {
    let jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let observer = parse_observer(args)?.ok_or((-32602, "latitude/longitude required".into()))?;
    let system = match arg_str(args, "house_system") {
        Some(s) => {
            parse_house_system(s).ok_or_else(|| (-32602, format!("unknown house_system: {s}")))?
        }
        None => 'P',
    };
    let h = compute_houses(jd, observer.lat, observer.long, system);
    use cerridwen::planets::*;
    let lon = |id: i32| -> f64 {
        swisseph::swe::calc_ut(jd, id as u32, 2)
            .map(|r| r.out[0])
            .unwrap_or(f64::NAN)
    };
    let sun = lon(SE_SUN);
    let moon = lon(SE_MOON);
    let mercury = lon(SE_MERCURY);
    let venus = lon(SE_VENUS);
    let mars = lon(SE_MARS);
    let jupiter = lon(SE_JUPITER);
    let saturn = lon(SE_SATURN);
    let dsc = (h.ascendant + 180.0).rem_euclid(360.0);
    let is_day = (sun - h.ascendant).rem_euclid(360.0) >= (dsc - h.ascendant).rem_euclid(360.0);
    let parts = cerridwen::astrology::arabic_parts(
        h.ascendant,
        sun,
        moon,
        mercury,
        venus,
        mars,
        jupiter,
        saturn,
        is_day,
    );
    let arr: Vec<Value> = parts
        .iter()
        .map(|p| {
            json!({
                "name": p.name, "longitude": p.longitude, "formula": p.formula,
            })
        })
        .collect();
    Ok(
        json!({"jd": jd, "iso_date": jd2iso(jd), "ascendant": h.ascendant, "is_day": is_day, "parts": arr}),
    )
}

fn tool_get_profections(args: &Value) -> Result<Value, (i64, String)> {
    let natal_jd =
        parse_date_arg(args, "natal_date")?.ok_or((-32602, "missing natal_date".to_string()))?;
    let lat =
        arg_num(args, "natal_latitude").ok_or((-32602, "missing natal_latitude".to_string()))?;
    let long =
        arg_num(args, "natal_longitude").ok_or((-32602, "missing natal_longitude".to_string()))?;
    let age = args
        .get("age")
        .and_then(|v| v.as_u64())
        .ok_or((-32602, "missing age".to_string()))? as u32;
    let h = compute_houses(natal_jd, lat, long, 'P');
    let p = cerridwen::astrology::profection(h.ascendant, age);
    Ok(json!({
        "natal_jd": natal_jd, "natal_ascendant": h.ascendant,
        "age": p.age, "house": p.house, "sign": p.sign, "lord": p.lord,
    }))
}

fn tool_get_synastry(args: &Value) -> Result<Value, (i64, String)> {
    let jd_a = parse_date_arg(args, "date_a")?.ok_or((-32602, "missing date_a".to_string()))?;
    let jd_b = parse_date_arg(args, "date_b")?.ok_or((-32602, "missing date_b".to_string()))?;
    let orb = arg_num(args, "orb").unwrap_or(4.0);
    let aspects =
        cerridwen::astrology::synastry(&snapshot_longitudes(jd_a), &snapshot_longitudes(jd_b), orb);
    let arr: Vec<Value> = aspects
        .iter()
        .map(|sa| {
            json!({
                "a": sa.a, "b": sa.b, "aspect": sa.aspect,
                "orb": sa.orb, "angle_a_to_b": sa.angle_a_to_b,
            })
        })
        .collect();
    Ok(json!({"jd_a": jd_a, "jd_b": jd_b, "orb": orb, "aspects": arr}))
}

fn tool_get_progressions(args: &Value) -> Result<Value, (i64, String)> {
    let natal_jd =
        parse_date_arg(args, "natal_date")?.ok_or((-32602, "missing natal_date".to_string()))?;
    let target_jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let method = arg_str(args, "method").unwrap_or("secondary");
    match method {
        "secondary" => {
            let pj = cerridwen::astrology::progressed_jd(natal_jd, target_jd);
            let bodies = snapshot_longitudes(pj);
            let arr: Vec<Value> = bodies
                .iter()
                .map(|(n, l)| json!({"name": n, "longitude": l}))
                .collect();
            Ok(
                json!({"method": "secondary", "progressed_jd": pj, "progressed_iso": jd2iso(pj), "bodies": arr}),
            )
        }
        "solar_arc" => {
            let arc = cerridwen::astrology::solar_arc_offset(natal_jd, target_jd);
            let natal = snapshot_longitudes(natal_jd);
            let arr: Vec<Value> = natal
                .iter()
                .map(|(n, l)| {
                    json!({
                        "name": n, "longitude": (l + arc).rem_euclid(360.0), "delta_deg": arc,
                    })
                })
                .collect();
            Ok(json!({"method": "solar_arc", "arc_deg": arc, "bodies": arr}))
        }
        other => Err((-32602, format!("unknown method: {other}"))),
    }
}

fn tool_get_prenatal_eclipse(args: &Value) -> Result<Value, (i64, String)> {
    let natal_jd =
        parse_date_arg(args, "natal_date")?.ok_or((-32602, "missing natal_date".to_string()))?;
    let solar = cerridwen::astrology::pre_natal_solar_eclipse(natal_jd);
    let lunar = cerridwen::astrology::pre_natal_lunar_eclipse(natal_jd);
    let to_json = |e: &cerridwen::Eclipse| -> Value {
        json!({"jd": e.max_jd, "iso_date": jd2iso(e.max_jd),
               "kind": format!("{:?}", e.kind), "central": e.central})
    };
    Ok(json!({
        "natal_jd": natal_jd,
        "solar": solar.as_ref().map(to_json),
        "lunar": lunar.as_ref().map(to_json),
    }))
}

fn tool_get_twilight(args: &Value) -> Result<Value, (i64, String)> {
    let jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let observer = parse_observer(args)?.ok_or((-32602, "latitude/longitude required".into()))?;
    let t = cerridwen::astrology::twilight_times(jd, &observer);
    Ok(json!({
        "jd": jd, "iso_date": jd2iso(jd),
        "sunrise": jd2iso(t.sunrise), "sunset": jd2iso(t.sunset),
        "civil": {"start_iso": jd2iso(t.civil_dawn), "end_iso": jd2iso(t.civil_dusk)},
        "nautical": {"start_iso": jd2iso(t.nautical_dawn), "end_iso": jd2iso(t.nautical_dusk)},
        "astronomical": {"start_iso": jd2iso(t.astronomical_dawn), "end_iso": jd2iso(t.astronomical_dusk)},
    }))
}

// -----------------------------------------------------------------------------
// Round 9 tools
// -----------------------------------------------------------------------------

fn tool_get_midpoints(args: &Value) -> Result<Value, (i64, String)> {
    let jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let orb = arg_num(args, "orb").unwrap_or(1.5);
    let chart = snapshot_longitudes(jd);
    let pairs = cerridwen::astrology::midpoints(&chart);
    let mps: Vec<Value> = pairs
        .iter()
        .map(|(a, b, m)| json!({"a": a, "b": b, "midpoint": m}))
        .collect();
    let hits = cerridwen::astrology::midpoint_hits(&chart, orb);
    let h: Vec<Value> = hits
        .iter()
        .map(|h| {
            json!({
                "a": h.a, "b": h.b, "hit_by": h.hit_by,
                "angle": h.angle, "orb": h.orb, "midpoint": h.midpoint,
            })
        })
        .collect();
    Ok(json!({"jd": jd, "orb": orb, "midpoints": mps, "hits": h}))
}

fn tool_get_antiscia(args: &Value) -> Result<Value, (i64, String)> {
    let jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let orb = arg_num(args, "orb").unwrap_or(1.0);
    let chart = snapshot_longitudes(jd);
    let bodies: Vec<Value> = chart
        .iter()
        .map(|(n, lon)| {
            json!({
                "body": n,
                "longitude": lon,
                "antiscion": cerridwen::astrology::antiscion(*lon),
                "contra_antiscion": cerridwen::astrology::contra_antiscion(*lon),
            })
        })
        .collect();
    let hits = cerridwen::astrology::antiscia_hits(&chart, orb);
    let h: Vec<Value> = hits
        .iter()
        .map(|h| {
            json!({
                "body": h.body, "antiscion": h.antiscion,
                "hit_by": h.hit_by, "orb": h.orb, "kind": h.kind,
            })
        })
        .collect();
    Ok(json!({"jd": jd, "orb": orb, "bodies": bodies, "hits": h}))
}

fn tool_get_decans(args: &Value) -> Result<Value, (i64, String)> {
    let jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let chart = snapshot_longitudes(jd);
    let arr: Vec<Value> = chart
        .iter()
        .map(|(n, lon)| {
            let d = cerridwen::astrology::decan_for(*lon);
            json!({
                "body": n, "longitude": lon,
                "decan_in_sign": d.decan_in_sign,
                "egyptian_index": d.egyptian_index,
                "triplicity_ruler": d.triplicity_ruler,
                "chaldean_ruler": d.chaldean_ruler,
            })
        })
        .collect();
    Ok(json!({"jd": jd, "bodies": arr}))
}

fn tool_get_terms(args: &Value) -> Result<Value, (i64, String)> {
    let jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let system = arg_str(args, "system").unwrap_or("ptolemaic");
    let chart = snapshot_longitudes(jd);
    let arr: Vec<Value> = chart
        .iter()
        .map(|(n, lon)| {
            let term = if system == "egyptian" {
                cerridwen::astrology::egyptian_term(*lon)
            } else {
                cerridwen::astrology::ptolemaic_term(*lon)
            };
            json!({"body": n, "longitude": lon, "term_ruler": term})
        })
        .collect();
    Ok(json!({"jd": jd, "system": system, "bodies": arr}))
}

fn tool_get_triplicity(args: &Value) -> Result<Value, (i64, String)> {
    let jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let observer = parse_observer(args)?;
    let chart = snapshot_longitudes(jd);
    let is_day = observer.map(|o| {
        let sun_lon = swisseph::swe::calc_ut(jd, 0, 2)
            .map(|r| r.out[0])
            .unwrap_or(0.0);
        let h = compute_houses(jd, o.lat, o.long, 'P');
        let dsc = (h.ascendant + 180.0).rem_euclid(360.0);
        (sun_lon - h.ascendant).rem_euclid(360.0) >= (dsc - h.ascendant).rem_euclid(360.0)
    });
    let arr: Vec<Value> = chart
        .iter()
        .map(|(n, lon)| {
            let t = cerridwen::astrology::triplicity_rulers(*lon);
            let active = match is_day {
                Some(true) => Some(t.day),
                Some(false) => Some(t.night),
                None => None,
            };
            json!({
                "body": n, "longitude": lon,
                "day_ruler": t.day, "night_ruler": t.night,
                "participating_ruler": t.participating,
                "active_ruler": active,
            })
        })
        .collect();
    Ok(json!({"jd": jd, "is_day": is_day, "bodies": arr}))
}

fn tool_get_receptions(args: &Value) -> Result<Value, (i64, String)> {
    let jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let chart = snapshot_longitudes(jd);
    let recs = cerridwen::astrology::receptions(&chart);
    let arr: Vec<Value> = recs
        .iter()
        .map(|r| {
            json!({
                "a": r.a, "b": r.b, "kind": r.kind,
            })
        })
        .collect();
    Ok(json!({"jd": jd, "receptions": arr}))
}

fn tool_get_equation_of_time(args: &Value) -> Result<Value, (i64, String)> {
    let jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let m = cerridwen::astrology::equation_of_time_minutes(jd);
    Ok(json!({"jd": jd, "iso_date": jd2iso(jd), "equation_of_time_minutes": m}))
}

fn tool_get_ingresses(args: &Value) -> Result<Value, (i64, String)> {
    let start_jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(4) as usize;
    let list = cerridwen::astrology::upcoming_cardinal_ingresses(start_jd, count);
    let arr: Vec<Value> = list
        .iter()
        .map(|i| {
            json!({
                "jd": i.jd, "iso_date": i.iso_date,
                "sign": i.sign, "kind": i.kind,
            })
        })
        .collect();
    Ok(json!({"start_jd": start_jd, "ingresses": arr}))
}

fn tool_get_lunations(args: &Value) -> Result<Value, (i64, String)> {
    let start = parse_date_arg(args, "date_start")?
        .or(parse_date_arg(args, "date")?)
        .unwrap_or_else(jd_now);
    let lookahead = arg_num(args, "lookahead").unwrap_or(90.0);
    let end = match arg_str(args, "date_end") {
        Some(s) => parse_jd_or_iso_date_in_tz(s, arg_str(args, "tz")).map_err(|e| (-32602, e))?,
        None => start + lookahead,
    };
    let list = cerridwen::astrology::lunations_in_window(start, end);
    let arr: Vec<Value> = list
        .iter()
        .map(|l| {
            json!({
                "jd": l.jd, "iso_date": l.iso_date,
                "kind": l.kind, "moon_longitude": l.moon_longitude,
            })
        })
        .collect();
    Ok(json!({"start_jd": start, "end_jd": end, "lunations": arr}))
}

fn tool_get_zodiacal_releasing(args: &Value) -> Result<Value, (i64, String)> {
    let natal_jd =
        parse_date_arg(args, "natal_date")?.ok_or((-32602, "missing natal_date".to_string()))?;
    let lat =
        arg_num(args, "natal_latitude").ok_or((-32602, "missing natal_latitude".to_string()))?;
    let long =
        arg_num(args, "natal_longitude").ok_or((-32602, "missing natal_longitude".to_string()))?;
    let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(12) as usize;
    let h = compute_houses(natal_jd, lat, long, 'P');
    use cerridwen::planets::*;
    let sun = swisseph::swe::calc_ut(natal_jd, SE_SUN as u32, 2)
        .map(|r| r.out[0])
        .unwrap_or(0.0);
    let moon = swisseph::swe::calc_ut(natal_jd, SE_MOON as u32, 2)
        .map(|r| r.out[0])
        .unwrap_or(0.0);
    let dsc = (h.ascendant + 180.0).rem_euclid(360.0);
    let is_day = (sun - h.ascendant).rem_euclid(360.0) >= (dsc - h.ascendant).rem_euclid(360.0);
    let spirit = if is_day {
        (h.ascendant + sun - moon).rem_euclid(360.0)
    } else {
        (h.ascendant + moon - sun).rem_euclid(360.0)
    };
    let periods = cerridwen::astrology::zodiacal_releasing_l1(spirit, count);
    let arr: Vec<Value> = periods
        .iter()
        .map(|p| {
            json!({
                "level": p.level, "sign": p.sign, "lord": p.lord,
                "years": p.years,
                "start_year_offset": p.start_year_offset,
                "end_year_offset": p.end_year_offset,
            })
        })
        .collect();
    Ok(json!({"natal_jd": natal_jd, "lot_spirit": spirit, "is_day": is_day, "periods": arr}))
}

fn tool_get_natal_chart(args: &Value) -> Result<Value, (i64, String)> {
    let jd = parse_date_arg(args, "date")?.unwrap_or_else(jd_now);
    let observer = parse_observer(args)?.ok_or((-32602, "latitude/longitude required".into()))?;
    let system = match arg_str(args, "house_system") {
        Some(s) => {
            parse_house_system(s).ok_or_else(|| (-32602, format!("unknown house_system: {s}")))?
        }
        None => 'P',
    };
    let h = compute_houses(jd, observer.lat, observer.long, system);
    let chart = snapshot_longitudes(jd);
    let bodies: Vec<Value> = chart
        .iter()
        .map(|(n, lon)| {
            json!({
                "name": n, "longitude": lon,
                "house": cerridwen::astrology::house_of_longitude(*lon, jd, &observer, system),
            })
        })
        .collect();
    use cerridwen::planets::*;
    let lon = |id: i32| {
        swisseph::swe::calc_ut(jd, id as u32, 2)
            .map(|r| r.out[0])
            .unwrap_or(f64::NAN)
    };
    let sun = lon(SE_SUN);
    let moon = lon(SE_MOON);
    let mercury = lon(SE_MERCURY);
    let venus = lon(SE_VENUS);
    let mars = lon(SE_MARS);
    let jupiter = lon(SE_JUPITER);
    let saturn = lon(SE_SATURN);
    let dsc = (h.ascendant + 180.0).rem_euclid(360.0);
    let is_day = (sun - h.ascendant).rem_euclid(360.0) >= (dsc - h.ascendant).rem_euclid(360.0);
    let parts = cerridwen::astrology::arabic_parts(
        h.ascendant,
        sun,
        moon,
        mercury,
        venus,
        mars,
        jupiter,
        saturn,
        is_day,
    );
    let part_arr: Vec<Value> = parts
        .iter()
        .map(|p| {
            json!({
                "name": p.name, "longitude": p.longitude, "formula": p.formula,
            })
        })
        .collect();
    Ok(json!({
        "jd": jd, "iso_date": jd2iso(jd),
        "ascendant": h.ascendant, "mc": h.mc,
        "is_day": is_day,
        "bodies": bodies,
        "lots": part_arr,
    }))
}

// -----------------------------------------------------------------------------
// Body name lookup (mirrors the HTTP server)
// -----------------------------------------------------------------------------

fn canonical_body_name(s: &str) -> Option<&'static str> {
    match s.to_ascii_lowercase().replace([' ', '-'], "_").as_str() {
        "sun" => Some("Sun"),
        "moon" => Some("Moon"),
        "mercury" => Some("Mercury"),
        "venus" => Some("Venus"),
        "mars" => Some("Mars"),
        "jupiter" => Some("Jupiter"),
        "saturn" => Some("Saturn"),
        "uranus" => Some("Uranus"),
        "neptune" => Some("Neptune"),
        "pluto" => Some("Pluto"),
        "mean_node" | "north_node" | "rahu" | "node" => Some("Mean Node"),
        "true_node" => Some("True Node"),
        "lilith" | "black_moon_lilith" | "mean_apogee" | "mean_apog" => Some("Mean Apogee"),
        "osc_apogee" | "true_lilith" | "osc_apog" => Some("Osc. Apogee"),
        "chiron" => Some("Chiron"),
        "ceres" => Some("Ceres"),
        "pallas" => Some("Pallas"),
        "juno" => Some("Juno"),
        "vesta" => Some("Vesta"),
        _ => None,
    }
}

fn body_for(name: &str, jd: f64) -> Option<Planet> {
    use cerridwen::planets::*;
    let id = match name {
        "Sun" => SE_SUN,
        "Moon" => SE_MOON,
        "Mercury" => SE_MERCURY,
        "Venus" => SE_VENUS,
        "Mars" => SE_MARS,
        "Jupiter" => SE_JUPITER,
        "Saturn" => SE_SATURN,
        "Uranus" => SE_URANUS,
        "Neptune" => SE_NEPTUNE,
        "Pluto" => SE_PLUTO,
        "Mean Node" => SE_MEAN_NODE,
        "True Node" => SE_TRUE_NODE,
        "Mean Apogee" => SE_MEAN_APOG,
        "Osc. Apogee" => SE_OSCU_APOG,
        "Chiron" => SE_CHIRON,
        "Ceres" => SE_CERES,
        "Pallas" => SE_PALLAS,
        "Juno" => SE_JUNO,
        "Vesta" => SE_VESTA,
        _ => return None,
    };
    Some(Planet::new(id, Some(jd), None))
}

// -----------------------------------------------------------------------------
// Compact JSON helpers — duplicated from the HTTP server so the MCP binary is
// self-contained.
// -----------------------------------------------------------------------------

fn planet_longitude_to_json(p: &PlanetLongitude) -> Value {
    let (sign, deg, min, sec) = p.rel_tuple();
    json!({
        "absolute_degrees": p.absolute_degrees,
        "sign": sign,
        "deg": p.deg(),
        "min": p.min(),
        "sec": p.sec(),
        "rel_tuple": [sign, deg, min, sec],
    })
}

fn planet_event_to_json(ev: &PlanetEvent) -> Value {
    json!({
        "description": ev.description,
        "jd": ev.jd,
        "iso_date": ev.iso_date(),
        "delta_days": ev.delta_days(None),
    })
}

fn moon_phase_to_json(p: &MoonPhaseData) -> Value {
    json!({
        "trend": p.trend,
        "shape": p.shape,
        "quarter": p.quarter,
        "quarter_english": p.quarter_english,
    })
}

fn void_of_course_to_json(v: &VoidOfCourseData) -> Value {
    json!({
        "is_void": v.is_void,
        "until_jd": v.until_jd,
        "until_iso": v.until_iso,
        "traditional_only": v.traditional_only,
    })
}

fn houses_to_json(h: &Houses, jd: f64) -> Value {
    let cusps: Vec<Value> = h
        .cusps
        .iter()
        .map(|&deg| {
            json!({
                "absolute_degrees": deg,
                "sign": PlanetLongitude::new(deg).sign(),
            })
        })
        .collect();
    json!({
        "jd": jd,
        "iso_date": jd2iso(jd),
        "system_code": h.system_code.to_string(),
        "system_name": h.system_name,
        "cusps": cusps,
        "ascendant": h.ascendant,
        "mc": h.mc,
        "armc": h.armc,
        "vertex": h.vertex,
        "equatorial_ascendant": h.equatorial_ascendant,
        "co_ascendant_koch": h.co_ascendant_koch,
        "co_ascendant_munkasey": h.co_ascendant_munkasey,
        "polar_ascendant": h.polar_ascendant,
    })
}

fn sun_data_to_json(d: &SunData, ayan: f64, ayan_name: &str) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("jd".into(), json!(d.jd));
    o.insert("iso_date".into(), json!(d.iso_date));
    o.insert("zodiac".into(), json!(ayan_name));
    if ayan != 0.0 {
        o.insert("ayanamsha_degrees".into(), json!(ayan));
    }
    let pos = shift_longitude(&d.position, ayan);
    o.insert("position".into(), planet_longitude_to_json(&pos));
    o.insert("dignity".into(), json!(d.dignity));
    o.insert("mean_orbital_period".into(), json!(d.mean_orbital_period));
    o.insert(
        "relative_orbital_velocity".into(),
        json!(d.relative_orbital_velocity),
    );
    if let Some(e) = &d.next_event {
        o.insert("next_event".into(), planet_event_to_json(e));
    }
    if let Some(e) = &d.next_rise {
        o.insert("next_rise".into(), planet_event_to_json(e));
    }
    if let Some(e) = &d.next_set {
        o.insert("next_set".into(), planet_event_to_json(e));
    }
    if let Some(e) = &d.last_rise {
        o.insert("last_rise".into(), planet_event_to_json(e));
    }
    if let Some(e) = &d.last_set {
        o.insert("last_set".into(), planet_event_to_json(e));
    }
    Value::Object(o)
}

fn moon_data_to_json(d: &MoonData, ayan: f64, ayan_name: &str) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("jd".into(), json!(d.jd));
    o.insert("iso_date".into(), json!(d.iso_date));
    o.insert("zodiac".into(), json!(ayan_name));
    if ayan != 0.0 {
        o.insert("ayanamsha_degrees".into(), json!(ayan));
    }
    let pos = shift_longitude(&d.position, ayan);
    o.insert("position".into(), planet_longitude_to_json(&pos));
    o.insert("phase".into(), moon_phase_to_json(&d.phase));
    o.insert("illumination".into(), json!(d.illumination));
    o.insert("distance".into(), json!(d.distance));
    o.insert("diameter".into(), json!(d.diameter));
    o.insert("diameter_ratio".into(), json!(d.diameter_ratio));
    o.insert("speed".into(), json!(d.speed));
    o.insert("speed_ratio".into(), json!(d.speed_ratio));
    o.insert("age".into(), json!(d.age));
    o.insert("period_length".into(), json!(d.period_length));
    o.insert("dignity".into(), json!(d.dignity));
    o.insert("mean_orbital_period".into(), json!(d.mean_orbital_period));
    o.insert(
        "relative_orbital_velocity".into(),
        json!(d.relative_orbital_velocity),
    );
    o.insert("lunation_number".into(), json!(d.lunation_number));
    o.insert(
        "void_of_course".into(),
        void_of_course_to_json(&d.void_of_course),
    );
    if let Some(e) = &d.next_event {
        o.insert("next_event".into(), planet_event_to_json(e));
    }
    o.insert(
        "next_new_moon".into(),
        planet_event_to_json(&d.next_new_moon),
    );
    o.insert(
        "next_full_moon".into(),
        planet_event_to_json(&d.next_full_moon),
    );
    o.insert(
        "next_new_or_full_moon".into(),
        planet_event_to_json(&d.next_new_or_full_moon),
    );
    o.insert(
        "last_new_moon".into(),
        planet_event_to_json(&d.last_new_moon),
    );
    o.insert(
        "last_full_moon".into(),
        planet_event_to_json(&d.last_full_moon),
    );
    if let Some(e) = &d.next_rise {
        o.insert("next_rise".into(), planet_event_to_json(e));
    }
    if let Some(e) = &d.next_set {
        o.insert("next_set".into(), planet_event_to_json(e));
    }
    if let Some(e) = &d.last_rise {
        o.insert("last_rise".into(), planet_event_to_json(e));
    }
    if let Some(e) = &d.last_set {
        o.insert("last_set".into(), planet_event_to_json(e));
    }
    Value::Object(o)
}

fn eclipse_to_json(e: &Eclipse) -> Value {
    json!({
        "kind": e.kind.as_str(),
        "central": e.central,
        "max_jd": e.max_jd,
        "max_iso": jd2iso(e.max_jd),
        "first_contact_jd": e.first_contact_jd,
        "first_contact_iso": e.first_contact_jd.map(jd2iso),
        "last_contact_jd": e.last_contact_jd,
        "last_contact_iso": e.last_contact_jd.map(jd2iso),
    })
}

fn transit_to_json(t: &ActiveTransit) -> Value {
    json!({
        "transit_body": t.transit_body,
        "natal_body": t.natal_body,
        "aspect": t.aspect_name,
        "mode": t.aspect_mode,
        "exact_angle": t.exact_angle,
        "orb_distance": t.orb_distance,
        "applying": t.applying,
    })
}
