# Endpoint reference

Auto-generated structure mirrors `/openapi.json` at the live server.
For the machine-readable schema, hit that URL or browse `/docs`.

## Body data

| Path                      | Returns                                         |
| ------------------------- | ----------------------------------------------- |
| `GET /v1/sun`             | Sun: position, dignity, next event, rise/set    |
| `GET /v1/moon`            | Moon: phase, illumination, lunation №, VoC      |
| `GET /v1/body/{name}`     | Any body: Sun..Pluto, nodes, Lilith, Chiron, asteroids |
| `GET /v1/star/{name}`     | Fixed star: Sirius, Vega, Spica, Regulus, Algol, … |
| `GET /v1/olivier`         | All 18 bodies in radians + house cusps          |

## Computed events

| Path                      | Returns                                         |
| ------------------------- | ----------------------------------------------- |
| `GET /v1/houses`          | 12 house cusps, Asc/MC/Vertex/etc.              |
| `GET /v1/aspects`         | Instantaneous aspect grid                       |
| `GET /v1/transits`        | Active transits to a natal chart                |
| `GET /v1/return`          | Next solar/lunar/planetary return               |
| `GET /v1/eclipses`        | Solar/lunar eclipses in a date range            |
| `GET /v1/midpoints`       | Pairwise midpoints plus harmonic hits           |
| `GET /v1/antiscia`        | Antiscia / contra-antiscia positions and hits   |
| `GET /v1/decans`          | Decan assignments by several traditional systems |
| `GET /v1/terms`           | Ptolemaic or Egyptian bound ruler per body      |
| `GET /v1/triplicity`      | Dorothean triplicity rulers per body            |
| `GET /v1/receptions`      | Mutual receptions by traditional rulership      |
| `GET /v1/equation-of-time` | Apparent solar time minus mean solar time      |
| `GET /v1/ingresses`       | Upcoming cardinal-sign ingresses                |
| `GET /v1/lunations`       | New/quarter/full/last-quarter Moons in a window |
| `GET /v1/heliacal/{star}` | Next heliacal rising for a star and observer    |
| `GET /v1/zodiacal-releasing` | Zodiacal Releasing L1 periods from Spirit    |
| `GET /v1/natal-chart`     | Houses, bodies-with-houses, aspects, and lots   |
| `GET /v1/events`          | DB-backed event log                             |
| `GET /v1/events.ics`      | Same, as an iCalendar feed                      |

## Streams (Server-Sent Events)

| Path                              | Pushes                                    |
| --------------------------------- | ----------------------------------------- |
| `GET /v1/stream/sun`              | Sun position every `?interval=N` seconds  |
| `GET /v1/stream/moon`             | Moon position                             |
| `GET /v1/stream/body/{name}`      | Any body                                  |
| `GET /v1/stream/events`           | DB events as a backlog stream             |

All SSE endpoints emit `id:` lines so clients reconnect with
`Last-Event-ID` and don't re-process events.

## Operational

| Path                | Returns                                       |
| ------------------- | --------------------------------------------- |
| `GET /health`       | Liveness probe (uptime + ephemeris check)     |
| `GET /metrics`      | Prometheus exposition                         |
| `GET /openapi.json` | OpenAPI 3.0 spec                              |
| `GET /docs`         | Swagger / rapidoc UI                          |
| `GET /robots.txt`   | Disallow crawlers from `/v1/*`                |
| `GET /favicon.ico`  | Inline SVG crescent-moon                      |

## Pages

| Path           | Description                                  |
| -------------- | -------------------------------------------- |
| `GET /`        | Web app (alias for `/app`)                   |
| `GET /app`     | Tabbed JS console covering every feature     |
| `GET /chart`   | Standalone chart-wheel page                  |

## Common query parameters

| Param                  | Where                                    |
| ---------------------- | ---------------------------------------- |
| `date=…`               | ISO 8601 timestamp or Julian Day decimal |
| `tz=Europe/Berlin`     | IANA zone for ISO inputs                 |
| `latitude=…&longitude=…` | Observer position                       |
| `zodiac=tropical|sidereal` | (default tropical)                   |
| `ayanamsha=…`          | When zodiac=sidereal: lahiri / krishnamurti / fagan_bradley / raman / yukteshwar / … |
| `house_system=…`       | Letter code (`P/K/W/…`) or name          |
| `orb=…`                | Aspect orb in degrees                    |
| `include=…`            | `/v1/aspects` and `/v1/transits`: comma-separated subset of `nodes,asteroids,chiron,lilith` |
| `include_angles=1`     | Same: include Asc/MC                     |
| `system=ptolemaic|egyptian` | `/v1/terms` bound table selector     |
| `count=…`              | `/v1/ingresses` and `/v1/zodiacal-releasing` result count |
| `lookahead=…`          | Days forward for eclipse/lunation/event windows |
| `date_start=…&date_end=…` | Window bounds where supported         |
| `natal_date=…&natal_latitude=…&natal_longitude=…` | Natal inputs for chart-derived techniques |

## Rate limits & caching

* Default rate limit: 60 requests / 10 s per client (configurable).
* Default response cache TTL: 10 s. Each response carries `X-Cache:
  HIT|MISS` and the cached body is byte-identical.
* Streams bypass both layers.
* Health and metrics endpoints bypass the rate limit so monitoring
  doesn't get rejected.

## Authentication

When the server is started with `--api-key SECRET` (or
`CERRIDWEN_API_KEY=SECRET`), every `/v1/*` request needs an
`X-API-Key: SECRET` header. `/health`, `/metrics`, `/openapi.json`,
`/docs`, `/app`, `/chart`, `/`, and `/favicon.ico` are always public.

## CORS

Default: `Access-Control-Allow-Origin: *`. Tighten with
`--cors-origins https://example.com,https://other.com`.
