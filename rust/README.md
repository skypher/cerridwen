# cerridwen — Rust port

A Rust port of the Python `cerridwen` package: geocentric Sun/Moon/planet
data backed by Swiss Ephemeris.

## Layout

- `src/lib.rs` — public API: `LatLong`, `compute_sun_data`, `compute_moon_data`, re-exports of the planet types.
- `src/defs.rs` — constants, ephemeris-path resolution, `init_swe`.
- `src/utils.rs` — JD↔ISO conversion, mod-360 helpers, formatting.
- `src/approximate.rs` — recursive event-finder used by angle/retrograde lookups.
- `src/planets.rs` — `Planet` core, `Sun`/`Moon`/`Mercury`/… wrappers, `Ascendant`, `FixedZodiacPoint`, `Body` trait.
- `src/bin/cli.rs` — `cerridwen` CLI (no features needed).
- `src/bin/server.rs` — `cerridwen-server` HTTP JSON API (`--features server`).
- `src/bin/event_generator.rs` — `cerridwen-event-generator` populates the sqlite events table (`--features events`).
- `tests/numerical.rs` — port of `cerridwen/tests.py`.

## Build

```bash
cd rust
cargo build                                   # lib + cli
cargo build --features server                 # + JSON HTTP server
cargo build --features events                 # + sqlite event generator
cargo test                                    # run the test suite
```

The Swiss Ephemeris data files (`sepl_*.se1` etc.) need to be reachable at
runtime. By default the crate looks for `./sweph` relative to the working
directory, then `../sweph`. Override with the `CERRIDWEN_EPHE_PATH`
environment variable.

For the sibling Python install layout (running from this repo's root):

```bash
CERRIDWEN_EPHE_PATH=$(pwd)/../sweph cargo run --bin cerridwen
```

## Notes vs the Python original

- `Planet::next_sign_change` uses an id-based switch over the known bodies for
  the sample-window lookahead; the Python equivalent dispatched via
  inheritance. Same effective values.
- The bundled Swiss Ephemeris stores its globals (including `swed.ephepath`)
  in `__thread`-local storage. `init_swe` therefore sets the ephe path
  *per thread*, gated by a `thread_local!` flag — without this, worker
  threads silently fall back to the Moshier formulae when `swe_calc_ut`
  loads ephemeris data on first use.
- All ported tests now match the Python reference values to the second,
  except the 2020 Jupiter-Saturn conjunction which drifts by ~3 s due to
  precession-model differences between `pyswisseph` and `libswisseph-sys`.
  ISO comparisons assert "within 10 s".
- Optional features pull in their dependencies only when enabled: `server`
  brings in axum/tokio/serde_json, `events` brings in rusqlite.

## License

Cerridwen itself is MIT. Swiss Ephemeris is dual-licensed; the bundled
`libswisseph-sys` ships the AGPL-3.0 variant, so the combined binary is
AGPL-3.0 unless you license Swiss Ephemeris separately.
