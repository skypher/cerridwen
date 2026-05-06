use std::collections::HashMap;
use std::net::SocketAddr;

use axum::{
    extract::{Path as AxumPath, Query},
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use cerridwen::events::{get_events, EventFilter};
use cerridwen::planets::Planet;
use cerridwen::{
    apply_ayanamsha, compute_ayanamsha, compute_houses, compute_moon_data_with, compute_sun_data,
    compute_transits, default_transit_bodies, eclipses_within_period, jd2iso, jd_now, next_return,
    parse_ayanamsha, parse_house_system, parse_jd_or_iso_date_in_tz, valid_house_systems,
    ActiveTransit, ASPECTS, Eclipse, Houses, LatLong, MoonData, MoonOptions, MoonPhaseData,
    PlanetEvent, PlanetLongitude, SunData, VoidOfCourseData,
};
use clap::Parser;
use serde_json::{json, Value};

#[derive(Parser, Debug)]
#[command(name = "cerridwen-server",
          about = "JSON HTTP server exposing cerridwen sun/moon/event data")]
struct Args {
    #[arg(short, long, default_value_t = 2828)]
    port: u16,
    #[arg(short, long, default_value_t = false)]
    test: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    if args.test {
        let observer = LatLong::new(52.0, 13.0).unwrap();
        let data = compute_moon_data_with(None, Some(observer), MoonOptions::default());
        println!("{}", serde_json::to_string_pretty(&moon_data_to_json(&data, 0.0, "tropical")).unwrap());
        return;
    }

    let app = Router::new()
        .route("/v1/sun", get(sun_endpoint))
        .route("/v1/moon", get(moon_endpoint))
        .route("/v1/olivier", get(olivier_endpoint))
        .route("/v1/events", get(events_endpoint))
        .route("/v1/body/:name", get(body_endpoint))
        .route("/v1/houses", get(houses_endpoint))
        .route("/v1/eclipses", get(eclipses_endpoint))
        .route("/v1/transits", get(transits_endpoint))
        .route("/v1/events.ics", get(events_ics_endpoint))
        .route("/v1/return", get(return_endpoint))
        .route("/openapi.json", get(openapi_endpoint))
        .route("/docs", get(docs_endpoint));

    let addr = SocketAddr::from(([127, 0, 0, 1], args.port));
    println!("Starting Cerridwen API server on port {}.", args.port);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ------------------------------------------------------------------------------------------------
// Endpoints
// ------------------------------------------------------------------------------------------------

async fn sun_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let (jd_opt, latlong) = match parse_observer_and_jd(&q) {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    let data = compute_sun_data(jd_opt, latlong);
    let (ayan, ayan_name) = match parse_zodiac(&q, data.jd) {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    json_ok(sun_data_to_json(&data, ayan, ayan_name))
}

async fn moon_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let (jd_opt, latlong) = match parse_observer_and_jd(&q) {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    let opts = MoonOptions {
        voc_traditional_only: parse_bool(q.get("voc_traditional_only")),
    };
    let data = compute_moon_data_with(jd_opt, latlong, opts);
    let (ayan, ayan_name) = match parse_zodiac(&q, data.jd) {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    json_ok(moon_data_to_json(&data, ayan, ayan_name))
}

/// Resolves the zodiac/ayanamsha query params for a given JD.
/// Returns Ok((ayanamsha_deg, ayanamsha_name)) — ayanamsha_deg is 0.0 in
/// tropical mode and `name` is "tropical".
fn parse_zodiac(
    q: &HashMap<String, String>,
    jd: f64,
) -> Result<(f64, &'static str), String> {
    let zodiac = q.get("zodiac").map(|s| s.to_ascii_lowercase());
    match zodiac.as_deref() {
        None | Some("tropical") => Ok((0.0, "tropical")),
        Some("sidereal") => {
            let name = q.get("ayanamsha").map(|s| s.as_str()).unwrap_or("lahiri");
            let (mode, label) = parse_ayanamsha(name)
                .ok_or_else(|| format!("unknown ayanamsha: {}", name))?;
            let deg = compute_ayanamsha(jd, mode);
            Ok((deg, label))
        }
        Some(other) => Err(format!("zodiac must be tropical or sidereal, got: {}", other)),
    }
}

fn shift_longitude(p: &PlanetLongitude, ayanamsha_deg: f64) -> PlanetLongitude {
    if ayanamsha_deg == 0.0 {
        *p
    } else {
        PlanetLongitude::new(apply_ayanamsha(p.absolute_degrees, ayanamsha_deg))
    }
}

/// Permissive bool parser: accepts "1", "true", "yes", "on" (case-insensitive).
fn parse_bool(opt: Option<&String>) -> bool {
    match opt {
        Some(s) => matches!(
            s.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        None => false,
    }
}

async fn olivier_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    use cerridwen::{
        Body, Jupiter, Mars, Mercury, Moon, Neptune, Pluto, Saturn, Sun, Uranus, Venus,
    };
    let (jd_opt, latlong) = match parse_observer_and_jd(&q) {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    let jd = jd_opt.unwrap_or_else(jd_now);

    let mut result = serde_json::Map::new();
    result.insert("jd".into(), json!(jd));
    result.insert("iso_date".into(), json!(jd2iso(jd)));

    let bodies: Vec<(&str, Box<dyn Body>)> = vec![
        ("sun", Box::new(Sun::at_jd(jd))),
        ("moon", Box::new(Moon::at_jd(jd))),
        ("mercury", Box::new(Mercury::at_jd(jd))),
        ("venus", Box::new(Venus::at_jd(jd))),
        ("mars", Box::new(Mars::at_jd(jd))),
        ("jupiter", Box::new(Jupiter::at_jd(jd))),
        ("saturn", Box::new(Saturn::at_jd(jd))),
        ("uranus", Box::new(Uranus::at_jd(jd))),
        ("neptune", Box::new(Neptune::at_jd(jd))),
        ("pluto", Box::new(Pluto::at_jd(jd))),
    ];
    for (name, body) in bodies {
        result.insert(name.into(), json!(body.longitude(jd).to_radians()));
    }

    if let Some(ll) = latlong {
        let system = match q.get("house_system") {
            Some(s) => match parse_house_system(s) {
                Some(c) => c,
                None => return bad_request(&format!("unknown house_system: {}", s)),
            },
            None => 'P',
        };
        let h = compute_houses(jd, ll.lat, ll.long, system);
        let cusps_rad: Vec<f64> = h.cusps.iter().map(|c| c.to_radians()).collect();
        result.insert("houses".into(), json!(cusps_rad));
        result.insert("house_system".into(), json!(h.system_code.to_string()));
    }

    json_ok(Value::Object(result))
}

async fn openapi_endpoint() -> Response {
    json_ok(openapi_spec())
}

async fn docs_endpoint() -> Response {
    let html = r##"<!doctype html>
<html><head>
<title>Cerridwen API</title>
<meta charset="utf-8">
<script type="module" src="https://unpkg.com/rapidoc/dist/rapidoc-min.js"></script>
</head><body>
<rapi-doc spec-url="/openapi.json"
          theme="dark"
          render-style="read"
          show-header="false"
          allow-try="true"
          primary-color="#9b59b6">
</rapi-doc>
</body></html>"##;
    let mut resp = (StatusCode::OK, html.to_string()).into_response();
    resp.headers_mut().insert("Content-Type", HeaderValue::from_static("text/html; charset=utf-8"));
    resp
}

fn openapi_spec() -> Value {
    // String parameter shorthand.
    let p_string = |name: &str, desc: &str, required: bool| {
        json!({
            "name": name, "in": "query", "required": required,
            "description": desc, "schema": {"type": "string"},
        })
    };
    let p_number = |name: &str, desc: &str, required: bool| {
        json!({
            "name": name, "in": "query", "required": required,
            "description": desc, "schema": {"type": "number"},
        })
    };
    let date_param = p_string("date", "ISO 8601 timestamp or Julian Day decimal", false);
    let lat_param = p_number("latitude", "Observer latitude in degrees (-90..90)", false);
    let long_param = p_number("longitude", "Observer longitude in degrees (-180..180)", false);
    let tz_param = p_string("tz", "IANA timezone name (e.g. Europe/Berlin)", false);
    let zodiac_param = p_string("zodiac", "tropical (default) or sidereal", false);
    let ayan_param = p_string("ayanamsha", "lahiri/krishnamurti/fagan_bradley/raman/yukteshwar/...", false);

    let common_params = json!([date_param, lat_param, long_param, tz_param,
                              zodiac_param, ayan_param]);

    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Cerridwen API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Geocentric Sun/Moon/planet data, eclipses, transits, and events backed by Swiss Ephemeris.",
        },
        "paths": {
            "/v1/sun": {
                "get": {
                    "summary": "Sun position and rise/set",
                    "parameters": common_params,
                    "responses": { "200": { "description": "SunData" } }
                }
            },
            "/v1/moon": {
                "get": {
                    "summary": "Moon position, phase, void-of-course, lunation number, etc.",
                    "parameters": json!([
                        date_param, lat_param, long_param, tz_param,
                        zodiac_param, ayan_param,
                        {"name": "voc_traditional_only", "in": "query",
                         "description": "Restrict VoC search to the seven traditional planets",
                         "schema": {"type": "boolean"}}
                    ]),
                    "responses": { "200": { "description": "MoonData" } }
                }
            },
            "/v1/body/{name}": {
                "get": {
                    "summary": "Per-body data: position, longitude, speed, retrograde, illumination",
                    "parameters": json!([
                        {"name": "name", "in": "path", "required": true,
                         "description": "Sun, Moon, Mercury, Venus, Mars, Jupiter, Saturn, Uranus, Neptune, Pluto, Mean Node (north_node/rahu), True Node, Lilith, Chiron, Ceres, Pallas, Juno, Vesta",
                         "schema": {"type": "string"}},
                        date_param, lat_param, long_param, tz_param,
                        zodiac_param, ayan_param
                    ]),
                    "responses": { "200": { "description": "Body data" }, "404": { "description": "Unknown body" } }
                }
            },
            "/v1/houses": {
                "get": {
                    "summary": "House cusps and angle points",
                    "parameters": json!([
                        date_param, lat_param, long_param, tz_param,
                        {"name": "house_system", "in": "query",
                         "description": "Letter code (P/K/W/...) or name (placidus/whole_sign/koch/...)",
                         "schema": {"type": "string", "default": "P"}}
                    ]),
                    "responses": { "200": { "description": "Houses" } }
                }
            },
            "/v1/eclipses": {
                "get": {
                    "summary": "Solar/lunar eclipse predictions",
                    "parameters": json!([
                        p_string("date_start", "ISO date or JD", false),
                        p_string("date_end", "ISO date or JD (mutually exclusive with lookahead)", false),
                        p_number("lookahead", "Days forward from date_start", false),
                        p_string("type", "solar | lunar | both (default)", false),
                        p_number("limit", "Max results (default 20)", false),
                        tz_param,
                    ]),
                    "responses": { "200": { "description": "Array of eclipses" } }
                }
            },
            "/v1/transits": {
                "get": {
                    "summary": "Active transit-to-natal aspects",
                    "parameters": json!([
                        p_string("natal_date", "ISO date or JD of natal chart", true),
                        p_string("transit_date", "ISO date or JD of transit moment (default now)", false),
                        p_number("orb", "Orb in degrees (default 1.5)", false),
                        tz_param,
                    ]),
                    "responses": { "200": { "description": "Active aspects" } }
                }
            },
            "/v1/return": {
                "get": {
                    "summary": "Next solar/lunar/planetary return",
                    "parameters": json!([
                        p_string("body", "Sun, Moon, Mercury, ...", true),
                        p_string("natal_date", "ISO date or JD of natal chart", true),
                        p_string("start_date", "Start search from (default now)", false),
                        tz_param,
                    ]),
                    "responses": { "200": { "description": "Return JD" } }
                }
            },
            "/v1/events": {
                "get": {
                    "summary": "Database-backed astrological events",
                    "parameters": json!([
                        p_string("date_start", "ISO date or JD", false),
                        p_string("date_end", "ISO date or JD (XOR lookahead)", false),
                        p_number("lookahead", "Days forward", false),
                        p_string("types", "Comma-separated event types", false),
                        p_string("planets", "Comma-separated planet names", false),
                        p_number("limit", "Max results (default 30)", false),
                    ]),
                    "responses": { "200": { "description": "Events array" } }
                }
            },
            "/v1/events.ics": {
                "get": {
                    "summary": "iCalendar feed for the same events",
                    "parameters": json!([
                        p_string("date_start", "", false),
                        p_string("date_end", "", false),
                        p_number("lookahead", "", false),
                        p_string("types", "", false),
                        p_string("planets", "", false),
                    ]),
                    "responses": { "200": {
                        "description": "RFC 5545 VCALENDAR",
                        "content": {"text/calendar": {}}
                    } }
                }
            },
            "/v1/olivier": {
                "get": {
                    "summary": "Compact body positions in radians; houses if observer given",
                    "parameters": json!([
                        date_param, lat_param, long_param, tz_param,
                        p_string("house_system", "Letter code (default P)", false),
                    ]),
                    "responses": { "200": { "description": "Compact positions" } }
                }
            },
        }
    })
}

async fn return_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let tz = q.get("tz").map(|s| s.as_str());
    let body_name = match q.get("body") {
        Some(s) => s.as_str(),
        None => return bad_request("required: body=<Sun|Moon|Mercury|...>"),
    };
    let canonical = match canonical_body_name(body_name) {
        Some(c) => c,
        None => return not_found(&format!("unknown body: {}", body_name)),
    };
    let body_planet = match body_for(canonical, 0.0) {
        Some(p) => p,
        None => return not_found(&format!("unknown body: {}", body_name)),
    };
    let body_id = body_planet.id;

    let natal_jd = match q.get("natal_jd").or(q.get("natal_date")) {
        Some(s) => match parse_jd_or_iso_date_in_tz(s, tz) {
            Ok(j) => j,
            Err(e) => return bad_request(&e),
        },
        None => return bad_request("required: natal_jd or natal_date"),
    };
    let start_jd = match q.get("start_jd").or(q.get("start_date")) {
        Some(s) => match parse_jd_or_iso_date_in_tz(s, tz) {
            Ok(j) => j,
            Err(e) => return bad_request(&e),
        },
        None => jd_now(),
    };

    let return_jd = match next_return(body_id, natal_jd, start_jd) {
        Some(j) => j,
        None => return bad_request(&format!(
            "no return found for {} within typical period", canonical
        )),
    };

    // Natal longitude for context.
    let natal_lon = swisseph::swe::calc_ut(natal_jd, body_id as u32, 2)
        .map(|r| r.out[0])
        .unwrap_or(f64::NAN);

    json_ok(json!({
        "body": canonical,
        "natal_jd": natal_jd,
        "natal_iso": jd2iso(natal_jd),
        "natal_longitude": natal_lon,
        "search_from_jd": start_jd,
        "return_jd": return_jd,
        "return_iso": jd2iso(return_jd),
        "delta_days": return_jd - start_jd,
    }))
}

async fn events_ics_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    use cerridwen::events::{get_events, EventFilter};
    let dbfile = std::env::var("CERRIDWEN_EVENTS_DB").unwrap_or_else(|_| "events.db".into());
    let tz = q.get("tz").map(|s| s.as_str());

    let jd_start = match q.get("date_start") {
        Some(s) => match parse_jd_or_iso_date_in_tz(s, tz) {
            Ok(j) => j,
            Err(e) => return bad_request(&e),
        },
        None => jd_now(),
    };
    let jd_end = if let Some(s) = q.get("date_end") {
        match parse_jd_or_iso_date_in_tz(s, tz) {
            Ok(j) => j,
            Err(e) => return bad_request(&e),
        }
    } else if let Some(s) = q.get("lookahead") {
        match s.parse::<f64>() {
            Ok(n) if n >= 0.0 => jd_start + n,
            _ => return bad_request("lookahead must be non-negative"),
        }
    } else {
        jd_start + 365.0
    };
    let limit: i64 = q.get("limit").and_then(|s| s.parse().ok()).unwrap_or(500);
    let split = |key: &str| -> Option<Vec<String>> {
        q.get(key).map(|s| s.split(',').map(|x| x.to_string()).collect())
    };
    let filter = EventFilter {
        types: split("types"),
        subtypes: split("subtypes"),
        planets: split("planets"),
        datas: split("datas"),
    };

    let events = match get_events(&dbfile, jd_start, jd_end, limit, &filter) {
        Ok(v) => v,
        Err(e) => return bad_request(&format!("event query failed: {}", e)),
    };

    let mut ics = String::new();
    ics.push_str("BEGIN:VCALENDAR\r\n");
    ics.push_str("VERSION:2.0\r\n");
    ics.push_str("PRODID:-//cerridwen//cerridwen-server//EN\r\n");
    ics.push_str("CALSCALE:GREGORIAN\r\n");
    ics.push_str("METHOD:PUBLISH\r\n");
    ics.push_str("X-WR-CALNAME:Cerridwen astrological events\r\n");
    for ev in &events {
        let utc = jd_to_utc_basic(ev.jd);
        let utc_end = jd_to_utc_basic(ev.jd + 1.0 / 1440.0); // 1-minute event
        let title = format_event_summary(&ev.r#type, &ev.subtype, &ev.planet, &ev.data);
        let uid = format!("cerridwen-{}-{}-{}-{}@cerridwen", ev.r#type, ev.planet, ev.data, ev.jd as i64);
        ics.push_str("BEGIN:VEVENT\r\n");
        ics.push_str(&format!("UID:{}\r\n", uid));
        ics.push_str(&format!("DTSTAMP:{}\r\n", utc));
        ics.push_str(&format!("DTSTART:{}\r\n", utc));
        ics.push_str(&format!("DTEND:{}\r\n", utc_end));
        ics.push_str(&format!("SUMMARY:{}\r\n", ical_escape(&title)));
        ics.push_str(&format!(
            "DESCRIPTION:JD {:.6}\\n{} {} {} {}\r\n",
            ev.jd, ev.r#type, ev.subtype, ev.planet, ev.data
        ));
        ics.push_str("TRANSP:TRANSPARENT\r\n");
        ics.push_str("END:VEVENT\r\n");
    }
    ics.push_str("END:VCALENDAR\r\n");

    let mut resp = (StatusCode::OK, ics).into_response();
    resp.headers_mut().insert("Content-Type", HeaderValue::from_static("text/calendar; charset=utf-8"));
    resp.headers_mut().insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
    resp
}

/// Produce a UTC iCal-basic timestamp (YYYYMMDDTHHMMSSZ) from a JD.
fn jd_to_utc_basic(jd: f64) -> String {
    // Use the same revjul-based math jd2iso uses, then reformat.
    let iso = jd2iso(jd);
    // iso is "YYYY-MM-DD HH:MM:SS"
    let bytes = iso.as_bytes();
    if bytes.len() < 19 {
        return format!("{}", iso);
    }
    format!(
        "{}{}{}T{}{}{}Z",
        &iso[0..4],
        &iso[5..7],
        &iso[8..10],
        &iso[11..13],
        &iso[14..16],
        &iso[17..19],
    )
}

fn ical_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

fn format_event_summary(t: &str, st: &str, p: &str, d: &str) -> String {
    match t {
        "ingress" => format!("{} enters {}", p, d),
        "rx" => format!("{} stations retrograde in {}", p, d),
        "direct" => format!("{} stations direct in {}", p, d),
        _ => {
            let mode = if st.is_empty() { String::new() } else { format!(" {}", st) };
            format!("{} {}{} {}", p, t, mode, d)
        }
    }
}

async fn transits_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let tz = q.get("tz").map(|s| s.as_str());
    let natal_jd = match q.get("natal_jd").or(q.get("natal_date")) {
        Some(s) => match parse_jd_or_iso_date_in_tz(s, tz) {
            Ok(j) => j,
            Err(e) => return bad_request(&e),
        },
        None => return bad_request("required: natal_jd or natal_date"),
    };
    let transit_jd = match q.get("transit_jd").or(q.get("transit_date")) {
        Some(s) => match parse_jd_or_iso_date_in_tz(s, tz) {
            Ok(j) => j,
            Err(e) => return bad_request(&e),
        },
        None => jd_now(),
    };
    let orb: f64 = match q.get("orb") {
        Some(s) => match s.parse::<f64>() {
            Ok(v) if v > 0.0 && v < 30.0 => v,
            _ => return bad_request("orb must be in (0, 30) degrees"),
        },
        None => 1.5,
    };
    let bodies = default_transit_bodies();
    let active = compute_transits(natal_jd, transit_jd, &bodies, orb);
    let arr: Vec<Value> = active.iter().map(transit_to_json).collect();
    let mut o = serde_json::Map::new();
    o.insert("natal_jd".into(), json!(natal_jd));
    o.insert("natal_iso".into(), json!(jd2iso(natal_jd)));
    o.insert("transit_jd".into(), json!(transit_jd));
    o.insert("transit_iso".into(), json!(jd2iso(transit_jd)));
    o.insert("orb".into(), json!(orb));
    o.insert("active".into(), json!(arr));
    json_ok(Value::Object(o))
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

async fn eclipses_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let jd_start = match q.get("date_start") {
        Some(s) => match parse_jd_or_iso_date_in_tz(s, q.get("tz").map(|x| x.as_str())) {
            Ok(j) => j,
            Err(e) => return bad_request(&e),
        },
        None => jd_now(),
    };

    let jd_end = if q.contains_key("lookahead") && q.contains_key("date_end") {
        return bad_request("Must not specify date_end and lookahead both together");
    } else if let Some(s) = q.get("date_end") {
        match parse_jd_or_iso_date_in_tz(s, q.get("tz").map(|x| x.as_str())) {
            Ok(j) => j,
            Err(e) => return bad_request(&e),
        }
    } else if let Some(s) = q.get("lookahead") {
        match s.parse::<f64>() {
            Ok(n) if n >= 0.0 => jd_start + n,
            Ok(_) => return bad_request("lookahead must be non-negative"),
            Err(_) => return bad_request("lookahead must be a number"),
        }
    } else {
        // Default: search a year forward — eclipses come in pairs every ~6 months.
        jd_start + 365.0
    };

    let kind = q.get("type").map(|s| s.to_ascii_lowercase());
    let (solar, lunar) = match kind.as_deref() {
        None | Some("both") | Some("any") => (true, true),
        Some("solar") => (true, false),
        Some("lunar") => (false, true),
        Some(other) => return bad_request(
            &format!("type must be one of: solar, lunar, both. Got: {}", other)
        ),
    };

    let limit: usize = match q.get("limit") {
        Some(s) => match s.parse::<usize>() {
            Ok(n) => n,
            Err(_) => return bad_request("limit must be a non-negative integer"),
        },
        None => 20,
    };

    let eclipses = eclipses_within_period(jd_start, jd_end, solar, lunar, limit);
    let arr: Vec<Value> = eclipses.iter().map(eclipse_to_json).collect();
    json_ok(Value::Array(arr))
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

async fn houses_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let (jd_opt, latlong) = match parse_observer_and_jd(&q) {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    let observer = match latlong {
        Some(o) => o,
        None => return bad_request("Specify both latitude and longitude"),
    };
    let jd = jd_opt.unwrap_or_else(jd_now);

    // Default to Placidus when not specified.
    let system = match q.get("house_system") {
        Some(s) => match parse_house_system(s) {
            Some(c) => c,
            None => {
                let known: Vec<String> = valid_house_systems()
                    .iter()
                    .map(|(c, name)| format!("{}={}", c, name))
                    .collect();
                return bad_request(&format!(
                    "unknown house_system: {}. Known systems: {}",
                    s,
                    known.join(", ")
                ));
            }
        },
        None => 'P',
    };

    let houses = compute_houses(jd, observer.lat, observer.long, system);
    json_ok(houses_to_json(&houses, jd))
}

fn houses_to_json(h: &Houses, jd: f64) -> Value {
    let cusps: Vec<Value> = h.cusps.iter()
        .map(|&deg| json!({
            "absolute_degrees": deg,
            "sign": cerridwen::PlanetLongitude::new(deg).sign(),
        }))
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

async fn body_endpoint(
    AxumPath(name): AxumPath<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let (jd_opt, latlong) = match parse_observer_and_jd(&q) {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    let jd = jd_opt.unwrap_or_else(jd_now);

    // Allow case-insensitive lookups: "mercury", "Mercury", "MERCURY".
    let canonical = canonical_body_name(&name);
    let planet = match canonical {
        Some(c) => match body_for(c, jd) {
            Some(p) => p,
            None => return not_found(&format!("unknown body: {}", name)),
        },
        None => return not_found(&format!("unknown body: {}", name)),
    };

    let (ayan, ayan_name) = match parse_zodiac(&q, jd) {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };

    let trop_lon = planet.longitude_at(jd);
    let lon = if ayan != 0.0 { apply_ayanamsha(trop_lon, ayan) } else { trop_lon };
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
    o.insert("mean_orbital_period".into(), json!(planet.mean_orbital_period()));
    o.insert(
        "relative_orbital_velocity".into(),
        json!(planet.relative_orbital_velocity()),
    );

    if let Some(ev) = planet.next_event() {
        o.insert("next_event".into(), planet_event_to_json(&ev));
    }

    if latlong.is_some() {
        // Build a fresh Planet with the observer set so rise/set work.
        let with_observer = Planet::new(planet.id, Some(jd), latlong);
        o.insert("next_rise".into(), planet_event_to_json(&with_observer.next_rise()));
        o.insert("next_set".into(), planet_event_to_json(&with_observer.next_set()));
        o.insert("last_rise".into(), planet_event_to_json(&with_observer.last_rise()));
        o.insert("last_set".into(), planet_event_to_json(&with_observer.last_set()));
    }

    json_ok(Value::Object(o))
}

fn canonical_body_name(s: &str) -> Option<&'static str> {
    // Accept synonyms — "rahu"/"north_node" → mean node, "ketu"/"south_node"
    // is rendered as the mean node opposite (handled at lookup time).
    match s.to_ascii_lowercase().as_str() {
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
        "true_node" | "true_north_node" => Some("True Node"),
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

async fn events_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let dbfile = std::env::var("CERRIDWEN_EVENTS_DB").unwrap_or_else(|_| "events.db".into());

    let jd_start = match q.get("date_start") {
        Some(s) => match parse_jd_or_iso_date_in_tz(s, q.get("tz").map(|x| x.as_str())) {
            Ok(j) => j,
            Err(e) => return bad_request(&e),
        },
        None => jd_now(),
    };

    let jd_end = if q.contains_key("lookahead") && q.contains_key("date_end") {
        return bad_request("Must not specify date_end and lookahead both together");
    } else if let Some(s) = q.get("date_end") {
        match parse_jd_or_iso_date_in_tz(s, q.get("tz").map(|x| x.as_str())) {
            Ok(j) => j,
            Err(e) => return bad_request(&e),
        }
    } else if let Some(s) = q.get("lookahead") {
        match s.parse::<i64>() {
            Ok(n) if n >= 0 => jd_start + n as f64,
            Ok(_) => return bad_request("lookahead must be non-negative"),
            Err(_) => return bad_request("lookahead must be an integer"),
        }
    } else {
        jd_start + 40.0
    };

    let limit: i64 = match q.get("limit") {
        Some(s) => match s.parse::<i64>() {
            Ok(n) if n >= 0 => n,
            Ok(_) => return bad_request("limit must be non-negative"),
            Err(_) => return bad_request("limit must be an integer"),
        },
        None => 30,
    };

    let split = |key: &str| -> Option<Vec<String>> {
        q.get(key).map(|s| s.split(',').map(|x| x.to_string()).collect())
    };

    let filter = EventFilter {
        types: split("types"),
        subtypes: split("subtypes"),
        planets: split("planets"),
        datas: split("datas"),
    };

    let events = match get_events(&dbfile, jd_start, jd_end, limit, &filter) {
        Ok(v) => v,
        Err(e) => return bad_request(&format!("event query failed: {}", e)),
    };

    let mut out = Vec::with_capacity(events.len());
    for ev in events {
        let mut obj = serde_json::Map::new();
        obj.insert("jd".into(), json!(ev.jd));
        obj.insert("type".into(), json!(ev.r#type));
        obj.insert("subtype".into(), json!(ev.subtype));
        obj.insert("planet".into(), json!(ev.planet));
        obj.insert("data".into(), json!(ev.data));
        obj.insert("iso_date".into(), json!(ev.iso_date));
        obj.insert("delta_days".into(), json!(ev.delta_days));

        if let Some(p) = body_for(&ev.planet, ev.jd) {
            obj.insert("position".into(), planet_longitude_to_json(&p.position(None)));
            if ASPECTS.iter().any(|a| a.name == ev.r#type) {
                if let Some(p2) = body_for(&ev.data, ev.jd) {
                    obj.insert(
                        "data_position".into(),
                        planet_longitude_to_json(&p2.position(None)),
                    );
                }
            }
        }
        out.push(Value::Object(obj));
    }
    json_ok(Value::Array(out))
}

// ------------------------------------------------------------------------------------------------
// Helpers — query parsing, JSON shaping, response building.
// ------------------------------------------------------------------------------------------------

fn parse_observer_and_jd(
    q: &HashMap<String, String>,
) -> Result<(Option<f64>, Option<LatLong>), String> {
    let tz = q.get("tz").map(|s| s.as_str());
    let jd = match q.get("date") {
        Some(s) => Some(parse_jd_or_iso_date_in_tz(s, tz)?),
        None => None,
    };
    let lat = q.get("latitude").map(|s| s.parse::<f64>()).transpose()
        .map_err(|e| format!("invalid latitude: {}", e))?;
    let long = q.get("longitude").map(|s| s.parse::<f64>()).transpose()
        .map_err(|e| format!("invalid longitude: {}", e))?;
    let latlong = match (lat, long) {
        (Some(la), Some(lo)) => Some(LatLong::new(la, lo).map_err(|s| s.to_string())?),
        (None, None) => None,
        _ => return Err("Specify both longitude and latitude or none".into()),
    };
    Ok((jd, latlong))
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

fn json_ok(v: Value) -> Response {
    let mut resp = (StatusCode::OK, serde_json::to_string_pretty(&v).unwrap_or_default())
        .into_response();
    resp.headers_mut().insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
    resp.headers_mut().insert("Content-Type", HeaderValue::from_static("application/json"));
    resp
}

fn bad_request(msg: &str) -> Response {
    error_response(StatusCode::BAD_REQUEST, msg)
}

fn not_found(msg: &str) -> Response {
    error_response(StatusCode::NOT_FOUND, msg)
}

fn error_response(status: StatusCode, msg: &str) -> Response {
    let mut resp = (status, msg.to_string()).into_response();
    resp.headers_mut().insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
    resp.headers_mut().insert("Content-Type", HeaderValue::from_static("text/plain"));
    resp
}

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
    if let Some(e) = &d.next_event { o.insert("next_event".into(), planet_event_to_json(e)); }
    if let Some(e) = &d.next_rise { o.insert("next_rise".into(), planet_event_to_json(e)); }
    if let Some(e) = &d.next_set  { o.insert("next_set".into(),  planet_event_to_json(e)); }
    if let Some(e) = &d.last_rise { o.insert("last_rise".into(), planet_event_to_json(e)); }
    if let Some(e) = &d.last_set  { o.insert("last_set".into(),  planet_event_to_json(e)); }
    Value::Object(o)
}

fn void_of_course_to_json(v: &VoidOfCourseData) -> Value {
    json!({
        "is_void": v.is_void,
        "until_jd": v.until_jd,
        "until_iso": v.until_iso,
        "traditional_only": v.traditional_only,
    })
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
    o.insert("void_of_course".into(), void_of_course_to_json(&d.void_of_course));
    if let Some(e) = &d.next_event { o.insert("next_event".into(), planet_event_to_json(e)); }
    o.insert("next_new_moon".into(), planet_event_to_json(&d.next_new_moon));
    o.insert("next_full_moon".into(), planet_event_to_json(&d.next_full_moon));
    o.insert("next_new_or_full_moon".into(), planet_event_to_json(&d.next_new_or_full_moon));
    o.insert("last_new_moon".into(), planet_event_to_json(&d.last_new_moon));
    o.insert("last_full_moon".into(), planet_event_to_json(&d.last_full_moon));
    if let Some(e) = &d.next_rise { o.insert("next_rise".into(), planet_event_to_json(e)); }
    if let Some(e) = &d.next_set  { o.insert("next_set".into(),  planet_event_to_json(e)); }
    if let Some(e) = &d.last_rise { o.insert("last_rise".into(), planet_event_to_json(e)); }
    if let Some(e) = &d.last_set  { o.insert("last_set".into(),  planet_event_to_json(e)); }
    Value::Object(o)
}
