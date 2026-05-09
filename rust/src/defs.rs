use std::cell::Cell;
use std::ffi::CString;
use std::path::PathBuf;

use libswisseph_sys as raw;
use once_cell::sync::Lazy;

/// Numerical tolerance: we guarantee event times to within this fraction of a day.
pub const MAXIMUM_ERROR: f64 = 2e-6;

/// Hard cap on samples in a single approximation pass; aborts the pass if exceeded.
pub const MAX_DATA_POINTS: usize = 100_000;

pub const DEBUG_EVENT_APPROXIMATION: bool = false;

pub const SIGNS: [&str; 12] = [
    "Aries",
    "Taurus",
    "Gemini",
    "Cancer",
    "Leo",
    "Virgo",
    "Libra",
    "Scorpio",
    "Sagittarius",
    "Capricorn",
    "Aquarius",
    "Pisces",
];

#[derive(Clone, Debug)]
pub struct Aspect {
    pub angle: f64,
    pub name: &'static str,
    pub mode: Option<&'static str>,
}

const DEXTER_ASPECTS: &[(f64, &str)] = &[
    (30.0, "semi-sextile"),
    (45.0, "semi-square"),
    (60.0, "sextile"),
    (72.0, "quintile"),
    (120.0, "trine"),
    (144.0, "bi-quintile"),
    (150.0, "quincunx"),
];

pub static ASPECTS: Lazy<Vec<Aspect>> = Lazy::new(|| {
    let mut v = Vec::with_capacity(2 + DEXTER_ASPECTS.len() * 2);
    v.push(Aspect {
        angle: 0.0,
        name: "conjunction",
        mode: None,
    });
    for (a, n) in DEXTER_ASPECTS {
        v.push(Aspect {
            angle: *a,
            name: n,
            mode: Some("dexter"),
        });
    }
    v.push(Aspect {
        angle: 180.0,
        name: "opposition",
        mode: None,
    });
    for (a, n) in DEXTER_ASPECTS.iter().rev() {
        v.push(Aspect {
            angle: 360.0 - *a,
            name: n,
            mode: Some("sinister"),
        });
    }
    v
});

pub static TRADITIONAL_MAJOR_ASPECTS: &[&str] =
    &["conjunction", "sextile", "square", "trine", "opposition"];

pub static SIGN_RELATED_ASPECTS: &[&str] = &[
    "conjunction",
    "semi-sextile",
    "sextile",
    "square",
    "trine",
    "quincunx",
    "opposition",
];

/// Return the absolute path to the Swiss Ephemeris data directory.
///
/// Resolution order:
///   1. `CERRIDWEN_EPHE_PATH` env var, if set.
///   2. `<cwd>/sweph` if it exists.
///   3. `<repo root>/sweph` (one level up from the rust crate dir) if it exists.
///   4. `./sweph` as a last resort (Swiss Ephemeris will fall back to Moshier).
pub fn ephe_path() -> PathBuf {
    if let Ok(p) = std::env::var("CERRIDWEN_EPHE_PATH") {
        return PathBuf::from(p);
    }
    let candidates = [
        std::env::current_dir().ok().map(|d| d.join("sweph")),
        std::env::current_dir()
            .ok()
            .and_then(|d| d.parent().map(|p| p.join("sweph"))),
    ];
    for c in candidates.iter().flatten() {
        if c.exists() {
            return c.clone();
        }
    }
    PathBuf::from("./sweph")
}

/// Initialise Swiss Ephemeris on the calling thread.
///
/// SwissEph stores its global state (`swed`, including `swed.ephepath`) in
/// thread-local storage — every thread gets its own zero-initialised copy,
/// so we have to set the path once *per thread*. The `THREAD_INIT` cell
/// makes the per-thread path-set cheap on subsequent calls.
pub fn init_swe() {
    thread_local! {
        static THREAD_INIT: Cell<bool> = const { Cell::new(false) };
    }
    THREAD_INIT.with(|cell| {
        if !cell.get() {
            force_set_ephe_path();
            cell.set(true);
        }
    });
}

/// Force-set the ephemeris path on the calling thread, bypassing the
/// per-thread gate. Calls the raw binding directly so we control the
/// C-string lifetime — the high-level `swisseph::swe::set_ephe_path`
/// drops its CString immediately after the call, which is fragile if
/// SwissEph ever rereads the pointer later.
pub fn force_set_ephe_path() {
    let p = ephe_path();
    let c_str = CString::new(p.to_string_lossy().as_ref()).expect("ephe path has no NUL bytes");
    unsafe {
        raw::swe_set_ephe_path(c_str.as_ptr() as *mut _);
    }
    // CString freed here; SwissEph copies the path into its internal buffer
    // via strncpy, so the pointer doesn't need to outlive this call.
}
