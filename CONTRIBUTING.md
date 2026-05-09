# Contributing to cerridwen

Thanks for taking a look. The crate lives under `rust/`; see
[`rust/README.md`](rust/README.md) for the full build/usage reference.

## Development loop

```bash
cd rust
CERRIDWEN_EPHE_PATH=$(pwd)/../sweph cargo build --features server,mcp,events
CERRIDWEN_EPHE_PATH=$(pwd)/../sweph cargo test  --features server,mcp,events
```

CI runs the same build, plus `cargo fmt --check` and
`cargo clippy --all-targets -- -D warnings`. Both must be clean before
merging.

## Pre-commit hook

A drop-in hook lives in `deploy/pre-commit` — it runs `cargo fmt --check`
and `cargo clippy --no-deps -D warnings` on every commit that touches
Rust code. Install it once and forget about it:

```bash
ln -sf $(pwd)/deploy/pre-commit .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
```

## Style

* Run `cargo fmt` before pushing — there's no negotiation; CI will
  reject mis-formatted code.
* Comments explain *why* something is non-obvious, not *what* the code
  does. The line above the code is for the next reader, not for the
  author.
* Default to writing no comments unless removing them would confuse a
  reader.

## What's in scope

Welcome:

* New body types (asteroids, fixed stars, etc.) — drop the data files
  into `sweph/` and wire the new IDs into `src/planets.rs`.
* New endpoints — `src/bin/server.rs` plus an OpenAPI entry plus a
  matching MCP tool in `src/bin/mcp.rs`.
* House systems / ayanamshas — they're table-driven; just extend the
  parsers and the SwissEph mode constants pass through.
* Astrology features that already have a Swiss Ephemeris primitive
  behind them.

Out of scope without a discussion first:

* Replacing the Swiss Ephemeris dependency. AGPL is the cost of doing
  business.
* Async-everywhere refactors. The server is async; the library is
  intentionally sync because the underlying C library is.
* Frontend frameworks. The web app is plain HTML/CSS/JS and that's
  staying — no React/Vue/etc.

## Tests

Three suites live under `rust/tests/`:

* `numerical.rs` — port of the original Python `tests.py`, validates
  numerical correctness against USNO data and astropy.
* `features.rs` — covers the post-port endpoints (eclipses, transits,
  returns, fixed stars, ayanamshas, time zones, plus regression tests).
* `mcp.rs` — protocol-level smoke tests for `cerridwen-mcp`.
* `server.rs` — end-to-end tests of the HTTP surface (cache, rate
  limit, /health, /metrics, aspect opt-ins).

When you add functionality, add tests.

## Safety / soundness

`libswisseph-sys` exposes the C API directly. Anything that calls into
it (typically `swe::*` calls or raw bindings) needs to remember:

* `swed` is `__thread`-local. `init_swe` is idempotent per thread; new
  worker threads need it called once on entry.
* C functions don't bound-check buffers. Allocate the documented size
  for output arrays and don't shrink them.

## License

Contributions are accepted under the same MIT + AGPL-3.0 terms as the
crate itself. By submitting a PR you confirm you have the right to
license your changes that way.
