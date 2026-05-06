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
    compute_houses, compute_moon_data_with, compute_sun_data, jd2iso, jd_now, parse_house_system,
    parse_jd_or_iso_date, valid_house_systems, ASPECTS, Houses, LatLong, MoonData, MoonOptions,
    MoonPhaseData, PlanetEvent, PlanetLongitude, SunData, VoidOfCourseData,
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
        println!("{}", serde_json::to_string_pretty(&moon_data_to_json(&data)).unwrap());
        return;
    }

    let app = Router::new()
        .route("/v1/sun", get(sun_endpoint))
        .route("/v1/moon", get(moon_endpoint))
        .route("/v1/olivier", get(olivier_endpoint))
        .route("/v1/events", get(events_endpoint))
        .route("/v1/body/:name", get(body_endpoint))
        .route("/v1/houses", get(houses_endpoint));

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
            let opts = MoonOptions {
                voc_traditional_only: parse_bool(q.get("voc_traditional_only")),
            };
            let data = compute_moon_data_with(jd, latlong, opts);
            json_ok(moon_data_to_json(&data))
        }
        Err(e) => bad_request(&e),
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

    let mut o = serde_json::Map::new();
    o.insert("jd".into(), json!(jd));
    o.insert("iso_date".into(), json!(jd2iso(jd)));
    o.insert("name".into(), json!(planet.name()));
    o.insert("position".into(), planet_longitude_to_json(&planet.position(None)));
    o.insert("longitude".into(), json!(planet.longitude_at(jd)));
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

fn sun_data_to_json(d: &SunData) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("jd".into(), json!(d.jd));
    o.insert("iso_date".into(), json!(d.iso_date));
    o.insert("position".into(), planet_longitude_to_json(&d.position));
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
