//! # cerridwen
//!
//! Geocentric planetary data — Sun, Moon, and the major planets — backed by
//! Swiss Ephemeris. This crate is a Rust port of the Python `cerridwen`
//! package; the public API mirrors the original closely.
//!
//! ## Quick start
//!
//! ```no_run
//! use cerridwen::{compute_moon_data, LatLong};
//!
//! let data = compute_moon_data(None, Some(LatLong::new(52.5, 13.3).unwrap()));
//! println!("Moon position: {}", data.position);
//! ```

pub mod approximate;
pub mod defs;
pub mod planets;
pub mod utils;

pub use crate::defs::{Aspect, ASPECTS, SIGNS, SIGN_RELATED_ASPECTS, TRADITIONAL_MAJOR_ASPECTS};
pub use crate::planets::{
    Ascendant, Body, FixedZodiacPoint, Jupiter, Mars, Mercury, Moon, MoonPhaseData, Neptune,
    Planet, PlanetEvent, PlanetLongitude, Pluto, Saturn, Sun, Uranus, Venus,
};
pub use crate::utils::{
    angle_to_aspect_name, aspect_name_to_angle, days_frac_to_dhms, iso2jd, jd2iso, jd_now,
    mod360_distance, parse_jd_or_iso_date, render_delta_days,
};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// ------------------------------------------------------------------------------------------------
// LatLong — observer position. Validated at construction so downstream code can
// assume the values are within range.
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct LatLong {
    pub lat: f64,
    pub long: f64,
}

impl LatLong {
    pub fn new(lat: f64, long: f64) -> Result<Self, &'static str> {
        if !(-90.0..=90.0).contains(&lat) {
            return Err("Latitude must be between -90 and 90");
        }
        if !(-180.0..=180.0).contains(&long) {
            return Err("Longitude must be between -180 and 180");
        }
        Ok(Self { lat, long })
    }
}

// ------------------------------------------------------------------------------------------------
// Aggregate "compute everything we know about X" entry points.
// ------------------------------------------------------------------------------------------------

/// Sun summary at the requested instant.
#[derive(Debug, Clone)]
pub struct SunData {
    pub jd: f64,
    pub iso_date: String,
    pub position: PlanetLongitude,
    pub dignity: Option<&'static str>,
    pub next_rise: Option<PlanetEvent>,
    pub next_set: Option<PlanetEvent>,
    pub last_rise: Option<PlanetEvent>,
    pub last_set: Option<PlanetEvent>,
}

/// Moon summary at the requested instant.
#[derive(Debug, Clone)]
pub struct MoonData {
    pub jd: f64,
    pub iso_date: String,
    pub position: PlanetLongitude,
    pub phase: MoonPhaseData,
    pub illumination: f64,
    pub distance: f64,
    pub diameter: f64,
    pub diameter_ratio: f64,
    pub speed: f64,
    pub speed_ratio: f64,
    pub age: f64,
    pub period_length: f64,
    pub dignity: Option<&'static str>,
    pub next_new_moon: PlanetEvent,
    pub next_full_moon: PlanetEvent,
    pub next_new_or_full_moon: PlanetEvent,
    pub last_new_moon: PlanetEvent,
    pub last_full_moon: PlanetEvent,
    pub next_rise: Option<PlanetEvent>,
    pub next_set: Option<PlanetEvent>,
    pub last_rise: Option<PlanetEvent>,
    pub last_set: Option<PlanetEvent>,
}

pub fn compute_sun_data(jd: Option<f64>, observer: Option<LatLong>) -> SunData {
    let jd = jd.unwrap_or_else(jd_now);
    let sun = Sun::at(Some(jd), observer);
    let position = sun.position(None);
    let dignity = sun.dignity(None);
    let (next_rise, next_set, last_rise, last_set) = if observer.is_some() {
        (Some(sun.next_rise()), Some(sun.next_set()),
         Some(sun.last_rise()), Some(sun.last_set()))
    } else {
        (None, None, None, None)
    };
    SunData {
        jd,
        iso_date: jd2iso(jd),
        position,
        dignity,
        next_rise,
        next_set,
        last_rise,
        last_set,
    }
}

pub fn compute_moon_data(jd: Option<f64>, observer: Option<LatLong>) -> MoonData {
    let jd = jd.unwrap_or_else(jd_now);
    let moon = Moon::at(Some(jd), observer);
    let position = moon.position(None);
    let phase = moon.phase(None);
    let illumination = moon.illumination(None);
    let distance = moon.distance(None);
    let diameter = moon.diameter(None);
    let diameter_ratio = moon.diameter_ratio(None);
    let speed = moon.speed(None);
    let speed_ratio = moon.speed_ratio(None);
    let age = moon.age(None);
    let period_length = moon.period_length(None);
    let dignity = moon.dignity(None);
    let next_new_moon = moon.next_new_moon(None);
    let next_full_moon = moon.next_full_moon(None);
    let next_new_or_full_moon = moon.next_new_or_full_moon(None);
    let last_new_moon = moon.last_new_moon(None);
    let last_full_moon = moon.last_full_moon(None);
    let (next_rise, next_set, last_rise, last_set) = if observer.is_some() {
        (Some(moon.next_rise()), Some(moon.next_set()),
         Some(moon.last_rise()), Some(moon.last_set()))
    } else {
        (None, None, None, None)
    };

    MoonData {
        jd,
        iso_date: jd2iso(jd),
        position,
        phase,
        illumination,
        distance,
        diameter,
        diameter_ratio,
        speed,
        speed_ratio,
        age,
        period_length,
        dignity,
        next_new_moon,
        next_full_moon,
        next_new_or_full_moon,
        last_new_moon,
        last_full_moon,
        next_rise,
        next_set,
        last_rise,
        last_set,
    }
}

// ------------------------------------------------------------------------------------------------
// Events (sqlite-backed) — only available with the `events` feature enabled.
// ------------------------------------------------------------------------------------------------

#[cfg(feature = "events")]
pub mod events {
    use rusqlite::{params, Connection};
    use std::path::Path;

    use crate::utils::jd2iso;

    #[derive(Debug, Clone)]
    pub struct EventFilter {
        pub types: Option<Vec<String>>,
        pub subtypes: Option<Vec<String>>,
        pub planets: Option<Vec<String>>,
        pub datas: Option<Vec<String>>,
    }

    impl EventFilter {
        pub fn new() -> Self {
            Self { types: None, subtypes: None, planets: None, datas: None }
        }
        fn keep(&self, t: &str, st: &str, p: &str, d: &str) -> bool {
            let pass = |allow: &Option<Vec<String>>, val: &str| {
                allow.as_ref().map_or(true, |xs| xs.iter().any(|x| x == val))
            };
            pass(&self.types, t) && pass(&self.subtypes, st)
                && pass(&self.planets, p) && pass(&self.datas, d)
        }
    }

    impl Default for EventFilter {
        fn default() -> Self { Self::new() }
    }

    #[derive(Debug, Clone)]
    pub struct EventRow {
        pub jd: f64,
        pub r#type: String,
        pub subtype: String,
        pub planet: String,
        pub data: String,
        pub iso_date: String,
        pub delta_days: f64,
    }

    pub fn get_events<P: AsRef<Path>>(
        dbfile: P,
        jd_start: f64,
        jd_end: f64,
        limit: i64,
        filter: &EventFilter,
    ) -> rusqlite::Result<Vec<EventRow>> {
        let conn = Connection::open(dbfile)?;
        // We filter in Rust rather than SQL to match the Python semantics:
        // the original applied `filter_event(...)` in WHERE *before* LIMIT.
        // Pulling rows in JD order and stopping after `limit` matches that
        // without needing a SQL user-defined function.
        let mut stmt = conn.prepare(
            "SELECT jd, type, subtype, planet, data \
             FROM events \
             WHERE jd BETWEEN ?1 AND ?2 \
             ORDER BY jd ASC",
        )?;
        let rows = stmt.query_map(params![jd_start, jd_end], |row| {
            Ok((
                row.get::<_, f64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (jd, t, st, p, d) = r?;
            if !filter.keep(&t, &st, &p, &d) {
                continue;
            }
            out.push(EventRow {
                jd,
                iso_date: jd2iso(jd),
                delta_days: jd - jd_start,
                r#type: t,
                subtype: st,
                planet: p,
                data: d,
            });
            if out.len() as i64 >= limit {
                break;
            }
        }
        Ok(out)
    }
}
