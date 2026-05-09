# Changelog

All notable changes to the Rust port. Pre-1.x dates predate the port and
live in `NEWS.rst`.

## Unreleased

### Added

* Web app at `/app` covering every endpoint in a tabbed JS console.
  Deep-linkable tabs, ARIA support, mobile-responsive layout.
* Standalone chart-wheel page at `/chart` with aspect lines, retrograde
  markers, and 18-body roster (planets + nodes + Lilith + Chiron + the
  four major asteroids).
* `cerridwen-mcp` Model Context Protocol server over stdio with 10
  tools (`get_sun`, `get_moon`, `get_body`, `get_houses`,
  `get_aspects`, `get_transits`, `get_return`, `get_eclipses`,
  `get_star`, `get_events`).
* `/v1/aspects` and `/v1/transits` accept `?include=nodes,asteroids,
  chiron,lilith` and `?include_angles=1` (Asc/MC participate in the
  grid).
* `/v1/return` (solar/lunar/planetary returns), `/v1/star/{name}`
  (fixed-star catalog), `/v1/eclipses`, `/v1/houses` (18 systems),
  `/v1/events.ics` (RFC 5545 iCal feed), `/v1/stream/...` SSE position
  pushes.
* Tropical / sidereal zodiac with multiple ayanamshas (Lahiri,
  Krishnamurti, Fagan-Bradley, Raman, Yukteshwar, …) on every endpoint
  that returns longitudes.
* Time-zone-aware date input (`?tz=Europe/Berlin` etc.) via IANA names.
* CLI flags / env vars to configure bind address, cache TTL, rate-limit
  window/max.
* `/health` (uptime + version), `/metrics` (Prometheus exposition with
  request/cache/rate-limit counters and status-class breakdown),
  `tower-http`-based CORS layer, `tracing` structured logging.
* Multi-stage Dockerfile, systemd unit, nginx reverse-proxy example
  under `deploy/`.
* GitHub Actions workflow: build + test + clippy + rustfmt.
* OpenAPI 3.0 spec at `/openapi.json` with rapidoc UI at `/docs`.

### Changed

* Swiss Ephemeris path is set per-thread (`__thread` TLS), so axum
  worker threads don't silently fall back to Moshier formulae.
* `next_sign_change` now accepts retrograde-direction crossings; mean
  lunar node and other always-regressing bodies terminate.
* `try_next_sign_change` returns `Option<f64>` so callers can handle the
  rare "no crossing within lookahead" case gracefully (the panicking
  variant remains for callers that always expect one).
* Empty filter strings on `/v1/events` (`?types=&planets=`) no longer
  silently filter to zero results.
* SSE events now include `id:` lines so clients can resume via
  `Last-Event-ID` on reconnect.

### Fixed

* SVG attribute quoting on the chart wheel — unquoted `stroke-width=1.5/>`
  was making the first element swallow every subsequent SVG child.
* Pluto / true_node / asteroid `next_return` lookahead values added so
  the search doesn't blow `MAX_DATA_POINTS`.
* Mercury and Venus return lookahead bumped to 400 d (geocentric returns
  are annual, not synodic).
* `/v1/olivier` and the chart wheel now apply the requested ayanamsha
  consistently with the per-body endpoints.
