// SPDX-License-Identifier: MIT AND AGPL-3.0-only

//! Higher-level astrological techniques layered on top of the raw
//! ephemeris primitives in `planets.rs`.
//!
//! Each function is small, deterministic, and side-effect free (apart
//! from `init_swe`, which the underlying `swe::` calls handle).
//!
//! Conventions:
//!   * All longitudes are *tropical* degrees in `[0, 360)` unless the
//!     caller folds an ayanamsha at the boundary. Sidereal callers do
//!     that shift in the endpoint layer.
//!   * `jd` parameters are Julian Day in UT.

use crate::defs::{init_swe, SIGNS};
use crate::planets::{compute_houses, Body, Eclipse, EclipseSearch, Moon, Sun};
use crate::utils::{jd2iso, mod360_distance};
use crate::LatLong;
use libswisseph_sys as raw;
use swisseph::swe;

// SwissEph flag bits used here.
const SEFLG_SWIEPH: i32 = 2;
const SEFLG_SPEED: i32 = 256;
const SEFLG_HELCTR: i32 = 8;
const SEFLG_TOPOCTR: i32 = 32 * 1024;
const SEFLG_EQUATORIAL: i32 = 2 * 1024;
const SE_CALC_RISE: i32 = 1;
const SE_CALC_SET: i32 = 2;

// ------------------------------------------------------------------------------------------------
// Reference frame — geo / helio / topo. Affects all body endpoints.
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Center {
    Geo,
    Helio,
    Topo,
}

impl Center {
    pub fn parse(s: &str) -> Option<Center> {
        match s.to_ascii_lowercase().as_str() {
            "geo" | "geocentric" | "earth" => Some(Center::Geo),
            "helio" | "heliocentric" | "sun" => Some(Center::Helio),
            "topo" | "topocentric" => Some(Center::Topo),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Center::Geo => "geocentric",
            Center::Helio => "heliocentric",
            Center::Topo => "topocentric",
        }
    }
    pub fn flags(self) -> i32 {
        match self {
            Center::Geo => SEFLG_SWIEPH,
            Center::Helio => SEFLG_SWIEPH | SEFLG_HELCTR,
            Center::Topo => SEFLG_SWIEPH | SEFLG_TOPOCTR,
        }
    }
}

/// Heliocentric / topocentric / geocentric ecliptic longitude.
///
/// For `Center::Topo` you must pre-arm SwissEph with the observer via
/// [`set_topo`]. For `Helio`, the Sun's longitude is undefined (returns
/// `f64::NAN`) — call sites should suppress it.
pub fn longitude_at(center: Center, body_id: i32, jd: f64) -> f64 {
    init_swe();
    if center == Center::Helio && body_id == crate::planets::SE_SUN {
        return f64::NAN;
    }
    let r = swe::calc_ut(jd, body_id as u32, center.flags() as u32).expect("calc_ut failed");
    r.out[0]
}

/// Set the topocentric observer for subsequent `Center::Topo` calls.
/// Idempotent and cheap; safe to call per request.
pub fn set_topo(observer: &LatLong) {
    init_swe();
    unsafe { raw::swe_set_topo(observer.long, observer.lat, 0.0) };
}

// ------------------------------------------------------------------------------------------------
// Declinations + parallels.
// ------------------------------------------------------------------------------------------------

/// Declination (δ) in degrees. Positive = north of the celestial equator.
pub fn declination(body_id: i32, jd: f64) -> f64 {
    init_swe();
    let r = swe::calc_ut(jd, body_id as u32, (SEFLG_SWIEPH | SEFLG_EQUATORIAL) as u32)
        .expect("calc_ut failed");
    r.out[1]
}

pub fn right_ascension(body_id: i32, jd: f64) -> f64 {
    init_swe();
    let r = swe::calc_ut(jd, body_id as u32, (SEFLG_SWIEPH | SEFLG_EQUATORIAL) as u32)
        .expect("calc_ut failed");
    r.out[0]
}

#[derive(Clone, Debug)]
pub struct ParallelAspect {
    pub a: String,
    pub b: String,
    pub kind: ParallelKind,
    pub orb: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParallelKind {
    Parallel,
    Contraparallel,
}

impl ParallelKind {
    pub fn label(self) -> &'static str {
        match self {
            ParallelKind::Parallel => "parallel",
            ParallelKind::Contraparallel => "contraparallel",
        }
    }
}

/// Build a declination grid: for every pair, classify as parallel
/// (same sign, |Δδ| ≤ orb) or contraparallel (opposite sign,
/// |δ_a + δ_b| ≤ orb).
pub fn declination_aspects(bodies: &[(String, i32)], jd: f64, orb: f64) -> Vec<ParallelAspect> {
    let decs: Vec<f64> = bodies.iter().map(|(_, id)| declination(*id, jd)).collect();
    let mut out = Vec::new();
    for i in 0..bodies.len() {
        for j in (i + 1)..bodies.len() {
            let (a, b) = (decs[i], decs[j]);
            if a.signum() == b.signum() && (a - b).abs() <= orb {
                out.push(ParallelAspect {
                    a: bodies[i].0.clone(),
                    b: bodies[j].0.clone(),
                    kind: ParallelKind::Parallel,
                    orb: (a - b).abs(),
                });
            } else if a.signum() != b.signum() && (a + b).abs() <= orb {
                out.push(ParallelAspect {
                    a: bodies[i].0.clone(),
                    b: bodies[j].0.clone(),
                    kind: ParallelKind::Contraparallel,
                    orb: (a + b).abs(),
                });
            }
        }
    }
    out
}

/// True iff the Moon is out-of-bounds: |δ| > 23.4367° (max solar
/// declination at J2000 + small margin).
pub fn moon_out_of_bounds(jd: f64) -> bool {
    declination(crate::planets::SE_MOON, jd).abs() > 23.4367
}

// ------------------------------------------------------------------------------------------------
// Tithi (lunar day 1-30) and Nakshatra (27 lunar mansions).
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Tithi {
    pub number: u8,             // 1..=30
    pub name: &'static str,     // e.g. "Shukla Pratipada"
    pub half: &'static str,     // "shukla" (waxing) or "krishna" (waning)
    pub paksha_index: u8,       // 1..=15 within the half
    pub elongation_deg: f64,    // Moon - Sun longitude, normalised
    pub fraction_complete: f64, // 0..1 within the current tithi
}

const TITHI_NAMES: [&str; 15] = [
    "Pratipada",
    "Dvitiya",
    "Tritiya",
    "Chaturthi",
    "Panchami",
    "Shashthi",
    "Saptami",
    "Ashtami",
    "Navami",
    "Dashami",
    "Ekadashi",
    "Dvadashi",
    "Trayodashi",
    "Chaturdashi",
    "Purnima/Amavasya",
];

pub fn tithi(jd: f64) -> Tithi {
    let s = Sun::new();
    let m = Moon::at_jd(jd);
    let elong = (m.longitude(jd) - s.0.longitude_at(jd)).rem_euclid(360.0);
    let t_real = elong / 12.0; // 0..30
    let number = (t_real.floor() as u8).min(29) + 1;
    let half = if number <= 15 { "shukla" } else { "krishna" };
    let paksha_index = if number <= 15 { number } else { number - 15 };
    let name = TITHI_NAMES[(paksha_index - 1) as usize];
    Tithi {
        number,
        name,
        half,
        paksha_index,
        elongation_deg: elong,
        fraction_complete: t_real - t_real.floor(),
    }
}

#[derive(Clone, Debug)]
pub struct Nakshatra {
    pub number: u8, // 1..=27
    pub name: &'static str,
    pub pada: u8,            // 1..=4 (quarter within a nakshatra)
    pub lon_in_mansion: f64, // 0..13.333°
}

const NAKSHATRA_NAMES: [&str; 27] = [
    "Ashwini",
    "Bharani",
    "Krittika",
    "Rohini",
    "Mrigashira",
    "Ardra",
    "Punarvasu",
    "Pushya",
    "Ashlesha",
    "Magha",
    "Purva Phalguni",
    "Uttara Phalguni",
    "Hasta",
    "Chitra",
    "Swati",
    "Vishakha",
    "Anuradha",
    "Jyeshtha",
    "Mula",
    "Purva Ashadha",
    "Uttara Ashadha",
    "Shravana",
    "Dhanishta",
    "Shatabhisha",
    "Purva Bhadrapada",
    "Uttara Bhadrapada",
    "Revati",
];

/// Nakshatra of a sidereal longitude (Lahiri ayanamsha convention).
/// Pass the *sidereal* Moon longitude — caller folds the ayanamsha.
pub fn nakshatra_sidereal(sid_long_deg: f64) -> Nakshatra {
    let lon = sid_long_deg.rem_euclid(360.0);
    let span = 360.0 / 27.0;
    let idx = (lon / span).floor() as usize;
    let n = idx.min(26);
    let lon_in = lon - (n as f64) * span;
    let pada = ((lon_in / (span / 4.0)).floor() as u8 + 1).min(4);
    Nakshatra {
        number: (n + 1) as u8,
        name: NAKSHATRA_NAMES[n],
        pada,
        lon_in_mansion: lon_in,
    }
}

// ------------------------------------------------------------------------------------------------
// Twilight times — sunrise/sunset variants. Returns rising and setting
// julian days when the Sun's altitude crosses the given angle below the
// horizon (6/12/18° for civil/nautical/astronomical).
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct TwilightTimes {
    pub sunrise: f64,
    pub sunset: f64,
    pub civil_dawn: f64,
    pub civil_dusk: f64,
    pub nautical_dawn: f64,
    pub nautical_dusk: f64,
    pub astronomical_dawn: f64,
    pub astronomical_dusk: f64,
}

const SE_BIT_DISC_CENTER: i32 = 256;
const SE_BIT_NO_REFRACTION: i32 = 512;

fn rise_or_set_at_alt(jd: f64, observer: &LatLong, rsmi: i32, sun_alt_deg: f64) -> Option<f64> {
    let mut tret = [0.0_f64; 10];
    let mut serr = [0_i8; 256];
    let mut geopos = [observer.long, observer.lat, 0.0];
    unsafe {
        let code = raw::swe_rise_trans(
            jd,
            crate::planets::SE_SUN,
            std::ptr::null_mut(),
            SEFLG_SWIEPH,
            rsmi | SE_BIT_DISC_CENTER | SE_BIT_NO_REFRACTION,
            geopos.as_mut_ptr(),
            1013.25,
            10.0,
            tret.as_mut_ptr(),
            serr.as_mut_ptr(),
        );
        if code < 0 {
            return None;
        }
    }
    // Refine by bisection on Sun altitude until alt = sun_alt_deg.
    // tret[0] is the geometric rise/set at 0° alt; for twilight we want
    // a different altitude. Do a coarse search outward from tret[0]
    // until altitude crosses the threshold.
    let direction = if rsmi & SE_CALC_RISE != 0 { -1.0 } else { 1.0 };
    let mut t = tret[0];
    let step = 1.0 / 1440.0; // 1-minute
    for _ in 0..720 {
        let alt = sun_altitude(t, observer);
        if alt <= sun_alt_deg {
            // We've passed the twilight; refine.
            let mut lo = t;
            let mut hi = t - direction * step;
            for _ in 0..40 {
                let mid = (lo + hi) / 2.0;
                let a = sun_altitude(mid, observer);
                if a > sun_alt_deg {
                    hi = mid;
                } else {
                    lo = mid;
                }
                if (hi - lo).abs() < 1e-7 {
                    break;
                }
            }
            return Some((lo + hi) / 2.0);
        }
        t += direction * step;
    }
    None
}

fn sun_altitude(jd: f64, observer: &LatLong) -> f64 {
    init_swe();
    unsafe { raw::swe_set_topo(observer.long, observer.lat, 0.0) };
    let r = swe::calc_ut(
        jd,
        crate::planets::SE_SUN as u32,
        (SEFLG_SWIEPH | SEFLG_TOPOCTR | SEFLG_EQUATORIAL) as u32,
    )
    .expect("calc_ut failed");
    let ra = r.out[0].to_radians();
    let dec = r.out[1].to_radians();
    let sidt_hours = unsafe { raw::swe_sidtime(jd) };
    let lst = (sidt_hours * 15.0 + observer.long).to_radians();
    let ha = lst - ra;
    let lat = observer.lat.to_radians();
    (lat.sin() * dec.sin() + lat.cos() * dec.cos() * ha.cos())
        .clamp(-1.0, 1.0)
        .asin()
        .to_degrees()
}

pub fn twilight_times(jd: f64, observer: &LatLong) -> TwilightTimes {
    let sunrise = rise_or_set_at_alt(jd, observer, SE_CALC_RISE, 0.0).unwrap_or(f64::NAN);
    let sunset = rise_or_set_at_alt(jd, observer, SE_CALC_SET, 0.0).unwrap_or(f64::NAN);
    TwilightTimes {
        sunrise,
        sunset,
        civil_dawn: rise_or_set_at_alt(jd, observer, SE_CALC_RISE, -6.0).unwrap_or(f64::NAN),
        civil_dusk: rise_or_set_at_alt(jd, observer, SE_CALC_SET, -6.0).unwrap_or(f64::NAN),
        nautical_dawn: rise_or_set_at_alt(jd, observer, SE_CALC_RISE, -12.0).unwrap_or(f64::NAN),
        nautical_dusk: rise_or_set_at_alt(jd, observer, SE_CALC_SET, -12.0).unwrap_or(f64::NAN),
        astronomical_dawn: rise_or_set_at_alt(jd, observer, SE_CALC_RISE, -18.0)
            .unwrap_or(f64::NAN),
        astronomical_dusk: rise_or_set_at_alt(jd, observer, SE_CALC_SET, -18.0).unwrap_or(f64::NAN),
    }
}

// ------------------------------------------------------------------------------------------------
// Planetary hours — Chaldean order. Day hour 1 is ruled by the day's
// planetary regent (Sun on Sunday, Moon on Monday, Mars on Tuesday, …).
// Day is split into 12 equal parts between sunrise and sunset; night
// likewise between sunset and the following sunrise.
// ------------------------------------------------------------------------------------------------

const CHALDEAN_ORDER: [&str; 7] = [
    "Saturn", "Jupiter", "Mars", "Sun", "Venus", "Mercury", "Moon",
];

// 0=Sun, 1=Mon, ..., 6=Sat → ruling planet (start of day hours).
fn day_ruler(weekday: u32) -> &'static str {
    match weekday {
        0 => "Sun",
        1 => "Moon",
        2 => "Mars",
        3 => "Mercury",
        4 => "Jupiter",
        5 => "Venus",
        6 => "Saturn",
        _ => unreachable!(),
    }
}

fn weekday_from_jd(jd: f64) -> u32 {
    // JD 0 = Monday noon. (floor(jd+1.5)) mod 7 = weekday 0..6 with 0=Sunday.
    ((jd + 1.5).floor() as i64).rem_euclid(7) as u32
}

#[derive(Clone, Debug)]
pub struct PlanetaryHour {
    pub index: u8,          // 1..=24
    pub kind: &'static str, // "day" or "night"
    pub ruler: &'static str,
    pub start_jd: f64,
    pub end_jd: f64,
}

pub fn planetary_hours(jd: f64, observer: &LatLong) -> Vec<PlanetaryHour> {
    let mut tret = [0.0_f64; 10];
    let mut serr = [0_i8; 256];
    let mut geopos = [observer.long, observer.lat, 0.0];
    let sunrise = unsafe {
        raw::swe_rise_trans(
            jd,
            crate::planets::SE_SUN,
            std::ptr::null_mut(),
            SEFLG_SWIEPH,
            SE_CALC_RISE,
            geopos.as_mut_ptr(),
            0.0,
            0.0,
            tret.as_mut_ptr(),
            serr.as_mut_ptr(),
        );
        tret[0]
    };
    let mut tret2 = [0.0_f64; 10];
    let sunset = unsafe {
        raw::swe_rise_trans(
            sunrise,
            crate::planets::SE_SUN,
            std::ptr::null_mut(),
            SEFLG_SWIEPH,
            SE_CALC_SET,
            geopos.as_mut_ptr(),
            0.0,
            0.0,
            tret2.as_mut_ptr(),
            serr.as_mut_ptr(),
        );
        tret2[0]
    };
    let mut tret3 = [0.0_f64; 10];
    let next_sunrise = unsafe {
        raw::swe_rise_trans(
            sunset,
            crate::planets::SE_SUN,
            std::ptr::null_mut(),
            SEFLG_SWIEPH,
            SE_CALC_RISE,
            geopos.as_mut_ptr(),
            0.0,
            0.0,
            tret3.as_mut_ptr(),
            serr.as_mut_ptr(),
        );
        tret3[0]
    };

    let day_len = (sunset - sunrise) / 12.0;
    let night_len = (next_sunrise - sunset) / 12.0;

    let ruler = day_ruler(weekday_from_jd(sunrise));
    let start = CHALDEAN_ORDER
        .iter()
        .position(|&p| p == ruler)
        .expect("ruler in chaldean order");

    let mut out = Vec::with_capacity(24);
    for i in 0..12 {
        let r = CHALDEAN_ORDER[(start + i) % 7];
        out.push(PlanetaryHour {
            index: (i + 1) as u8,
            kind: "day",
            ruler: r,
            start_jd: sunrise + (i as f64) * day_len,
            end_jd: sunrise + ((i + 1) as f64) * day_len,
        });
    }
    for i in 0..12 {
        let r = CHALDEAN_ORDER[(start + 12 + i) % 7];
        out.push(PlanetaryHour {
            index: (12 + i + 1) as u8,
            kind: "night",
            ruler: r,
            start_jd: sunset + (i as f64) * night_len,
            end_jd: sunset + ((i + 1) as f64) * night_len,
        });
    }
    out
}

// ------------------------------------------------------------------------------------------------
// Arabic parts / Lots. Built from Asc + two body longitudes.
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ArabicPart {
    pub name: &'static str,
    pub longitude: f64,
    pub formula: &'static str,
}

/// Compute the standard Hellenistic lots from natal Asc, Sun, Moon, and
/// the visible planets. `is_day` toggles diurnal vs. nocturnal formulas
/// for those lots that swap.
#[allow(clippy::too_many_arguments)]
pub fn arabic_parts(
    asc: f64,
    sun: f64,
    moon: f64,
    mercury: f64,
    venus: f64,
    mars: f64,
    jupiter: f64,
    saturn: f64,
    is_day: bool,
) -> Vec<ArabicPart> {
    let norm = |x: f64| x.rem_euclid(360.0);
    let fortune = if is_day {
        norm(asc + moon - sun)
    } else {
        norm(asc + sun - moon)
    };
    let spirit = if is_day {
        norm(asc + sun - moon)
    } else {
        norm(asc + moon - sun)
    };
    let mk = |name: &'static str, lon: f64, formula: &'static str| ArabicPart {
        name,
        longitude: norm(lon),
        formula,
    };
    vec![
        if is_day {
            mk("Fortune", asc + moon - sun, "Asc + Moon - Sun (day)")
        } else {
            mk("Fortune", asc + sun - moon, "Asc + Sun - Moon (night)")
        },
        if is_day {
            mk("Spirit", asc + sun - moon, "Asc + Sun - Moon (day)")
        } else {
            mk("Spirit", asc + moon - sun, "Asc + Moon - Sun (night)")
        },
        if is_day {
            mk("Eros", asc + venus - spirit, "Asc + Venus - Spirit (day)")
        } else {
            mk("Eros", asc + spirit - venus, "Asc + Spirit - Venus (night)")
        },
        if is_day {
            mk(
                "Necessity",
                asc + fortune - mercury,
                "Asc + Fortune - Mercury (day)",
            )
        } else {
            mk(
                "Necessity",
                asc + mercury - fortune,
                "Asc + Mercury - Fortune (night)",
            )
        },
        if is_day {
            mk(
                "Courage",
                asc + fortune - mars,
                "Asc + Fortune - Mars (day)",
            )
        } else {
            mk(
                "Courage",
                asc + mars - fortune,
                "Asc + Mars - Fortune (night)",
            )
        },
        if is_day {
            mk(
                "Victory",
                asc + jupiter - spirit,
                "Asc + Jupiter - Spirit (day)",
            )
        } else {
            mk(
                "Victory",
                asc + spirit - jupiter,
                "Asc + Spirit - Jupiter (night)",
            )
        },
        if is_day {
            mk(
                "Nemesis",
                asc + fortune - saturn,
                "Asc + Fortune - Saturn (day)",
            )
        } else {
            mk(
                "Nemesis",
                asc + saturn - fortune,
                "Asc + Saturn - Fortune (night)",
            )
        },
    ]
}

// ------------------------------------------------------------------------------------------------
// Annual profections — Whole-sign rotation through the houses, one
// sign per year. Year 0 = natal Asc sign; year 1 = next sign; etc.
// Lord of the year = traditional ruler of the profected sign.
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Profection {
    pub age: u32,
    pub house: u8,          // 1..=12
    pub sign: &'static str, // sign of the profected house
    pub lord: &'static str, // traditional planetary ruler of that sign
}

fn traditional_ruler(sign: &str) -> &'static str {
    match sign {
        "Aries" => "Mars",
        "Taurus" => "Venus",
        "Gemini" => "Mercury",
        "Cancer" => "Moon",
        "Leo" => "Sun",
        "Virgo" => "Mercury",
        "Libra" => "Venus",
        "Scorpio" => "Mars",
        "Sagittarius" => "Jupiter",
        "Capricorn" => "Saturn",
        "Aquarius" => "Saturn",
        "Pisces" => "Jupiter",
        _ => "?",
    }
}

pub fn profection(natal_asc_long: f64, age: u32) -> Profection {
    let asc_sign_idx = ((natal_asc_long.rem_euclid(360.0)) / 30.0).floor() as u32;
    let house = (age % 12) as u8 + 1; // 1..=12
    let sign_idx = ((asc_sign_idx + age) % 12) as usize;
    let sign = SIGNS[sign_idx];
    Profection {
        age,
        house,
        sign,
        lord: traditional_ruler(sign),
    }
}

// ------------------------------------------------------------------------------------------------
// Pre-natal eclipses — last solar and last lunar eclipse before `jd`.
// Uses the same swe_sol_eclipse / swe_lun_eclipse iterators as
// next_eclipse, just backwards.
// ------------------------------------------------------------------------------------------------

pub fn pre_natal_solar_eclipse(jd: f64) -> Option<Eclipse> {
    crate::planets::next_eclipse(jd, EclipseSearch::Solar, true)
}

pub fn pre_natal_lunar_eclipse(jd: f64) -> Option<Eclipse> {
    crate::planets::next_eclipse(jd, EclipseSearch::Lunar, true)
}

// ------------------------------------------------------------------------------------------------
// Synastry — inter-aspect grid between two charts.
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct SynastryAspect {
    pub a: String,
    pub b: String,
    pub aspect: &'static str,
    pub orb: f64,
    pub angle_a_to_b: f64,
}

pub fn synastry(
    chart_a: &[(String, f64)],
    chart_b: &[(String, f64)],
    orb: f64,
) -> Vec<SynastryAspect> {
    let mut out = Vec::new();
    for (na, la) in chart_a {
        for (nb, lb) in chart_b {
            let angle = (la - lb).rem_euclid(360.0);
            for a in crate::defs::ASPECTS.iter() {
                let d = mod360_distance(angle, a.angle);
                if d.abs() <= orb {
                    out.push(SynastryAspect {
                        a: na.clone(),
                        b: nb.clone(),
                        aspect: a.name,
                        orb: d.abs(),
                        angle_a_to_b: angle,
                    });
                }
            }
        }
    }
    out
}

// ------------------------------------------------------------------------------------------------
// Composite charts — midpoint composite (per body, shortest-arc midpoint
// of the two longitudes) and Davison (chart at midpoint date & midpoint
// location).
// ------------------------------------------------------------------------------------------------

/// Midpoint along the shortest arc on the zodiac circle.
pub fn shortest_midpoint(a: f64, b: f64) -> f64 {
    let a = a.rem_euclid(360.0);
    let b = b.rem_euclid(360.0);
    let diff = (b - a).rem_euclid(360.0);
    if diff <= 180.0 {
        (a + diff / 2.0).rem_euclid(360.0)
    } else {
        (a + (diff - 360.0) / 2.0).rem_euclid(360.0)
    }
}

pub fn midpoint_composite(
    chart_a: &[(String, f64)],
    chart_b: &[(String, f64)],
) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    for (na, la) in chart_a {
        if let Some((_, lb)) = chart_b.iter().find(|(nb, _)| nb == na) {
            out.push((na.clone(), shortest_midpoint(*la, *lb)));
        }
    }
    out
}

#[derive(Clone, Debug)]
pub struct DavisonChart {
    pub jd: f64,
    pub iso_date: String,
    pub latitude: f64,
    pub longitude: f64,
}

pub fn davison_chart(jd_a: f64, jd_b: f64, loc_a: &LatLong, loc_b: &LatLong) -> DavisonChart {
    let jd = (jd_a + jd_b) / 2.0;
    DavisonChart {
        jd,
        iso_date: jd2iso(jd),
        latitude: (loc_a.lat + loc_b.lat) / 2.0,
        longitude: (loc_a.long + loc_b.long) / 2.0,
    }
}

// ------------------------------------------------------------------------------------------------
// Secondary progressions — one day = one year. Progressed jd = natal_jd
// + (target_jd - natal_jd) / 365.2422.
// Solar arc — every body moves by the same arc the progressed Sun did.
// ------------------------------------------------------------------------------------------------

pub fn progressed_jd(natal_jd: f64, target_jd: f64) -> f64 {
    let years = (target_jd - natal_jd) / 365.2422;
    natal_jd + years
}

pub fn solar_arc_offset(natal_jd: f64, target_jd: f64) -> f64 {
    init_swe();
    let s_natal = swe::calc_ut(natal_jd, crate::planets::SE_SUN as u32, SEFLG_SWIEPH as u32)
        .expect("calc_ut failed")
        .out[0];
    let s_prog = swe::calc_ut(
        progressed_jd(natal_jd, target_jd),
        crate::planets::SE_SUN as u32,
        SEFLG_SWIEPH as u32,
    )
    .expect("calc_ut failed")
    .out[0];
    (s_prog - s_natal).rem_euclid(360.0)
}

// ------------------------------------------------------------------------------------------------
// Retrograde stations — find the next N times `body` changes the sign
// of its longitude speed.
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub enum StationKind {
    Retrograde,
    Direct,
}

impl StationKind {
    pub fn label(self) -> &'static str {
        match self {
            StationKind::Retrograde => "retrograde",
            StationKind::Direct => "direct",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Station {
    pub jd: f64,
    pub iso_date: String,
    pub kind: StationKind,
    pub longitude: f64,
}

pub fn upcoming_stations(
    body_id: i32,
    start_jd: f64,
    lookahead_days: f64,
    max: usize,
) -> Vec<Station> {
    init_swe();
    let mut out = Vec::new();
    let mut t = start_jd;
    let step = 1.0; // 1-day scan
    let end = start_jd + lookahead_days;
    let speed = |t: f64| -> f64 {
        swe::calc_ut(t, body_id as u32, (SEFLG_SWIEPH | SEFLG_SPEED) as u32)
            .expect("calc_ut failed")
            .out[3]
    };
    while t < end && out.len() < max {
        let s_now = speed(t);
        let s_next = speed(t + step);
        if s_now.signum() != s_next.signum() && s_now.abs() > 1e-9 {
            // Bisect for zero crossing.
            let mut lo = t;
            let mut hi = t + step;
            for _ in 0..40 {
                let mid = (lo + hi) / 2.0;
                let sm = speed(mid);
                if sm.signum() == s_now.signum() {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let jd_x = (lo + hi) / 2.0;
            let kind = if s_now > 0.0 {
                StationKind::Retrograde
            } else {
                StationKind::Direct
            };
            let lon = swe::calc_ut(jd_x, body_id as u32, SEFLG_SWIEPH as u32)
                .expect("calc_ut failed")
                .out[0];
            out.push(Station {
                jd: jd_x,
                iso_date: jd2iso(jd_x),
                kind,
                longitude: lon,
            });
            t = jd_x + 1.0;
        } else {
            t += step;
        }
    }
    out
}

// ------------------------------------------------------------------------------------------------
// House placement for a body — return the (1..=12) house index using
// whole-sign or by-cusp depending on the system letter. Only used
// indirectly by other endpoints today; exposed for callers.
// ------------------------------------------------------------------------------------------------

pub fn house_of_longitude(lon: f64, jd: f64, observer: &LatLong, system: char) -> u8 {
    let h = compute_houses(jd, observer.lat, observer.long, system);
    let lon = lon.rem_euclid(360.0);
    for i in 0..12 {
        let cusp_a = h.cusps[i];
        let cusp_b = h.cusps[(i + 1) % 12];
        let span = (cusp_b - cusp_a).rem_euclid(360.0);
        let off = (lon - cusp_a).rem_euclid(360.0);
        if off < span {
            return (i + 1) as u8;
        }
    }
    1
}
