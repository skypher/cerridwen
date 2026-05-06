use std::collections::HashMap;
use std::net::SocketAddr;

use axum::{
    extract::Query,
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use cerridwen::events::{get_events, EventFilter};
use cerridwen::planets::Planet;
use cerridwen::{
    compute_moon_data, compute_sun_data, jd2iso, jd_now, parse_jd_or_iso_date, ASPECTS, LatLong,
    MoonData, MoonPhaseData, PlanetEvent, PlanetLongitude, SunData,
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
        let data = compute_moon_data(None, Some(observer));
        println!("{}", serde_json::to_string_pretty(&moon_data_to_json(&data)).unwrap());
        return;
    }

    let app = Router::new()
        .route("/v1/sun", get(sun_endpoint))
        .route("/v1/moon", get(moon_endpoint))
        .route("/v1/olivier", get(olivier_endpoint))
        .route("/v1/events", get(events_endpoint));

    let addr = SocketAddr::from(([127, 0, 0, 1], args.port));
    println!("Starting Cerridwen API server on port {}.", args.port);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ------------------------------------------------------------------------------------------------
// Endpoints
// ------------------------------------------------------------------------------------------------

async fn sun_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    match parse_observer_and_jd(&q) {
        Ok((jd, latlong)) => {
            let data = compute_sun_data(jd, latlong);
            json_ok(sun_data_to_json(&data))
        }
        Err(e) => bad_request(&e),
    }
}

async fn moon_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    match parse_observer_and_jd(&q) {
        Ok((jd, latlong)) => {
            let data = compute_moon_data(jd, latlong);
            json_ok(moon_data_to_json(&data))
        }
        Err(e) => bad_request(&e),
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
        let (cusps, _ascmc) = swisseph::swe::houses(jd, ll.lat, ll.long, b'P' as i32);
        // SwissEph returns cusps[0] unused, cusps[1..=12] = houses 1-12. The
        // Python wrapper (pyswisseph) re-indexes to a 12-tuple of houses, so
        // we slice [1..13] to match.
        let cusps_rad: Vec<f64> = cusps[1..13].iter().map(|c| c.to_radians()).collect();
        result.insert("houses".into(), json!(cusps_rad));
    }

    json_ok(Value::Object(result))
}

async fn events_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let dbfile = std::env::var("CERRIDWEN_EVENTS_DB").unwrap_or_else(|_| "events.db".into());

    let jd_start = match q.get("date_start") {
        Some(s) => match parse_jd_or_iso_date(s) {
            Ok(j) => j,
            Err(e) => return bad_request(&e),
        },
        None => jd_now(),
    };

    let jd_end = if q.contains_key("lookahead") && q.contains_key("date_end") {
        return bad_request("Must not specify date_end and lookahead both together");
    } else if let Some(s) = q.get("date_end") {
        match parse_jd_or_iso_date(s) {
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
    let jd = match q.get("date") {
        Some(s) => Some(parse_jd_or_iso_date(s)?),
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
    let mut resp = (StatusCode::BAD_REQUEST, msg.to_string()).into_response();
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

fn sun_data_to_json(d: &SunData) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("jd".into(), json!(d.jd));
    o.insert("iso_date".into(), json!(d.iso_date));
    o.insert("position".into(), planet_longitude_to_json(&d.position));
    o.insert("dignity".into(), json!(d.dignity));
    if let Some(e) = &d.next_rise { o.insert("next_rise".into(), planet_event_to_json(e)); }
    if let Some(e) = &d.next_set  { o.insert("next_set".into(),  planet_event_to_json(e)); }
    if let Some(e) = &d.last_rise { o.insert("last_rise".into(), planet_event_to_json(e)); }
    if let Some(e) = &d.last_set  { o.insert("last_set".into(),  planet_event_to_json(e)); }
    Value::Object(o)
}

fn moon_data_to_json(d: &MoonData) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("jd".into(), json!(d.jd));
    o.insert("iso_date".into(), json!(d.iso_date));
    o.insert("position".into(), planet_longitude_to_json(&d.position));
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
