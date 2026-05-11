// SPDX-License-Identifier: MIT AND AGPL-3.0-only

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use axum::{
    body::Body,
    extract::{Path as AxumPath, Query},
    http::{HeaderValue, Request, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event as SseEvent, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::get,
    Router,
};
use cerridwen::astrology;
use cerridwen::events::{get_events, EventFilter};
use cerridwen::planets::Planet;
use cerridwen::{
    angle_points, apply_ayanamsha, compute_aspects_extended, compute_ayanamsha, compute_houses,
    compute_moon_data_with, compute_sun_data, compute_transits_extended, default_transit_bodies,
    eclipses_within_period, fixed_star, jd2iso, jd_now, next_return, parse_ayanamsha,
    parse_house_system, parse_jd_or_iso_date_in_tz, valid_house_systems, ActiveTransit, Eclipse,
    Houses, InstantAspect, LatLong, MoonData, MoonOptions, MoonPhaseData, PlanetEvent,
    PlanetLongitude, SunData, VoidOfCourseData, ASPECTS,
};
use clap::Parser;
use futures_util::stream::{self, Stream};
use serde_json::{json, Value};
use std::convert::Infallible;

#[derive(Parser, Debug)]
#[command(
    name = "cerridwen-server",
    about = "JSON HTTP server exposing cerridwen sun/moon/event data"
)]
struct Args {
    /// Listen address. Use `0.0.0.0` to expose externally.
    /// Default `127.0.0.1` keeps the server local — sit behind nginx.
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,

    /// Log output format. `text` is the default human-readable form;
    /// `json` emits one JSON object per line for log aggregators.
    /// Env: `CERRIDWEN_LOG_FORMAT`.
    #[arg(long, env = "CERRIDWEN_LOG_FORMAT", default_value = "text")]
    log_format: String,

    /// Optional log file directory; logs rotate daily as
    /// `cerridwen.log.YYYY-MM-DD`. Empty = stderr only.
    /// Env: `CERRIDWEN_LOG_DIR`.
    #[arg(long, env = "CERRIDWEN_LOG_DIR", default_value = "")]
    log_dir: String,

    /// Comma-separated list of allowed CORS origins. Empty = allow any (*).
    /// Env: `CERRIDWEN_CORS_ORIGINS`.
    #[arg(long, env = "CERRIDWEN_CORS_ORIGINS", default_value = "")]
    cors_origins: String,

    /// Optional API key. When set, all `/v1/*` requests must carry an
    /// `X-API-Key` header matching this value. `/health`, `/metrics`,
    /// `/openapi.json`, `/docs`, `/app`, `/chart`, `/favicon.ico`,
    /// `/robots.txt`, and `/` are always public.
    /// Env: `CERRIDWEN_API_KEY`.
    #[arg(long, env = "CERRIDWEN_API_KEY", default_value = "")]
    api_key: String,

    #[arg(short, long, default_value_t = 2828)]
    port: u16,

    /// Response cache TTL in seconds. Set to 0 to disable caching entirely.
    /// Env: `CERRIDWEN_CACHE_TTL`.
    #[arg(long, env = "CERRIDWEN_CACHE_TTL", default_value_t = 10)]
    cache_ttl: u64,

    /// Rate limit: max requests per window per client.
    /// Env: `CERRIDWEN_RATE_LIMIT_MAX`.
    #[arg(long, env = "CERRIDWEN_RATE_LIMIT_MAX", default_value_t = 60)]
    rate_limit_max: usize,

    /// Rate limit: window length in seconds.
    /// Env: `CERRIDWEN_RATE_LIMIT_WINDOW`.
    #[arg(long, env = "CERRIDWEN_RATE_LIMIT_WINDOW", default_value_t = 10)]
    rate_limit_window: u64,

    #[arg(short, long, default_value_t = false)]
    test: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Structured logging — RUST_LOG=info,cerridwen_server=debug or similar.
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    // Hold a guard if we're rotating to a file so the appender's
    // background thread keeps running for the lifetime of main.
    let _log_guard = if !args.log_dir.is_empty() {
        let appender = tracing_appender::rolling::daily(&args.log_dir, "cerridwen.log");
        let (nb, guard) = tracing_appender::non_blocking(appender);
        match args.log_format.as_str() {
            "json" => tracing_subscriber::fmt()
                .json()
                .flatten_event(true)
                .with_current_span(false)
                .with_env_filter(env_filter)
                .with_target(false)
                .with_writer(nb)
                .init(),
            _ => tracing_subscriber::fmt()
                .with_env_filter(env_filter)
                .with_target(false)
                .with_writer(nb)
                .init(),
        }
        Some(guard)
    } else {
        match args.log_format.as_str() {
            "json" => tracing_subscriber::fmt()
                .json()
                .flatten_event(true)
                .with_current_span(false)
                .with_env_filter(env_filter)
                .with_target(false)
                .init(),
            _ => tracing_subscriber::fmt()
                .with_env_filter(env_filter)
                .with_target(false)
                .init(),
        }
        None
    };
    if args.test {
        let observer = LatLong::new(52.0, 13.0).unwrap();
        let data = compute_moon_data_with(None, Some(observer), MoonOptions::default());
        println!(
            "{}",
            serde_json::to_string_pretty(&moon_data_to_json(&data, 0.0, "tropical")).unwrap()
        );
        return;
    }

    let cache = Arc::new(ResponseCache::new(Duration::from_secs(args.cache_ttl)));
    let rate_limiter = RateLimiter::new(
        Duration::from_secs(args.rate_limit_window),
        args.rate_limit_max,
    );
    METRICS
        .rate_limit_max
        .store(args.rate_limit_max as u64, Ordering::Relaxed);
    METRICS
        .rate_limit_window_seconds
        .store(args.rate_limit_window, Ordering::Relaxed);
    METRICS
        .cache_ttl_seconds
        .store(args.cache_ttl, Ordering::Relaxed);
    // Build the CORS layer based on the --cors-origins setting.
    let cors_methods = [axum::http::Method::GET, axum::http::Method::OPTIONS];
    let cors = if args.cors_origins.is_empty() {
        tower_http::cors::CorsLayer::new()
            .allow_origin(tower_http::cors::Any)
            .allow_methods(cors_methods)
            .allow_headers(tower_http::cors::Any)
    } else {
        let origins: Vec<HeaderValue> = args
            .cors_origins
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .filter_map(|s| HeaderValue::from_str(s).ok())
            .collect();
        tower_http::cors::CorsLayer::new()
            .allow_origin(origins)
            .allow_methods(cors_methods)
            .allow_headers(tower_http::cors::Any)
    };
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
        .route("/v1/star/:name", get(star_endpoint))
        .route("/v1/aspects", get(aspects_endpoint))
        .route("/v1/declinations", get(declinations_endpoint))
        .route("/v1/stations", get(stations_endpoint))
        .route("/v1/twilight", get(twilight_endpoint))
        .route("/v1/planetary-hours", get(planetary_hours_endpoint))
        .route("/v1/arabic-parts", get(arabic_parts_endpoint))
        .route("/v1/profections", get(profections_endpoint))
        .route("/v1/prenatal-eclipse", get(prenatal_eclipse_endpoint))
        .route("/v1/synastry", get(synastry_endpoint))
        .route("/v1/composite", get(composite_endpoint))
        .route("/v1/progressions", get(progressions_endpoint))
        .route("/v1/stream/sun", get(stream_sun_endpoint))
        .route("/v1/stream/moon", get(stream_moon_endpoint))
        .route("/v1/stream/body/:name", get(stream_body_endpoint))
        .route("/v1/stream/events", get(stream_events_endpoint))
        .route("/openapi.json", get(openapi_endpoint))
        .route("/docs", get(docs_endpoint))
        .route("/chart", get(chart_endpoint))
        .route("/app", get(app_endpoint))
        .route("/", get(app_endpoint))
        .route("/favicon.ico", get(favicon_endpoint))
        .route("/robots.txt", get(robots_endpoint))
        .route("/health", get(health_endpoint))
        .route("/metrics", get(metrics_endpoint))
        .layer(middleware::from_fn(latency_middleware))
        .layer(middleware::from_fn_with_state(
            cache.clone(),
            cache_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            rate_limiter,
            rate_limit_middleware,
        ));
    // API-key gate sits OUTSIDE the rate limit + cache so unauthenticated
    // requests don't poison either; only attach when configured.
    let app = if args.api_key.is_empty() {
        app
    } else {
        let key = Arc::new(args.api_key.clone());
        app.layer(middleware::from_fn_with_state(key, api_key_middleware))
    };
    let app = app
        // Compress JSON / text / openapi responses. Streams remain
        // uncompressed because they're already line-delimited and
        // gzip-buffered SSE would defeat its real-time nature.
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(tower_http::limit::RequestBodyLimitLayer::new(64 * 1024))
        // Generate a request id, propagate it back as the x-request-id
        // response header, and log it as part of the trace span so server
        // logs and client errors line up. tower-http's .layer chain applies
        // outermost-last, so SetRequestId must come AFTER PropagateRequestId
        // for the propagation layer to see the id on the way back out.
        .layer(tower_http::request_id::PropagateRequestIdLayer::new(
            axum::http::HeaderName::from_static("x-request-id"),
        ))
        .layer(tower_http::request_id::SetRequestIdLayer::new(
            axum::http::HeaderName::from_static("x-request-id"),
            RequestIdGen,
        ))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(cors)
        .with_state(cache);

    let bind_str = format!("{}:{}", args.bind, args.port);
    let addr: SocketAddr = bind_str.parse().unwrap_or_else(|e| {
        tracing::error!("invalid --bind {}: {}", bind_str, e);
        std::process::exit(2);
    });
    tracing::info!(
        bind = %addr,
        cache_ttl = args.cache_ttl,
        rate_limit = format!("{}/{}s", args.rate_limit_max, args.rate_limit_window),
        version = env!("CARGO_PKG_VERSION"),
        "starting cerridwen-server"
    );
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

/// Per-request UUID generator. Cheap (a process-counter-based ID),
/// non-cryptographic; collisions across restarts don't matter.
#[derive(Clone, Copy)]
struct RequestIdGen;
impl tower_http::request_id::MakeRequestId for RequestIdGen {
    fn make_request_id<B>(&mut self, _: &Request<B>) -> Option<tower_http::request_id::RequestId> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // Pid + counter so multiple workers don't collide cross-restart.
        let id = format!("{:x}-{:x}", std::process::id(), n);
        Some(tower_http::request_id::RequestId::new(
            axum::http::HeaderValue::from_str(&id).ok()?,
        ))
    }
}

/// Wait for SIGINT (Ctrl-C) or SIGTERM. axum's `with_graceful_shutdown`
/// then drains in-flight requests before returning.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install Ctrl-C handler");
    };
    #[cfg(unix)]
    let term = async {
        use tokio::signal::unix::{signal, SignalKind};
        signal(SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("ctrl-c received, draining"),
        _ = term => tracing::info!("SIGTERM received, draining"),
    }
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
fn parse_zodiac(q: &HashMap<String, String>, jd: f64) -> Result<(f64, &'static str), String> {
    let zodiac = q.get("zodiac").map(|s| s.to_ascii_lowercase());
    match zodiac.as_deref() {
        None | Some("tropical") => Ok((0.0, "tropical")),
        Some("sidereal") => {
            let name = q.get("ayanamsha").map(|s| s.as_str()).unwrap_or("lahiri");
            let (mode, label) =
                parse_ayanamsha(name).ok_or_else(|| format!("unknown ayanamsha: {name}"))?;
            let deg = compute_ayanamsha(jd, mode);
            Ok((deg, label))
        }
        Some(other) => Err(format!("zodiac must be tropical or sidereal, got: {other}")),
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
        Some(s) => matches!(s.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
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

    // Sidereal mode applies an ayanamsha shift to all body longitudes.
    let (ayan, ayan_name) = match parse_zodiac(&q, jd) {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    let shift = |lon_deg: f64| -> f64 {
        if ayan != 0.0 {
            apply_ayanamsha(lon_deg, ayan)
        } else {
            lon_deg
        }
    };

    let mut result = serde_json::Map::new();
    result.insert("jd".into(), json!(jd));
    result.insert("iso_date".into(), json!(jd2iso(jd)));
    result.insert("zodiac".into(), json!(ayan_name));
    if ayan != 0.0 {
        result.insert("ayanamsha_degrees".into(), json!(ayan));
    }

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
    // Track which bodies are retrograde so the chart can show ℞ markers.
    let mut rx = serde_json::Map::new();
    for (name, body) in bodies {
        result.insert(name.into(), json!(shift(body.longitude(jd)).to_radians()));
    }
    // Speeds for the ten classic bodies + extras. Re-fetch via Planet so we
    // can read the speed component without touching the trait dispatch.
    use cerridwen::planets::{
        SE_JUPITER, SE_MARS, SE_MERCURY, SE_MOON, SE_NEPTUNE, SE_PLUTO, SE_SATURN, SE_SUN,
        SE_URANUS, SE_VENUS,
    };
    let classic_ids: &[(&str, i32)] = &[
        ("sun", SE_SUN),
        ("moon", SE_MOON),
        ("mercury", SE_MERCURY),
        ("venus", SE_VENUS),
        ("mars", SE_MARS),
        ("jupiter", SE_JUPITER),
        ("saturn", SE_SATURN),
        ("uranus", SE_URANUS),
        ("neptune", SE_NEPTUNE),
        ("pluto", SE_PLUTO),
    ];
    for (name, id) in classic_ids {
        let p = Planet::new(*id, Some(jd), None);
        rx.insert((*name).into(), json!(p.is_rx(None)));
    }

    // Extras: lunar nodes, Lilith, Chiron, the four asteroids — fetched
    // via raw Planet so we don't need 8 more wrapper macro instantiations
    // here.
    use cerridwen::planets::{
        SE_CERES, SE_CHIRON, SE_JUNO, SE_MEAN_APOG, SE_MEAN_NODE, SE_PALLAS, SE_VESTA,
    };
    let extras: &[(&str, i32)] = &[
        ("north_node", SE_MEAN_NODE),
        ("lilith", SE_MEAN_APOG),
        ("chiron", SE_CHIRON),
        ("ceres", SE_CERES),
        ("pallas", SE_PALLAS),
        ("juno", SE_JUNO),
        ("vesta", SE_VESTA),
    ];
    for (name, id) in extras {
        let p = Planet::new(*id, Some(jd), None);
        result.insert(
            (*name).into(),
            json!(shift(p.longitude_at(jd)).to_radians()),
        );
        if p.has_rx_stations() {
            rx.insert((*name).into(), json!(p.is_rx(None)));
        }
    }
    result.insert("retrograde".into(), Value::Object(rx));
    // south_node opposes north_node by 180° in either zodiac.
    if let Some(nn) = result.get("north_node").and_then(|v| v.as_f64()) {
        let sn = (nn + std::f64::consts::PI) % (2.0 * std::f64::consts::PI);
        result.insert("south_node".into(), json!(sn));
    }

    if let Some(ll) = latlong {
        let system = match q.get("house_system") {
            Some(s) => match parse_house_system(s) {
                Some(c) => c,
                None => return bad_request(&format!("unknown house_system: {s}")),
            },
            None => 'P',
        };
        let h = compute_houses(jd, ll.lat, ll.long, system);
        // House cusps are tropical by construction; shift them along with
        // the bodies when sidereal mode is selected.
        let cusps_rad: Vec<f64> = h.cusps.iter().map(|c| shift(*c).to_radians()).collect();
        result.insert("houses".into(), json!(cusps_rad));
        result.insert("house_system".into(), json!(h.system_code.to_string()));
    }

    json_ok(Value::Object(result))
}

// ------------------------------------------------------------------------------------------------
// Response cache — small in-memory TTL cache. Replaces Python's MWT(timeout=10).
//
// All endpoint responses are deterministic given the full URL (path + query),
// so we key the cache on that. TTL defaults to 10s, matching the original
// Python's per-endpoint memoize timeout.
// ------------------------------------------------------------------------------------------------

#[derive(Clone)]
struct CachedResponse {
    body: Vec<u8>,
    content_type: String,
    expires_at: Instant,
}

struct ResponseCache {
    inner: RwLock<HashMap<String, CachedResponse>>,
    ttl: Duration,
    /// Hard upper bound on the number of cached entries. When exceeded,
    /// the oldest-expiring entries are evicted until the cap is met.
    capacity: usize,
}

impl ResponseCache {
    fn new(ttl: Duration) -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
            ttl,
            capacity: 1024,
        }
    }
    async fn get(&self, key: &str) -> Option<CachedResponse> {
        let g = self.inner.read().await;
        match g.get(key) {
            Some(c) if c.expires_at > Instant::now() => Some(c.clone()),
            _ => None,
        }
    }
    async fn len(&self) -> usize {
        self.inner.read().await.len()
    }
    async fn set(&self, key: String, body: Vec<u8>, content_type: String) {
        let mut g = self.inner.write().await;
        // Drop expired entries first, then enforce the hard cap by evicting
        // the entries with the earliest expiry until we're under capacity.
        let now = Instant::now();
        g.retain(|_, v| v.expires_at > now);
        if g.len() >= self.capacity {
            let mut entries: Vec<(String, Instant)> =
                g.iter().map(|(k, v)| (k.clone(), v.expires_at)).collect();
            entries.sort_by_key(|(_, t)| *t);
            let to_drop = g.len().saturating_sub(self.capacity / 2);
            for (k, _) in entries.into_iter().take(to_drop) {
                g.remove(&k);
            }
        }
        g.insert(
            key,
            CachedResponse {
                body,
                content_type,
                expires_at: Instant::now() + self.ttl,
            },
        );
    }
}

async fn cache_middleware(
    axum::extract::State(cache): axum::extract::State<Arc<ResponseCache>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // Streaming endpoints are infinite — never read them to completion.
    if req.uri().path().starts_with("/v1/stream/") {
        return next.run(req).await;
    }
    let key = format!("{}?{}", req.uri().path(), req.uri().query().unwrap_or(""));
    if let Some(cached) = cache.get(&key).await {
        METRICS.cache_hits.fetch_add(1, Ordering::Relaxed);
        METRICS.record_status(StatusCode::OK);
        let mut resp = (StatusCode::OK, cached.body).into_response();
        resp.headers_mut().insert(
            "Content-Type",
            HeaderValue::from_str(&cached.content_type)
                .unwrap_or_else(|_| HeaderValue::from_static("application/json")),
        );
        resp.headers_mut()
            .insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
        resp.headers_mut()
            .insert("X-Cache", HeaderValue::from_static("HIT"));
        return resp;
    }
    METRICS.cache_misses.fetch_add(1, Ordering::Relaxed);
    let resp = next.run(req).await;
    let status = resp.status();
    METRICS.record_status(status);
    if status != StatusCode::OK {
        return resp; // don't cache non-200
    }
    let content_type = resp
        .headers()
        .get("Content-Type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json")
        .to_string();
    let (parts, body) = resp.into_parts();
    let bytes = match axum::body::to_bytes(body, 16 * 1024 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => return Response::from_parts(parts, Body::from(Vec::<u8>::new())),
    };
    cache.set(key, bytes.clone(), content_type).await;
    let mut resp = Response::from_parts(parts, Body::from(bytes));
    resp.headers_mut()
        .insert("X-Cache", HeaderValue::from_static("MISS"));
    resp
}

// ------------------------------------------------------------------------------------------------
// SSE position streams — emit a fresh position every `interval` seconds.
// ------------------------------------------------------------------------------------------------

fn parse_interval_seconds(opt: Option<&String>) -> u64 {
    opt.and_then(|s| s.parse::<u64>().ok())
        .map(|n| n.clamp(1, 3600))
        .unwrap_or(60)
}

async fn stream_sun_endpoint(
    headers: axum::http::HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let interval = parse_interval_seconds(q.get("interval"));
    let zod = match parse_stream_zodiac(&q) {
        Ok(z) => z,
        Err(e) => return bad_request(&e),
    };
    let start_id = parse_last_event_id(&headers);
    let stream = position_stream("Sun".to_string(), interval, zod, start_id);
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

async fn stream_moon_endpoint(
    headers: axum::http::HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let interval = parse_interval_seconds(q.get("interval"));
    let zod = match parse_stream_zodiac(&q) {
        Ok(z) => z,
        Err(e) => return bad_request(&e),
    };
    let start_id = parse_last_event_id(&headers);
    let stream = position_stream("Moon".to_string(), interval, zod, start_id);
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

async fn stream_body_endpoint(
    AxumPath(name): AxumPath<String>,
    headers: axum::http::HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let canonical = match canonical_body_name(&name) {
        Some(c) => c.to_string(),
        None => return not_found(&format!("unknown body: {name}")),
    };
    let interval = parse_interval_seconds(q.get("interval"));
    let zod = match parse_stream_zodiac(&q) {
        Ok(z) => z,
        Err(e) => return bad_request(&e),
    };
    let start_id = parse_last_event_id(&headers);
    let stream = position_stream(canonical, interval, zod, start_id);
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Read Last-Event-ID, parse as u64, return as the seed value the next
/// emitted event should overshoot. Default 0 — first event becomes id=1.
fn parse_last_event_id(headers: &axum::http::HeaderMap) -> u64 {
    headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}

/// Returns Ok(None) for tropical, Ok(Some((mode, label))) for sidereal.
fn parse_stream_zodiac(q: &HashMap<String, String>) -> Result<Option<(i32, &'static str)>, String> {
    match q.get("zodiac").map(|s| s.to_ascii_lowercase()).as_deref() {
        None | Some("") | Some("tropical") => Ok(None),
        Some("sidereal") => {
            let name = q.get("ayanamsha").map(|s| s.as_str()).unwrap_or("lahiri");
            parse_ayanamsha(name)
                .map(Some)
                .ok_or_else(|| format!("unknown ayanamsha: {name}"))
        }
        Some(other) => Err(format!("zodiac must be tropical or sidereal: {other}")),
    }
}

async fn stream_events_endpoint(
    headers: axum::http::HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
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
    let jd_end = match q.get("date_end") {
        Some(s) => match parse_jd_or_iso_date_in_tz(s, tz) {
            Ok(j) => j,
            Err(e) => return bad_request(&e),
        },
        None => jd_start + 30.0,
    };
    let split = |key: &str| -> Option<Vec<String>> {
        q.get(key)
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.split(',')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect()
            })
            .filter(|v: &Vec<String>| !v.is_empty())
    };
    let filter = EventFilter {
        types: split("types"),
        subtypes: split("subtypes"),
        planets: split("planets"),
        datas: split("datas"),
    };
    let events = match get_events(&dbfile, jd_start, jd_end, i64::MAX, &filter) {
        Ok(v) => v,
        Err(e) => return bad_request(&format!("event query failed: {e}")),
    };

    let start_id = parse_last_event_id(&headers);
    // Honour Last-Event-ID by skipping the (start_id) earliest events.
    let skip = start_id as usize;
    let interval_ms = q
        .get("interval_ms")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(50)
        .clamp(1, 5_000);

    let stream = stream::unfold(
        (events.into_iter().skip(skip).enumerate(), interval_ms),
        move |(mut iter, ms)| async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            let (idx, ev) = iter.next()?;
            let id = (start_id + 1) + idx as u64;
            let payload = json!({
                "jd": ev.jd,
                "iso_date": ev.iso_date,
                "type": ev.r#type,
                "subtype": ev.subtype,
                "planet": ev.planet,
                "data": ev.data,
                "delta_days": ev.delta_days,
            });
            let event = SseEvent::default()
                .event("event")
                .id(id.to_string())
                .data(payload.to_string());
            Some((Ok::<_, Infallible>(event), (iter, ms)))
        },
    );

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn position_stream(
    canonical: String,
    interval_seconds: u64,
    zodiac: Option<(i32, &'static str)>,
    start_id: u64,
) -> impl Stream<Item = Result<SseEvent, Infallible>> {
    let mut ticker = tokio::time::interval(Duration::from_secs(interval_seconds));
    // Fire on first poll and then at each interval.
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    stream::unfold(
        (canonical, ticker, zodiac, start_id),
        |(name, mut ticker, zod, mut id)| async move {
            ticker.tick().await;
            id += 1;
            let jd = jd_now();
            let planet = match body_for(&name, jd) {
                Some(p) => p,
                None => return None,
            };
            let trop_lon = planet.longitude_at(jd);
            let (lon, ayan_label, ayan_deg) = match zod {
                Some((mode, label)) => {
                    let ayan = compute_ayanamsha(jd, mode);
                    (apply_ayanamsha(trop_lon, ayan), label, Some(ayan))
                }
                None => (trop_lon, "tropical", None),
            };
            let pos = PlanetLongitude::new(lon);
            let payload = json!({
                "body": name,
                "jd": jd,
                "iso_date": jd2iso(jd),
                "longitude": lon,
                "speed": planet.speed(None),
                "is_rx": planet.is_rx(None),
                "position": planet_longitude_to_json(&pos),
                "zodiac": ayan_label,
                "ayanamsha_degrees": ayan_deg,
            });
            // Include id: line so SSE clients can resume via Last-Event-ID
            // on reconnect (browsers send the last id back as the
            // Last-Event-ID HTTP header automatically).
            let event = SseEvent::default()
                .event("position")
                .id(id.to_string())
                .data(payload.to_string());
            Some((Ok(event), (name, ticker, zod, id)))
        },
    )
}

async fn openapi_endpoint() -> Response {
    let mut resp = json_ok(openapi_spec());
    // OpenAPI spec rarely changes within a deployment, so let intermediaries
    // and clients cache for an hour.
    resp.headers_mut().insert(
        "Cache-Control",
        HeaderValue::from_static("public, max-age=3600"),
    );
    resp
}

// ------------------------------------------------------------------------------------------------
// Rate limiter — sliding window per client IP. Cheap, in-memory; not a
// substitute for an upstream load-balancer policy in serious deployments,
// but enough to keep a single misbehaving client from monopolising the
// expensive search endpoints.
// ------------------------------------------------------------------------------------------------

#[derive(Clone)]
struct RateLimiter {
    inner: Arc<RwLock<HashMap<String, Vec<Instant>>>>,
    window: Duration,
    max: usize,
}

struct RateLimitDecision {
    allowed: bool,
    /// Seconds until the oldest entry in the client's window expires.
    /// Only meaningful when `allowed = false`.
    retry_after_seconds: u64,
}

impl RateLimiter {
    fn new(window: Duration, max: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            window,
            max,
        }
    }
    /// Returns the decision plus, when denied, the seconds the client
    /// should wait before retrying — derived from when the oldest
    /// in-window timestamp falls off.
    async fn check(&self, key: &str) -> RateLimitDecision {
        let mut g = self.inner.write().await;
        let now = Instant::now();
        let entry = g.entry(key.to_string()).or_default();
        entry.retain(|t| now.duration_since(*t) < self.window);
        if entry.len() >= self.max {
            // The oldest timestamp is the one that will fall off first.
            let oldest = *entry.iter().min().unwrap_or(&now);
            let elapsed = now.duration_since(oldest);
            let remaining = self
                .window
                .checked_sub(elapsed)
                .unwrap_or_else(|| Duration::from_secs(0));
            // Round up so a partial second still tells the client to wait.
            let secs = remaining.as_secs() + u64::from(remaining.subsec_millis() > 0);
            return RateLimitDecision {
                allowed: false,
                retry_after_seconds: secs,
            };
        }
        entry.push(now);
        // Opportunistic GC: trim the map periodically.
        if g.len() > 4096 {
            let cutoff = self.window;
            g.retain(|_, ts| ts.iter().any(|t| now.duration_since(*t) < cutoff));
        }
        RateLimitDecision {
            allowed: true,
            retry_after_seconds: 0,
        }
    }
}

/// Time every request and feed the duration into the latency histogram.
/// Also surfaces the duration as a `Server-Timing` response header so
/// browsers see it in the network tab without parsing /metrics.
/// Streams aren't measured (their wall-clock is dominated by the
/// keep-alive interval, not real work).
async fn latency_middleware(req: Request<Body>, next: Next) -> Response {
    if req.uri().path().starts_with("/v1/stream/") {
        return next.run(req).await;
    }
    METRICS.in_flight.fetch_add(1, Ordering::Relaxed);
    let start = Instant::now();
    let mut resp = next.run(req).await;
    let elapsed = start.elapsed();
    METRICS.observe_latency(elapsed);
    METRICS.in_flight.fetch_sub(1, Ordering::Relaxed);
    let ms = elapsed.as_secs_f64() * 1000.0;
    if let Ok(v) = HeaderValue::from_str(&format!("total;dur={ms:.1}")) {
        resp.headers_mut().insert("Server-Timing", v);
    }
    resp
}

/// API-key gate. Static state-less middleware with the configured key
/// captured at start-up. /v1/* is gated; everything else is public.
async fn api_key_middleware(
    axum::extract::State(key): axum::extract::State<Arc<String>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let path = req.uri().path();
    if !path.starts_with("/v1/") {
        return next.run(req).await;
    }
    let presented = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    // Constant-time comparison to avoid timing leaks.
    let expected: &[u8] = key.as_bytes();
    let got: &[u8] = presented.as_bytes();
    let ok = expected.len() == got.len()
        && expected
            .iter()
            .zip(got.iter())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0;
    if !ok {
        return error_response(StatusCode::UNAUTHORIZED, "missing or invalid X-API-Key");
    }
    next.run(req).await
}

async fn rate_limit_middleware(
    axum::extract::State(rl): axum::extract::State<RateLimiter>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // Health and metrics bypass the limit so monitoring isn't gated.
    let path = req.uri().path();
    if path == "/health" || path == "/metrics" || path.starts_with("/v1/stream/") {
        return next.run(req).await;
    }
    // Best-effort client identifier: X-Forwarded-For first, then X-Real-IP,
    // else the connection remote (which is uniform behind a reverse proxy
    // and so effectively a global limit — acceptable as a fallback).
    let key = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or("").trim().to_string())
        .or_else(|| {
            req.headers()
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());
    let allowed = rl.check(&key).await;
    if !allowed.allowed {
        METRICS.rate_limited.fetch_add(1, Ordering::Relaxed);
        METRICS.record_status(StatusCode::TOO_MANY_REQUESTS);
        let mut resp = error_response(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded");
        // Real Retry-After: seconds until the oldest entry in the
        // client's window expires.
        let retry_after = allowed.retry_after_seconds.max(1);
        if let Ok(v) = HeaderValue::from_str(&retry_after.to_string()) {
            resp.headers_mut().insert("Retry-After", v);
        }
        return resp;
    }
    next.run(req).await
}

// ------------------------------------------------------------------------------------------------
// Metrics — atomic counters tracked from the cache and rate-limit
// middleware. Kept tiny and lock-free so the hot path doesn't pay for it.
// ------------------------------------------------------------------------------------------------

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
struct Metrics {
    requests_total: AtomicU64,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    rate_limited: AtomicU64,
    /// status_2xx, _3xx, _4xx, _5xx
    status_classes: [AtomicU64; 4],
    /// Snapshot of rate-limit configuration for monitoring.
    rate_limit_max: AtomicU64,
    rate_limit_window_seconds: AtomicU64,
    /// Snapshot of cache TTL for monitoring.
    cache_ttl_seconds: AtomicU64,
    /// Latency histograms keyed by (route_class, bucket).
    /// Buckets in seconds: 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, +Inf.
    latency_buckets_2xx: [AtomicU64; 9],
    latency_sum_us: AtomicU64,
    latency_count: AtomicU64,
    /// Currently-in-flight (non-stream) requests.
    in_flight: AtomicU64,
}

const LATENCY_BUCKETS_S: [f64; 8] = [0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0];

impl Metrics {
    fn observe_latency(&self, dur: Duration) {
        let secs = dur.as_secs_f64();
        let us = dur.as_micros() as u64;
        self.latency_sum_us.fetch_add(us, Ordering::Relaxed);
        self.latency_count.fetch_add(1, Ordering::Relaxed);
        // Cumulative: every bucket whose upper bound >= secs gets ++.
        for (i, &upper) in LATENCY_BUCKETS_S.iter().enumerate() {
            if secs <= upper {
                self.latency_buckets_2xx[i].fetch_add(1, Ordering::Relaxed);
            }
        }
        // The +Inf bucket counts every observation.
        self.latency_buckets_2xx[8].fetch_add(1, Ordering::Relaxed);
    }
}

impl Metrics {
    fn record_status(&self, status: StatusCode) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        let bucket = match status.as_u16() / 100 {
            2 => 0,
            3 => 1,
            4 => 2,
            _ => 3,
        };
        self.status_classes[bucket].fetch_add(1, Ordering::Relaxed);
    }
}

static METRICS: once_cell::sync::Lazy<Metrics> = once_cell::sync::Lazy::new(Metrics::default);

fn histogram_lines() -> String {
    let mut out = String::new();
    for (i, &upper) in LATENCY_BUCKETS_S.iter().enumerate() {
        let n = METRICS.latency_buckets_2xx[i].load(Ordering::Relaxed);
        out.push_str(&format!(
            "cerridwen_request_duration_seconds_bucket{{le=\"{upper}\"}} {n}\n"
        ));
    }
    let inf = METRICS.latency_buckets_2xx[8].load(Ordering::Relaxed);
    out.push_str(&format!(
        "cerridwen_request_duration_seconds_bucket{{le=\"+Inf\"}} {inf}\n"
    ));
    out
}

// ------------------------------------------------------------------------------------------------
// Liveness + metrics
// ------------------------------------------------------------------------------------------------

static SERVER_STARTED_AT: once_cell::sync::Lazy<Instant> = once_cell::sync::Lazy::new(Instant::now);

async fn health_endpoint() -> Response {
    let uptime = SERVER_STARTED_AT.elapsed().as_secs();
    // Real liveness check: the server is only useful if Swiss Ephemeris
    // can compute *something*. Compute the Sun's longitude at jd_now and
    // verify we get a sane value back. If sweph is broken (missing files,
    // wrong path, panic), fail closed with 503.
    let ephe_ok = std::panic::catch_unwind(|| {
        use cerridwen::planets::{Planet, SE_SUN};
        let jd = cerridwen::jd_now();
        let p = Planet::new(SE_SUN, Some(jd), None);
        let lon = p.longitude_at(jd);
        lon.is_finite() && (0.0..=360.0).contains(&lon)
    })
    .unwrap_or(false);

    let status_str = if ephe_ok { "ok" } else { "degraded" };
    let body = json!({
        "status": status_str,
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": uptime,
        "ephemeris_ok": ephe_ok,
    });
    let code = if ephe_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let mut resp = (code, body.to_string()).into_response();
    resp.headers_mut()
        .insert("Content-Type", HeaderValue::from_static("application/json"));
    resp.headers_mut()
        .insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
    resp
}

async fn metrics_endpoint(
    axum::extract::State(cache): axum::extract::State<Arc<ResponseCache>>,
) -> Response {
    // Prometheus exposition format. Stays text/plain so curl/scrapers
    // are happy without parsing JSON.
    let uptime = SERVER_STARTED_AT.elapsed().as_secs();
    let cache_size = cache.len().await;
    let cache_hits = METRICS.cache_hits.load(Ordering::Relaxed);
    let cache_misses = METRICS.cache_misses.load(Ordering::Relaxed);
    let total_reqs = METRICS.requests_total.load(Ordering::Relaxed);
    let rl_hits = METRICS.rate_limited.load(Ordering::Relaxed);
    let s2 = METRICS.status_classes[0].load(Ordering::Relaxed);
    let s3 = METRICS.status_classes[1].load(Ordering::Relaxed);
    let s4 = METRICS.status_classes[2].load(Ordering::Relaxed);
    let s5 = METRICS.status_classes[3].load(Ordering::Relaxed);
    let body = format!(
        "# HELP cerridwen_uptime_seconds Process uptime\n\
         # TYPE cerridwen_uptime_seconds counter\n\
         cerridwen_uptime_seconds {uptime}\n\
         # HELP cerridwen_cache_entries Current number of entries in the response cache\n\
         # TYPE cerridwen_cache_entries gauge\n\
         cerridwen_cache_entries {cache_size}\n\
         # HELP cerridwen_cache_hits_total Cache hits since startup\n\
         # TYPE cerridwen_cache_hits_total counter\n\
         cerridwen_cache_hits_total {cache_hits}\n\
         # HELP cerridwen_cache_misses_total Cache misses since startup\n\
         # TYPE cerridwen_cache_misses_total counter\n\
         cerridwen_cache_misses_total {cache_misses}\n\
         # HELP cerridwen_requests_total Total HTTP requests handled\n\
         # TYPE cerridwen_requests_total counter\n\
         cerridwen_requests_total {total_reqs}\n\
         # HELP cerridwen_rate_limit_rejections_total Requests rejected by the rate limiter\n\
         # TYPE cerridwen_rate_limit_rejections_total counter\n\
         cerridwen_rate_limit_rejections_total {rl_hits}\n\
         # HELP cerridwen_responses_total Responses by status class\n\
         # TYPE cerridwen_responses_total counter\n\
         cerridwen_responses_total{{class=\"2xx\"}} {s2}\n\
         cerridwen_responses_total{{class=\"3xx\"}} {s3}\n\
         cerridwen_responses_total{{class=\"4xx\"}} {s4}\n\
         cerridwen_responses_total{{class=\"5xx\"}} {s5}\n\
         # HELP cerridwen_rate_limit_max Configured per-client request limit\n\
         # TYPE cerridwen_rate_limit_max gauge\n\
         cerridwen_rate_limit_max {rl_max}\n\
         # HELP cerridwen_rate_limit_window_seconds Rate-limit window in seconds\n\
         # TYPE cerridwen_rate_limit_window_seconds gauge\n\
         cerridwen_rate_limit_window_seconds {rl_win}\n\
         # HELP cerridwen_cache_ttl_seconds Configured response-cache TTL\n\
         # TYPE cerridwen_cache_ttl_seconds gauge\n\
         cerridwen_cache_ttl_seconds {cache_ttl}\n\
         # HELP cerridwen_requests_inflight In-flight non-stream requests right now\n\
         # TYPE cerridwen_requests_inflight gauge\n\
         cerridwen_requests_inflight {in_flight}\n\
         # HELP cerridwen_request_duration_seconds Latency histogram\n\
         # TYPE cerridwen_request_duration_seconds histogram\n\
         {hist_lines}\
         cerridwen_request_duration_seconds_sum {hist_sum}\n\
         cerridwen_request_duration_seconds_count {hist_count}\n\
         # HELP cerridwen_build_info Static build info\n\
         # TYPE cerridwen_build_info gauge\n\
         cerridwen_build_info{{version=\"{ver}\"}} 1\n",
        hist_lines = histogram_lines(),
        hist_sum = METRICS.latency_sum_us.load(Ordering::Relaxed) as f64 / 1_000_000.0,
        hist_count = METRICS.latency_count.load(Ordering::Relaxed),
        in_flight = METRICS.in_flight.load(Ordering::Relaxed),
        rl_max = METRICS.rate_limit_max.load(Ordering::Relaxed),
        rl_win = METRICS.rate_limit_window_seconds.load(Ordering::Relaxed),
        cache_ttl = METRICS.cache_ttl_seconds.load(Ordering::Relaxed),
        ver = env!("CARGO_PKG_VERSION"),
    );
    let mut resp = (StatusCode::OK, body).into_response();
    resp.headers_mut().insert(
        "Content-Type",
        HeaderValue::from_static("text/plain; version=0.0.4"),
    );
    resp
}

async fn robots_endpoint() -> Response {
    // Block crawlers from /v1/* (every hit is a real compute cycle and
    // many of the endpoints are expensive). Allow the landing page,
    // chart wheel, OpenAPI spec, and rapidoc UI — those are static-ish
    // and useful for indexing.
    let body = "User-agent: *\n\
                Disallow: /v1/\n\
                Allow: /\n\
                Allow: /app\n\
                Allow: /chart\n\
                Allow: /docs\n\
                Allow: /openapi.json\n";
    let mut resp = (StatusCode::OK, body.to_string()).into_response();
    resp.headers_mut().insert(
        "Content-Type",
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    resp.headers_mut().insert(
        "Cache-Control",
        HeaderValue::from_static("public, max-age=86400"),
    );
    resp
}

async fn favicon_endpoint() -> Response {
    // Tiny SVG favicon — a purple crescent moon glyph. Inline avoids 404
    // chatter when browsers autofetch /favicon.ico.
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32"><circle cx="16" cy="16" r="14" fill="#0e0e14"/><text x="16" y="22" font-size="20" text-anchor="middle" fill="#9b59b6">☽</text></svg>"##;
    let mut resp = (StatusCode::OK, svg.to_string()).into_response();
    resp.headers_mut()
        .insert("Content-Type", HeaderValue::from_static("image/svg+xml"));
    resp.headers_mut().insert(
        "Cache-Control",
        HeaderValue::from_static("public, max-age=86400"),
    );
    resp
}

async fn app_endpoint() -> Response {
    let html = include_str!("../../../webapp/app.html");
    let mut resp = (StatusCode::OK, html.to_string()).into_response();
    resp.headers_mut().insert(
        "Content-Type",
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    resp
}

async fn chart_endpoint() -> Response {
    // Embedded at compile time; needs no static-file routing.
    let html = include_str!("../../../chart/chart.html");
    let mut resp = (StatusCode::OK, html.to_string()).into_response();
    resp.headers_mut().insert(
        "Content-Type",
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    resp
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
    resp.headers_mut().insert(
        "Content-Type",
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
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
    let long_param = p_number(
        "longitude",
        "Observer longitude in degrees (-180..180)",
        false,
    );
    let tz_param = p_string("tz", "IANA timezone name (e.g. Europe/Berlin)", false);
    let zodiac_param = p_string("zodiac", "tropical (default) or sidereal", false);
    let ayan_param = p_string(
        "ayanamsha",
        "lahiri/krishnamurti/fagan_bradley/raman/yukteshwar/...",
        false,
    );

    let common_params = json!([
        date_param,
        lat_param,
        long_param,
        tz_param,
        zodiac_param,
        ayan_param
    ]);

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
            "/v1/aspects": {
                "get": {
                    "summary": "Instantaneous aspect grid between every pair of planets",
                    "parameters": json!([
                        date_param, tz_param, lat_param, long_param,
                        p_number("orb", "Orb in degrees (default 5)", false),
                        p_string("include", "Roster opt-ins: comma-separated subset of nodes,asteroids,chiron,lilith", false),
                        {"name":"include_angles","in":"query","required":false,
                         "description":"Include Asc/MC as virtual bodies (requires latitude+longitude)",
                         "schema":{"type":"boolean"}},
                        p_string("house_system", "House system code/name when include_angles=1", false),
                    ]),
                    "responses": {
                        "200": { "description": "Aspect array" },
                        "400": { "description": "Bad request (e.g. include_angles=1 without observer)" }
                    }
                }
            },
            "/v1/transits": {
                "get": {
                    "summary": "Active transit-to-natal aspects",
                    "parameters": json!([
                        p_string("natal_date", "ISO date or JD of natal chart", true),
                        p_string("transit_date", "ISO date or JD of transit moment (default now)", false),
                        p_number("orb", "Orb in degrees (default 1.5)", false),
                        p_string("include", "Roster opt-ins: comma-separated subset of nodes,asteroids,chiron,lilith", false),
                        {"name":"include_angles","in":"query","required":false,
                         "description":"Include Asc/MC as virtual bodies (uses natal_latitude/natal_longitude and/or latitude/longitude)",
                         "schema":{"type":"boolean"}},
                        p_number("natal_latitude", "Natal observer latitude (for natal Asc/MC)", false),
                        p_number("natal_longitude", "Natal observer longitude (for natal Asc/MC)", false),
                        lat_param, long_param,
                        p_string("house_system", "House system code/name when include_angles=1", false),
                        tz_param,
                    ]),
                    "responses": { "200": { "description": "Active aspects" } }
                }
            },
            "/v1/stream/sun": {
                "get": {
                    "summary": "Server-Sent-Events stream pushing the Sun's position",
                    "parameters": json!([
                        p_number("interval", "Seconds between events (default 60, clamped 1..3600)", false),
                        zodiac_param, ayan_param,
                    ]),
                    "responses": { "200": {
                        "description": "text/event-stream with `position` events carrying JSON payloads",
                        "content": {"text/event-stream": {}}
                    } }
                }
            },
            "/v1/stream/moon": {
                "get": {
                    "summary": "SSE stream of the Moon's position",
                    "parameters": json!([
                        p_number("interval", "Seconds between events (default 60)", false),
                        zodiac_param, ayan_param,
                    ]),
                    "responses": { "200": { "description": "SSE stream" } }
                }
            },
            "/v1/stream/body/{name}": {
                "get": {
                    "summary": "SSE stream of any body's position",
                    "parameters": json!([
                        {"name":"name","in":"path","required":true,
                         "description":"sun/moon/.../north_node/lilith/chiron/ceres/...",
                         "schema":{"type":"string"}},
                        p_number("interval", "Seconds between events", false),
                        zodiac_param, ayan_param,
                    ]),
                    "responses": { "200": { "description": "SSE stream" }, "404": { "description": "Unknown body" } }
                }
            },
            "/health": {
                "get": {
                    "summary": "Liveness probe (uptime + build version)",
                    "responses": { "200": { "description": "JSON status" } }
                }
            },
            "/metrics": {
                "get": {
                    "summary": "Prometheus metrics exposition",
                    "responses": { "200": {
                        "description": "Prometheus exposition format",
                        "content": {"text/plain": {}}
                    } }
                }
            },
            "/v1/star/{name}": {
                "get": {
                    "summary": "Fixed star position from the bundled sefstars.txt catalog",
                    "parameters": json!([
                        {"name": "name", "in": "path", "required": true,
                         "description": "Star name (Sirius, Vega, Spica, Regulus, Algol, ...) or Bayer designation",
                         "schema": {"type": "string"}},
                        date_param, tz_param, zodiac_param, ayan_param,
                    ]),
                    "responses": { "200": { "description": "Star data" }, "404": { "description": "Unknown star" } }
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
            "/v1/declinations": {
                "get": {
                    "summary": "Declination grid plus parallel/contraparallel aspects",
                    "parameters": json!([
                        date_param, tz_param,
                        p_number("orb", "Declination orb in degrees (default 1)", false),
                        {"name":"include_nodes","in":"query","required":false,
                         "schema":{"type":"boolean"}},
                        {"name":"include_asteroids","in":"query","required":false,
                         "schema":{"type":"boolean"}},
                    ]),
                    "responses": { "200": { "description": "Declination grid" } }
                }
            },
            "/v1/stations": {
                "get": {
                    "summary": "Upcoming retrograde / direct stations for one body",
                    "parameters": json!([
                        p_string("body", "Body name (Mercury, Venus, ...)", true),
                        p_string("date", "Search start (default now)", false),
                        p_number("lookahead", "Days forward (default 730)", false),
                        p_number("limit", "Max results (default 8)", false),
                        tz_param,
                    ]),
                    "responses": { "200": { "description": "Stations array" } }
                }
            },
            "/v1/twilight": {
                "get": {
                    "summary": "Sunrise, sunset, civil/nautical/astronomical twilight",
                    "parameters": json!([
                        date_param, lat_param, long_param, tz_param,
                    ]),
                    "responses": { "200": { "description": "Twilight schedule" } }
                }
            },
            "/v1/planetary-hours": {
                "get": {
                    "summary": "Chaldean planetary hours (24/day) for an observer",
                    "parameters": json!([
                        date_param, lat_param, long_param, tz_param,
                    ]),
                    "responses": { "200": { "description": "Array of 24 hours" } }
                }
            },
            "/v1/arabic-parts": {
                "get": {
                    "summary": "Hellenistic lots (Fortune, Spirit, Eros, Necessity, Courage, Victory, Nemesis)",
                    "parameters": json!([
                        date_param, lat_param, long_param, tz_param,
                        p_string("house_system", "Asc system (default P)", false),
                    ]),
                    "responses": { "200": { "description": "Lots" } }
                }
            },
            "/v1/profections": {
                "get": {
                    "summary": "Annual profections (whole-sign rotation) for a natal Asc and age",
                    "parameters": json!([
                        p_string("natal_date", "ISO date or JD of natal chart", true),
                        p_number("natal_latitude", "", true),
                        p_number("natal_longitude", "", true),
                        p_number("age", "Years since birth (integer)", true),
                        tz_param,
                    ]),
                    "responses": { "200": { "description": "Profected house + lord" } }
                }
            },
            "/v1/prenatal-eclipse": {
                "get": {
                    "summary": "Last solar + lunar eclipse before a natal moment",
                    "parameters": json!([
                        p_string("natal_date", "ISO date or JD", true),
                        tz_param,
                    ]),
                    "responses": { "200": { "description": "Pre-natal eclipses" } }
                }
            },
            "/v1/synastry": {
                "get": {
                    "summary": "Inter-aspect grid between two charts",
                    "parameters": json!([
                        p_string("date_a", "ISO date or JD for chart A", true),
                        p_string("date_b", "ISO date or JD for chart B", true),
                        p_number("orb", "Degrees (default 4)", false),
                        tz_param,
                    ]),
                    "responses": { "200": { "description": "Aspect array" } }
                }
            },
            "/v1/composite": {
                "get": {
                    "summary": "Composite chart (midpoint or Davison)",
                    "parameters": json!([
                        p_string("date_a", "ISO date or JD for chart A", true),
                        p_string("date_b", "ISO date or JD for chart B", true),
                        p_string("method", "midpoint (default) or davison", false),
                        p_number("latitude_a", "Davison only", false),
                        p_number("longitude_a", "Davison only", false),
                        p_number("latitude_b", "Davison only", false),
                        p_number("longitude_b", "Davison only", false),
                        tz_param,
                    ]),
                    "responses": { "200": { "description": "Composite chart" } }
                }
            },
            "/v1/progressions": {
                "get": {
                    "summary": "Secondary progressions or solar arc directions",
                    "parameters": json!([
                        p_string("natal_date", "ISO date or JD of natal chart", true),
                        p_string("date", "Target date (default now)", false),
                        p_string("method", "secondary (default) or solar_arc", false),
                        tz_param,
                    ]),
                    "responses": { "200": { "description": "Progressed chart" } }
                }
            },
        }
    })
}

async fn aspects_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let (jd_opt, latlong) = match parse_observer_and_jd(&q) {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    let jd = jd_opt.unwrap_or_else(jd_now);
    let orb: f64 = match q.get("orb") {
        Some(s) => match s.parse::<f64>() {
            Ok(v) if v > 0.0 && v < 30.0 => v,
            _ => return bad_request("orb must be in (0, 30) degrees"),
        },
        None => 5.0,
    };

    // Optional opt-ins for the body roster.
    let include_set: std::collections::HashSet<String> = q
        .get("include")
        .map(|s| {
            s.split(',')
                .map(|x| x.trim().to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default();
    let mut bodies: Vec<i32> = default_transit_bodies().to_vec();
    use cerridwen::planets::{
        SE_CERES, SE_CHIRON, SE_JUNO, SE_MEAN_APOG, SE_MEAN_NODE, SE_PALLAS, SE_VESTA,
    };
    if include_set.contains("nodes") {
        bodies.push(SE_MEAN_NODE);
    }
    if include_set.contains("lilith") {
        bodies.push(SE_MEAN_APOG);
    }
    if include_set.contains("chiron") {
        bodies.push(SE_CHIRON);
    }
    if include_set.contains("asteroids") {
        bodies.extend([SE_CERES, SE_PALLAS, SE_JUNO, SE_VESTA]);
    }

    // Optional ?include_angles=1 — needs an observer to compute Asc/MC.
    let mut extras: Vec<(String, f64, f64)> = Vec::new();
    let include_angles = parse_bool(q.get("include_angles"));
    if include_angles {
        let Some(ll) = latlong else {
            return bad_request("include_angles=1 requires latitude+longitude");
        };
        let system = match q.get("house_system") {
            Some(s) => match parse_house_system(s) {
                Some(c) => c,
                None => return bad_request(&format!("unknown house_system: {s}")),
            },
            None => 'P',
        };
        for (name, now, next) in angle_points(jd, ll.lat, ll.long, system) {
            extras.push((name, now, next));
        }
    }

    let aspects = compute_aspects_extended(jd, &bodies, &extras, orb);
    let arr: Vec<Value> = aspects.iter().map(instant_aspect_to_json).collect();
    json_ok(json!({
        "jd": jd,
        "iso_date": jd2iso(jd),
        "orb": orb,
        "aspects": arr,
        "include_angles": include_angles,
    }))
}

fn instant_aspect_to_json(t: &InstantAspect) -> Value {
    json!({
        "body_a": t.body_a,
        "body_b": t.body_b,
        "aspect": t.aspect_name,
        "mode": t.aspect_mode,
        "exact_angle": t.exact_angle,
        "orb_distance": t.orb_distance,
        "applying": t.applying,
    })
}

async fn star_endpoint(
    AxumPath(name): AxumPath<String>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let (jd_opt, _latlong) = match parse_observer_and_jd(&q) {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    let jd = jd_opt.unwrap_or_else(jd_now);

    let star = match fixed_star(&name, jd) {
        Ok(s) => s,
        Err(e) if e.contains("not found") || e.contains("not contained") => {
            return not_found(&format!("unknown star: {name} ({e})"));
        }
        Err(e) => {
            return bad_request(&format!("fixstar lookup failed: {e}"));
        }
    };

    let (ayan, ayan_name) = match parse_zodiac(&q, jd) {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    let lon = if ayan != 0.0 {
        apply_ayanamsha(star.longitude, ayan)
    } else {
        star.longitude
    };
    let pos = PlanetLongitude::new(lon);

    json_ok(json!({
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

async fn return_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let tz = q.get("tz").map(|s| s.as_str());
    let body_name = match q.get("body") {
        Some(s) => s.as_str(),
        None => return bad_request("required: body=<Sun|Moon|Mercury|...>"),
    };
    let canonical = match canonical_body_name(body_name) {
        Some(c) => c,
        None => return not_found(&format!("unknown body: {body_name}")),
    };
    let body_planet = match body_for(canonical, 0.0) {
        Some(p) => p,
        None => return not_found(&format!("unknown body: {body_name}")),
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
        None => {
            return bad_request(&format!(
                "no return found for {canonical} within typical period"
            ))
        }
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
        q.get(key)
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.split(',')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect()
            })
            .filter(|v: &Vec<String>| !v.is_empty())
    };
    let filter = EventFilter {
        types: split("types"),
        subtypes: split("subtypes"),
        planets: split("planets"),
        datas: split("datas"),
    };

    let events = match get_events(&dbfile, jd_start, jd_end, limit, &filter) {
        Ok(v) => v,
        Err(e) => return bad_request(&format!("event query failed: {e}")),
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
        let uid = format!(
            "cerridwen-{}-{}-{}-{}@cerridwen",
            ev.r#type, ev.planet, ev.data, ev.jd as i64
        );
        ics.push_str("BEGIN:VEVENT\r\n");
        ics.push_str(&format!("UID:{uid}\r\n"));
        ics.push_str(&format!("DTSTAMP:{utc}\r\n"));
        ics.push_str(&format!("DTSTART:{utc}\r\n"));
        ics.push_str(&format!("DTEND:{utc_end}\r\n"));
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
    resp.headers_mut().insert(
        "Content-Type",
        HeaderValue::from_static("text/calendar; charset=utf-8"),
    );
    resp.headers_mut()
        .insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
    resp
}

/// Produce a UTC iCal-basic timestamp (YYYYMMDDTHHMMSSZ) from a JD.
fn jd_to_utc_basic(jd: f64) -> String {
    // Use the same revjul-based math jd2iso uses, then reformat.
    let iso = jd2iso(jd);
    // iso is "YYYY-MM-DD HH:MM:SS"
    let bytes = iso.as_bytes();
    if bytes.len() < 19 {
        return iso.to_string();
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
        "ingress" => format!("{p} enters {d}"),
        "rx" => format!("{p} stations retrograde in {d}"),
        "direct" => format!("{p} stations direct in {d}"),
        _ => {
            let mode = if st.is_empty() {
                String::new()
            } else {
                format!(" {st}")
            };
            format!("{p} {t}{mode} {d}")
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
    // Optional roster opt-ins, mirroring /v1/aspects.
    let include_set: std::collections::HashSet<String> = q
        .get("include")
        .map(|s| {
            s.split(',')
                .map(|x| x.trim().to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default();
    let mut bodies: Vec<i32> = default_transit_bodies().to_vec();
    use cerridwen::planets::{
        SE_CERES, SE_CHIRON, SE_JUNO, SE_MEAN_APOG, SE_MEAN_NODE, SE_PALLAS, SE_VESTA,
    };
    if include_set.contains("nodes") {
        bodies.push(SE_MEAN_NODE);
    }
    if include_set.contains("lilith") {
        bodies.push(SE_MEAN_APOG);
    }
    if include_set.contains("chiron") {
        bodies.push(SE_CHIRON);
    }
    if include_set.contains("asteroids") {
        bodies.extend([SE_CERES, SE_PALLAS, SE_JUNO, SE_VESTA]);
    }

    // Optional angles (Asc/MC) opt-ins. Natal angles use natal_latitude /
    // natal_longitude; transiting angles use the active observer (latitude /
    // longitude). All are optional; if missing, skip that side.
    let include_angles = parse_bool(q.get("include_angles"));
    let mut natal_extras: Vec<(String, f64, f64)> = Vec::new();
    let mut transit_extras: Vec<(String, f64, f64)> = Vec::new();
    if include_angles {
        let house_system = match q.get("house_system") {
            Some(s) => match parse_house_system(s) {
                Some(c) => c,
                None => return bad_request(&format!("unknown house_system: {s}")),
            },
            None => 'P',
        };
        let nlat = q.get("natal_latitude").and_then(|s| s.parse::<f64>().ok());
        let nlon = q.get("natal_longitude").and_then(|s| s.parse::<f64>().ok());
        let tlat = q.get("latitude").and_then(|s| s.parse::<f64>().ok());
        let tlon = q.get("longitude").and_then(|s| s.parse::<f64>().ok());
        if let (Some(la), Some(lo)) = (nlat, nlon) {
            for (n, a, b) in angle_points(natal_jd, la, lo, house_system) {
                natal_extras.push((format!("natal {n}"), a, b));
            }
        }
        if let (Some(la), Some(lo)) = (tlat, tlon) {
            for (n, a, b) in angle_points(transit_jd, la, lo, house_system) {
                transit_extras.push((format!("transit {n}"), a, b));
            }
        }
        if natal_extras.is_empty() && transit_extras.is_empty() {
            return bad_request(
                "include_angles=1 requires natal_latitude+natal_longitude \
                 and/or latitude+longitude (for the transit moment)",
            );
        }
    }

    let active = compute_transits_extended(
        natal_jd,
        transit_jd,
        &bodies,
        &natal_extras,
        &transit_extras,
        orb,
    );
    let arr: Vec<Value> = active.iter().map(transit_to_json).collect();
    let mut o = serde_json::Map::new();
    o.insert("natal_jd".into(), json!(natal_jd));
    o.insert("natal_iso".into(), json!(jd2iso(natal_jd)));
    o.insert("transit_jd".into(), json!(transit_jd));
    o.insert("transit_iso".into(), json!(jd2iso(transit_jd)));
    o.insert("orb".into(), json!(orb));
    o.insert("include_angles".into(), json!(include_angles));
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
        Some(other) => {
            return bad_request(&format!(
                "type must be one of: solar, lunar, both. Got: {other}"
            ))
        }
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
                    .map(|(c, name)| format!("{c}={name}"))
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
    let cusps: Vec<Value> = h
        .cusps
        .iter()
        .map(|&deg| {
            json!({
                "absolute_degrees": deg,
                "sign": cerridwen::PlanetLongitude::new(deg).sign(),
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
            None => return not_found(&format!("unknown body: {name}")),
        },
        None => return not_found(&format!("unknown body: {name}")),
    };

    let (ayan, ayan_name) = match parse_zodiac(&q, jd) {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };

    let center = match q.get("center") {
        Some(s) => match astrology::Center::parse(s) {
            Some(c) => c,
            None => return bad_request(&format!("unknown center: {s}")),
        },
        None => astrology::Center::Geo,
    };
    if center == astrology::Center::Topo {
        match latlong {
            Some(o) => astrology::set_topo(&o),
            None => return bad_request("center=topo requires latitude+longitude"),
        }
    }

    let trop_lon = match center {
        astrology::Center::Geo => planet.longitude_at(jd),
        _ => astrology::longitude_at(center, planet.id, jd),
    };
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
    o.insert("center".into(), json!(center.label()));
    o.insert("name".into(), json!(planet.name()));
    o.insert("position".into(), planet_longitude_to_json(&pos));
    o.insert("longitude".into(), json!(lon));
    o.insert("latitude".into(), json!(planet.latitude(None)));
    o.insert("declination".into(), json!(planet.declination(None)));
    o.insert("right_ascension".into(), json!(planet.rectascension(None)));
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

    if latlong.is_some() {
        // Build a fresh Planet with the observer set so rise/set work.
        let with_observer = Planet::new(planet.id, Some(jd), latlong);
        o.insert(
            "next_rise".into(),
            planet_event_to_json(&with_observer.next_rise()),
        );
        o.insert(
            "next_set".into(),
            planet_event_to_json(&with_observer.next_set()),
        );
        o.insert(
            "last_rise".into(),
            planet_event_to_json(&with_observer.last_rise()),
        );
        o.insert(
            "last_set".into(),
            planet_event_to_json(&with_observer.last_set()),
        );
    }

    json_ok(Value::Object(o))
}

// -- new endpoints ---------------------------------------------------------

fn collect_body_chart(
    q: &HashMap<String, String>,
    jd: f64,
) -> Result<Vec<(String, i32, f64)>, String> {
    use cerridwen::planets::*;
    let mut out: Vec<(String, i32, f64)> = vec![
        ("Sun".into(), SE_SUN, 0.0),
        ("Moon".into(), SE_MOON, 0.0),
        ("Mercury".into(), SE_MERCURY, 0.0),
        ("Venus".into(), SE_VENUS, 0.0),
        ("Mars".into(), SE_MARS, 0.0),
        ("Jupiter".into(), SE_JUPITER, 0.0),
        ("Saturn".into(), SE_SATURN, 0.0),
        ("Uranus".into(), SE_URANUS, 0.0),
        ("Neptune".into(), SE_NEPTUNE, 0.0),
        ("Pluto".into(), SE_PLUTO, 0.0),
    ];
    if q.get("include_nodes").map(|s| s == "1").unwrap_or(false) {
        out.push(("Mean Node".into(), SE_MEAN_NODE, 0.0));
        out.push(("True Node".into(), SE_TRUE_NODE, 0.0));
    }
    if q.get("include_asteroids")
        .map(|s| s == "1")
        .unwrap_or(false)
    {
        out.push(("Chiron".into(), SE_CHIRON, 0.0));
        out.push(("Ceres".into(), SE_CERES, 0.0));
        out.push(("Pallas".into(), SE_PALLAS, 0.0));
        out.push(("Juno".into(), SE_JUNO, 0.0));
        out.push(("Vesta".into(), SE_VESTA, 0.0));
    }
    for entry in &mut out {
        let id = entry.1;
        entry.2 = swisseph::swe::calc_ut(jd, id as u32, 2)
            .map(|r| r.out[0])
            .unwrap_or(f64::NAN);
    }
    Ok(out)
}

async fn declinations_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let (jd_opt, _) = match parse_observer_and_jd(&q) {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    let jd = jd_opt.unwrap_or_else(jd_now);
    let orb: f64 = match q.get("orb").map(|s| s.parse::<f64>()) {
        Some(Ok(v)) if v > 0.0 => v,
        Some(Ok(_)) => return bad_request("orb must be positive"),
        Some(Err(_)) => return bad_request("orb must be a number"),
        None => 1.0,
    };
    let chart = match collect_body_chart(&q, jd) {
        Ok(c) => c,
        Err(e) => return bad_request(&e),
    };
    let bodies: Vec<(String, i32)> = chart.iter().map(|(n, id, _)| (n.clone(), *id)).collect();
    let decs: Vec<Value> = bodies
        .iter()
        .map(|(name, id)| {
            json!({
                "body": name,
                "declination": astrology::declination(*id, jd),
            })
        })
        .collect();
    let parallels = astrology::declination_aspects(&bodies, jd, orb);
    let pj: Vec<Value> = parallels
        .iter()
        .map(|p| {
            json!({
                "a": p.a,
                "b": p.b,
                "kind": p.kind.label(),
                "orb": p.orb,
            })
        })
        .collect();
    json_ok(json!({
        "jd": jd,
        "iso_date": jd2iso(jd),
        "orb": orb,
        "declinations": decs,
        "parallels": pj,
        "moon_out_of_bounds": astrology::moon_out_of_bounds(jd),
    }))
}

async fn stations_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let tz = q.get("tz").map(|s| s.as_str());
    let body_name = match q.get("body") {
        Some(s) => s.as_str(),
        None => return bad_request("required: body=<Sun..Pluto|...>"),
    };
    let canonical = match canonical_body_name(body_name) {
        Some(c) => c,
        None => return not_found(&format!("unknown body: {body_name}")),
    };
    let planet = match body_for(canonical, 0.0) {
        Some(p) => p,
        None => return not_found(&format!("unknown body: {body_name}")),
    };
    let start_jd = match q.get("date") {
        Some(s) => match parse_jd_or_iso_date_in_tz(s, tz) {
            Ok(j) => j,
            Err(e) => return bad_request(&e),
        },
        None => jd_now(),
    };
    let lookahead: f64 = match q.get("lookahead").map(|s| s.parse::<f64>()) {
        Some(Ok(v)) if v > 0.0 => v,
        Some(Ok(_)) => return bad_request("lookahead must be positive"),
        Some(Err(_)) => return bad_request("lookahead must be a number"),
        None => 730.0,
    };
    let max: usize = match q.get("limit").map(|s| s.parse::<usize>()) {
        Some(Ok(v)) if v > 0 => v,
        Some(Ok(_)) => return bad_request("limit must be positive"),
        Some(Err(_)) => return bad_request("limit must be an integer"),
        None => 8,
    };
    let stations = astrology::upcoming_stations(planet.id, start_jd, lookahead, max);
    let arr: Vec<Value> = stations
        .iter()
        .map(|s| {
            json!({
                "jd": s.jd,
                "iso_date": s.iso_date,
                "kind": s.kind.label(),
                "longitude": s.longitude,
                "position": planet_longitude_to_json(&PlanetLongitude::new(s.longitude)),
            })
        })
        .collect();
    json_ok(json!({
        "body": canonical,
        "start_jd": start_jd,
        "lookahead_days": lookahead,
        "stations": arr,
    }))
}

async fn twilight_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let (jd_opt, latlong) = match parse_observer_and_jd(&q) {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    let observer = match latlong {
        Some(o) => o,
        None => return bad_request("required: latitude and longitude"),
    };
    let jd = jd_opt.unwrap_or_else(jd_now);
    let t = astrology::twilight_times(jd, &observer);
    let pair = |a: f64, b: f64| -> Value {
        json!({
            "start_jd": a, "start_iso": jd2iso(a),
            "end_jd":   b, "end_iso":   jd2iso(b),
        })
    };
    json_ok(json!({
        "jd": jd,
        "iso_date": jd2iso(jd),
        "latitude": observer.lat,
        "longitude": observer.long,
        "sunrise": jd2iso(t.sunrise),
        "sunset": jd2iso(t.sunset),
        "civil": pair(t.civil_dawn, t.civil_dusk),
        "nautical": pair(t.nautical_dawn, t.nautical_dusk),
        "astronomical": pair(t.astronomical_dawn, t.astronomical_dusk),
    }))
}

async fn planetary_hours_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let (jd_opt, latlong) = match parse_observer_and_jd(&q) {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    let observer = match latlong {
        Some(o) => o,
        None => return bad_request("required: latitude and longitude"),
    };
    let jd = jd_opt.unwrap_or_else(jd_now);
    let hours = astrology::planetary_hours(jd, &observer);
    let arr: Vec<Value> = hours
        .iter()
        .map(|h| {
            json!({
                "index": h.index,
                "kind": h.kind,
                "ruler": h.ruler,
                "start_jd": h.start_jd, "start_iso": jd2iso(h.start_jd),
                "end_jd": h.end_jd, "end_iso": jd2iso(h.end_jd),
            })
        })
        .collect();
    let now = jd_now();
    let current = hours
        .iter()
        .find(|h| now >= h.start_jd && now < h.end_jd)
        .map(|h| {
            json!({
                "index": h.index, "kind": h.kind, "ruler": h.ruler,
                "start_iso": jd2iso(h.start_jd),
                "end_iso":   jd2iso(h.end_jd),
            })
        });
    json_ok(json!({
        "jd": jd,
        "iso_date": jd2iso(jd),
        "current": current,
        "hours": arr,
    }))
}

async fn arabic_parts_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let (jd_opt, latlong) = match parse_observer_and_jd(&q) {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    let observer = match latlong {
        Some(o) => o,
        None => return bad_request("required: latitude and longitude (for Asc)"),
    };
    let jd = jd_opt.unwrap_or_else(jd_now);

    let system = match q.get("house_system") {
        Some(s) => match parse_house_system(s) {
            Some(c) => c,
            None => return bad_request(&format!("unknown house_system: {s}")),
        },
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

    // Day chart if Sun above the horizon (between the 7th and 1st cusp,
    // i.e. from MC side going up through Asc).
    let dsc = (h.ascendant + 180.0).rem_euclid(360.0);
    let sun_arc = (sun - h.ascendant).rem_euclid(360.0);
    let dsc_arc = (dsc - h.ascendant).rem_euclid(360.0);
    let is_day = sun_arc >= dsc_arc; // sun in houses 7..12 means above horizon
    let parts = astrology::arabic_parts(
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
                "name": p.name,
                "longitude": p.longitude,
                "position": planet_longitude_to_json(&PlanetLongitude::new(p.longitude)),
                "formula": p.formula,
            })
        })
        .collect();
    json_ok(json!({
        "jd": jd,
        "iso_date": jd2iso(jd),
        "ascendant": h.ascendant,
        "is_day": is_day,
        "parts": arr,
    }))
}

async fn profections_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let tz = q.get("tz").map(|s| s.as_str());
    let natal_date = match q.get("natal_jd").or(q.get("natal_date")) {
        Some(s) => match parse_jd_or_iso_date_in_tz(s, tz) {
            Ok(j) => j,
            Err(e) => return bad_request(&e),
        },
        None => return bad_request("required: natal_jd or natal_date"),
    };
    let lat = match q.get("natal_latitude").map(|s| s.parse::<f64>()) {
        Some(Ok(v)) => v,
        _ => return bad_request("required: natal_latitude"),
    };
    let long = match q.get("natal_longitude").map(|s| s.parse::<f64>()) {
        Some(Ok(v)) => v,
        _ => return bad_request("required: natal_longitude"),
    };
    let age: u32 = match q.get("age").map(|s| s.parse::<u32>()) {
        Some(Ok(v)) => v,
        Some(Err(_)) => return bad_request("age must be a non-negative integer"),
        None => return bad_request("required: age"),
    };
    let h = compute_houses(natal_date, lat, long, 'P');
    let p = astrology::profection(h.ascendant, age);
    json_ok(json!({
        "natal_jd": natal_date,
        "natal_ascendant": h.ascendant,
        "age": p.age,
        "house": p.house,
        "sign": p.sign,
        "lord": p.lord,
    }))
}

async fn prenatal_eclipse_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let tz = q.get("tz").map(|s| s.as_str());
    let natal_jd = match q.get("natal_jd").or(q.get("natal_date")) {
        Some(s) => match parse_jd_or_iso_date_in_tz(s, tz) {
            Ok(j) => j,
            Err(e) => return bad_request(&e),
        },
        None => return bad_request("required: natal_jd or natal_date"),
    };
    let solar = astrology::pre_natal_solar_eclipse(natal_jd);
    let lunar = astrology::pre_natal_lunar_eclipse(natal_jd);
    let to_json = |e: &Eclipse| -> Value {
        json!({
            "jd": e.max_jd,
            "iso_date": jd2iso(e.max_jd),
            "kind": format!("{:?}", e.kind),
            "central": e.central,
        })
    };
    json_ok(json!({
        "natal_jd": natal_jd,
        "solar": solar.as_ref().map(to_json),
        "lunar": lunar.as_ref().map(to_json),
    }))
}

fn parse_chart_param(
    q: &HashMap<String, String>,
    suffix: &str,
) -> Result<(f64, Option<LatLong>), String> {
    let tz = q.get("tz").map(|s| s.as_str());
    let jd_key = format!("date_{suffix}");
    let date_str = q
        .get(&jd_key)
        .ok_or_else(|| format!("required: date_{suffix}"))?;
    let jd = parse_jd_or_iso_date_in_tz(date_str, tz)?;
    let lat_key = format!("latitude_{suffix}");
    let long_key = format!("longitude_{suffix}");
    let lat = q
        .get(&lat_key)
        .map(|s| s.parse::<f64>())
        .transpose()
        .map_err(|e| format!("invalid {lat_key}: {e}"))?;
    let long = q
        .get(&long_key)
        .map(|s| s.parse::<f64>())
        .transpose()
        .map_err(|e| format!("invalid {long_key}: {e}"))?;
    let latlong = match (lat, long) {
        (Some(la), Some(lo)) => Some(LatLong::new(la, lo).map_err(|s| s.to_string())?),
        (None, None) => None,
        _ => return Err(format!("Specify both {lat_key} and {long_key} or none")),
    };
    Ok((jd, latlong))
}

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

async fn synastry_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let (jd_a, _) = match parse_chart_param(&q, "a") {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    let (jd_b, _) = match parse_chart_param(&q, "b") {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    let orb: f64 = match q.get("orb").map(|s| s.parse::<f64>()) {
        Some(Ok(v)) if v > 0.0 => v,
        Some(Ok(_)) => return bad_request("orb must be positive"),
        Some(Err(_)) => return bad_request("orb must be a number"),
        None => 4.0,
    };
    let a = snapshot_longitudes(jd_a);
    let b = snapshot_longitudes(jd_b);
    let aspects = astrology::synastry(&a, &b, orb);
    let arr: Vec<Value> = aspects
        .iter()
        .map(|sa| {
            json!({
                "a": sa.a, "b": sa.b,
                "aspect": sa.aspect,
                "orb": sa.orb,
                "angle_a_to_b": sa.angle_a_to_b,
            })
        })
        .collect();
    json_ok(json!({
        "chart_a": { "jd": jd_a, "iso_date": jd2iso(jd_a) },
        "chart_b": { "jd": jd_b, "iso_date": jd2iso(jd_b) },
        "orb": orb,
        "aspects": arr,
    }))
}

async fn composite_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let (jd_a, loc_a) = match parse_chart_param(&q, "a") {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    let (jd_b, loc_b) = match parse_chart_param(&q, "b") {
        Ok(x) => x,
        Err(e) => return bad_request(&e),
    };
    let mode = q
        .get("method")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "midpoint".into());
    let a = snapshot_longitudes(jd_a);
    let b = snapshot_longitudes(jd_b);
    if mode == "davison" {
        let loc_a = match loc_a {
            Some(l) => l,
            None => return bad_request("davison composite needs latitude_a + longitude_a"),
        };
        let loc_b = match loc_b {
            Some(l) => l,
            None => return bad_request("davison composite needs latitude_b + longitude_b"),
        };
        let d = astrology::davison_chart(jd_a, jd_b, &loc_a, &loc_b);
        let chart = snapshot_longitudes(d.jd);
        let bodies: Vec<Value> = chart
            .iter()
            .map(|(n, lon)| {
                json!({
                    "name": n,
                    "longitude": lon,
                    "position": planet_longitude_to_json(&PlanetLongitude::new(*lon)),
                })
            })
            .collect();
        json_ok(json!({
            "method": "davison",
            "midpoint_jd": d.jd,
            "midpoint_iso": d.iso_date,
            "midpoint_latitude": d.latitude,
            "midpoint_longitude": d.longitude,
            "bodies": bodies,
        }))
    } else {
        let composite = astrology::midpoint_composite(&a, &b);
        let bodies: Vec<Value> = composite
            .iter()
            .map(|(n, lon)| {
                json!({
                    "name": n,
                    "longitude": lon,
                    "position": planet_longitude_to_json(&PlanetLongitude::new(*lon)),
                })
            })
            .collect();
        json_ok(json!({
            "method": "midpoint",
            "chart_a_iso": jd2iso(jd_a),
            "chart_b_iso": jd2iso(jd_b),
            "bodies": bodies,
        }))
    }
}

async fn progressions_endpoint(Query(q): Query<HashMap<String, String>>) -> Response {
    let tz = q.get("tz").map(|s| s.as_str());
    let natal_jd = match q.get("natal_jd").or(q.get("natal_date")) {
        Some(s) => match parse_jd_or_iso_date_in_tz(s, tz) {
            Ok(j) => j,
            Err(e) => return bad_request(&e),
        },
        None => return bad_request("required: natal_jd or natal_date"),
    };
    let target_jd = match q.get("date") {
        Some(s) => match parse_jd_or_iso_date_in_tz(s, tz) {
            Ok(j) => j,
            Err(e) => return bad_request(&e),
        },
        None => jd_now(),
    };
    let method = q
        .get("method")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "secondary".into());
    let natal = snapshot_longitudes(natal_jd);
    let body = match method.as_str() {
        "secondary" => {
            let pj = astrology::progressed_jd(natal_jd, target_jd);
            let prog = snapshot_longitudes(pj);
            let bodies: Vec<Value> = prog
                .iter()
                .map(|(n, lon)| {
                    json!({
                        "name": n,
                        "longitude": lon,
                        "position": planet_longitude_to_json(&PlanetLongitude::new(*lon)),
                    })
                })
                .collect();
            json!({
                "method": "secondary",
                "progressed_jd": pj,
                "progressed_iso": jd2iso(pj),
                "bodies": bodies,
            })
        }
        "solar_arc" => {
            let arc = astrology::solar_arc_offset(natal_jd, target_jd);
            let bodies: Vec<Value> = natal
                .iter()
                .map(|(n, lon)| {
                    let new = (lon + arc).rem_euclid(360.0);
                    json!({
                        "name": n,
                        "longitude": new,
                        "position": planet_longitude_to_json(&PlanetLongitude::new(new)),
                        "delta_deg": arc,
                    })
                })
                .collect();
            json!({
                "method": "solar_arc",
                "arc_deg": arc,
                "bodies": bodies,
            })
        }
        other => return bad_request(&format!("unknown method: {other}. Try secondary|solar_arc")),
    };
    let mut wrap = serde_json::Map::new();
    wrap.insert("natal_jd".into(), json!(natal_jd));
    wrap.insert("natal_iso".into(), json!(jd2iso(natal_jd)));
    wrap.insert("target_jd".into(), json!(target_jd));
    wrap.insert("target_iso".into(), json!(jd2iso(target_jd)));
    if let Value::Object(m) = body {
        for (k, v) in m {
            wrap.insert(k, v);
        }
    }
    json_ok(Value::Object(wrap))
}

// -- end new endpoints -----------------------------------------------------

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
        q.get(key)
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.split(',')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect()
            })
            .filter(|v: &Vec<String>| !v.is_empty())
    };

    let filter = EventFilter {
        types: split("types"),
        subtypes: split("subtypes"),
        planets: split("planets"),
        datas: split("datas"),
    };

    let events = match get_events(&dbfile, jd_start, jd_end, limit, &filter) {
        Ok(v) => v,
        Err(e) => return bad_request(&format!("event query failed: {e}")),
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
            obj.insert(
                "position".into(),
                planet_longitude_to_json(&p.position(None)),
            );
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
    let lat = q
        .get("latitude")
        .map(|s| s.parse::<f64>())
        .transpose()
        .map_err(|e| format!("invalid latitude: {e}"))?;
    let long = q
        .get("longitude")
        .map(|s| s.parse::<f64>())
        .transpose()
        .map_err(|e| format!("invalid longitude: {e}"))?;
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
    let mut resp = (
        StatusCode::OK,
        serde_json::to_string_pretty(&v).unwrap_or_default(),
    )
        .into_response();
    resp.headers_mut()
        .insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
    resp.headers_mut()
        .insert("Content-Type", HeaderValue::from_static("application/json"));
    resp
}

fn bad_request(msg: &str) -> Response {
    error_response(StatusCode::BAD_REQUEST, msg)
}

fn not_found(msg: &str) -> Response {
    error_response(StatusCode::NOT_FOUND, msg)
}

/// Structured JSON error envelope: `{"error": "...", "code": <status>}`.
/// Clients get a uniform shape they can parse without sniffing the
/// content-type or splitting strings.
fn error_response(status: StatusCode, msg: &str) -> Response {
    let body = json!({
        "error": msg,
        "code": status.as_u16(),
    })
    .to_string();
    let mut resp = (status, body).into_response();
    resp.headers_mut()
        .insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
    resp.headers_mut()
        .insert("Content-Type", HeaderValue::from_static("application/json"));
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
    {
        let t = astrology::tithi(d.jd);
        o.insert(
            "tithi".into(),
            json!({
                "number": t.number,
                "name": t.name,
                "half": t.half,
                "paksha_index": t.paksha_index,
                "elongation_deg": t.elongation_deg,
                "fraction_complete": t.fraction_complete,
            }),
        );
        // Nakshatra wants sidereal Moon longitude (Lahiri convention).
        let lahiri = compute_ayanamsha(d.jd, 1);
        let sid = apply_ayanamsha(d.position.absolute_degrees, lahiri);
        let n = astrology::nakshatra_sidereal(sid);
        o.insert(
            "nakshatra".into(),
            json!({
                "number": n.number,
                "name": n.name,
                "pada": n.pada,
                "lon_in_mansion": n.lon_in_mansion,
            }),
        );
    }
    o.insert(
        "out_of_bounds".into(),
        json!(astrology::moon_out_of_bounds(d.jd)),
    );
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
