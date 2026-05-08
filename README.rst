Cerridwen
=========

Cerridwen provides geocentric planetary data suitable for a broad spectrum
of astronomical and astrological applications, with a focus on the solar
system and the bodies/points commonly used in astrology.

Among its features:

* Sun, Moon, the eight major planets, lunar nodes, Black Moon Lilith,
  Chiron, the four major asteroids (Ceres, Pallas, Juno, Vesta), and a
  curated set of fixed stars (Sirius, Vega, Spica, Regulus, Algol …).

* Tropical or sidereal zodiac with selectable ayanamshas (Lahiri,
  Krishnamurti, Fagan-Bradley, Raman, Yukteshwar, Djwhal Khul, J2000,
  galactic-center, …).

* 18 supported house systems (Placidus, Koch, Whole-sign, Porphyry,
  Regiomontanus, Campanus, Equal, Vehlow, Morinus, Topocentric,
  Alcabitius, APC, Meridian, Horizon …).

* Eclipse predictions, instantaneous aspect grids, transit-to-natal
  aspect lookup with applying/separating tags, solar/lunar/planetary
  returns, void-of-course Moon, lunation number, and a sqlite-backed
  events database with an iCal feed.

* Time-zone-aware date input via IANA names (Europe/Berlin etc.).

This repository hosts the **Rust port**. The original Python package was
retired; the Rust crate under ``rust/`` is API-compatible at the data
shape level and ships substantially more.

Quick start
-----------

::

    cd rust
    CERRIDWEN_EPHE_PATH=$(pwd)/../sweph cargo run --features server --bin cerridwen-server

then open http://127.0.0.1:2828/ in a browser.

Components
----------

* **HTTP/JSON server** — every feature exposed as REST endpoints under
  ``/v1/…`` plus ``/openapi.json`` + Swagger UI at ``/docs``.

* **Tabbed web console at /app** — comprehensive in-browser interface
  covering every endpoint, with deep-linkable tabs, a chart wheel, and
  live SSE position streams.

* **CLI ``cerridwen``** — prints sun/moon/lunation/VoC/next-event for
  the current moment.

* **MCP server ``cerridwen-mcp``** — Model Context Protocol over stdio,
  letting LLM agents (Claude Code, IDE clients) call cerridwen as a
  tool.

* **Event generator ``cerridwen-event-generator``** — populates a sqlite
  events table that the ``/v1/events`` and ``/v1/events.ics`` endpoints
  query.

See ``rust/README.md`` for full build/usage instructions and the API
reference.

License
-------

Cerridwen itself is MIT. Swiss Ephemeris is dual-licensed; the bundled
``libswisseph-sys`` ships the AGPL-3.0 variant, so the combined binary
is AGPL-3.0 unless you license Swiss Ephemeris separately.
