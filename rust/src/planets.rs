use std::ffi::CStr;

use libswisseph_sys as raw;
use swisseph::swe;

use crate::approximate::approximate_event_date;
use crate::defs::{init_swe, MAXIMUM_ERROR, SIGNS};
use crate::utils::{jd2iso, jd_now, mod360_distance};
use crate::LatLong;

// Constants pulled from swephexp.h.
const SEFLG_SWIEPH: i32 = 2;
const SEFLG_SPEED: i32 = 256;
const SEFLG_EQUATORIAL: i32 = 2 * 1024;
const SE_CALC_RISE: i32 = 1;
const SE_CALC_SET: i32 = 2;

// Eclipse classification bits (from swephexp.h).
const SE_ECL_CENTRAL: i32 = 1;
const SE_ECL_NONCENTRAL: i32 = 2;
const SE_ECL_TOTAL: i32 = 4;
const SE_ECL_ANNULAR: i32 = 8;
const SE_ECL_PARTIAL: i32 = 16;
const SE_ECL_ANNULAR_TOTAL: i32 = 32;
const SE_ECL_PENUMBRAL: i32 = 64;
// Body IDs (mirroring swephexp.h).
pub const SE_SUN: i32 = 0;
pub const SE_MOON: i32 = 1;
pub const SE_MERCURY: i32 = 2;
pub const SE_VENUS: i32 = 3;
pub const SE_MARS: i32 = 4;
pub const SE_JUPITER: i32 = 5;
pub const SE_SATURN: i32 = 6;
pub const SE_URANUS: i32 = 7;
pub const SE_NEPTUNE: i32 = 8;
pub const SE_PLUTO: i32 = 9;
pub const SE_MEAN_NODE: i32 = 10;
pub const SE_TRUE_NODE: i32 = 11;
pub const SE_MEAN_APOG: i32 = 12;   // Black Moon Lilith (mean)
pub const SE_OSCU_APOG: i32 = 13;   // Black Moon Lilith (osculating)
pub const SE_CHIRON: i32 = 15;
pub const SE_CERES: i32 = 17;
pub const SE_PALLAS: i32 = 18;
pub const SE_JUNO: i32 = 19;
pub const SE_VESTA: i32 = 20;

// ------------------------------------------------------------------------------------------------
// PlanetEvent / PlanetLongitude / Ascendant / FixedZodiacPoint
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct PlanetEvent {
    pub description: String,
    pub jd: f64,
}

impl PlanetEvent {
    pub fn new(description: impl Into<String>, jd: f64) -> Self {
        Self { description: description.into(), jd }
    }

    pub fn iso_date(&self) -> String {
        jd2iso(self.jd)
    }

    pub fn delta_days(&self, rel_jd: Option<f64>) -> f64 {
        self.jd - rel_jd.unwrap_or_else(jd_now)
    }
}

impl std::fmt::Display for PlanetEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}", self.description, self.iso_date())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PlanetLongitude {
    pub absolute_degrees: f64,
}

impl PlanetLongitude {
    pub fn new(absolute_degrees: f64) -> Self {
        Self { absolute_degrees }
    }

    pub fn sign(&self) -> &'static str {
        let normalized = self.absolute_degrees.rem_euclid(360.0);
        SIGNS[(normalized / 30.0) as usize]
    }

    pub fn deg(&self) -> f64 {
        self.absolute_degrees.rem_euclid(360.0) % 30.0
    }

    pub fn min(&self) -> f64 {
        (self.deg() % 1.0) * 60.0
    }

    pub fn sec(&self) -> f64 {
        ((self.deg() % 1.0) * 60.0 - self.min().floor()) * 60.0
    }

    /// (sign, deg, min, sec) — the latter three truncated for printing.
    pub fn rel_tuple(&self) -> (&'static str, i64, i64, i64) {
        (
            self.sign(),
            self.deg().floor() as i64,
            self.min().floor() as i64,
            self.sec().floor() as i64,
        )
    }
}

impl std::fmt::Display for PlanetLongitude {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (sign, deg, min, sec) = self.rel_tuple();
        write!(f, "{} {} {}' {}\"", deg, &sign[..3], min, sec)
    }
}

pub struct Ascendant {
    pub jd: f64,
    pub long: f64,
    pub lat: f64,
    pub house_system: u8,
}

impl Ascendant {
    pub fn new(long: f64, lat: f64, jd: Option<f64>) -> Self {
        Self::with_house_system(long, lat, jd, b'P')
    }

    pub fn with_house_system(long: f64, lat: f64, jd: Option<f64>, hsys: u8) -> Self {
        init_swe();
        Self { jd: jd.unwrap_or_else(jd_now), long, lat, house_system: hsys }
    }

    pub fn name(&self) -> &'static str {
        "Ascendant"
    }

    pub fn longitude(&self, jd: Option<f64>) -> f64 {
        let jd = jd.unwrap_or(self.jd);
        let (_cusps, ascmc) = swe::houses(jd, self.lat, self.long, self.house_system as i32);
        ascmc[0]
    }

    pub fn position(&self, jd: Option<f64>) -> PlanetLongitude {
        PlanetLongitude::new(self.longitude(jd))
    }

    pub fn sign(&self, jd: Option<f64>) -> &'static str {
        self.position(jd).sign()
    }
}

/// Full house-system result. Cusps are 1-indexed in the API, so element
/// `cusps[i]` holds house i+1's cusp longitude (i in 0..12).
#[derive(Clone, Debug)]
pub struct Houses {
    pub system_code: char,
    pub system_name: String,
    pub cusps: [f64; 12],
    pub ascendant: f64,
    pub mc: f64,
    pub armc: f64,
    pub vertex: f64,
    pub equatorial_ascendant: f64,
    pub co_ascendant_koch: f64,
    pub co_ascendant_munkasey: f64,
    pub polar_ascendant: f64,
}

/// Compute houses for a given moment and observer using a SwissEph
/// house-system letter code. See `valid_house_systems()` for the list.
pub fn compute_houses(jd: f64, lat: f64, long: f64, system: char) -> Houses {
    init_swe();
    let code = system as i32;
    let (cusps_raw, ascmc) = swe::houses(jd, lat, long, code);
    let mut cusps = [0.0_f64; 12];
    for i in 0..12 {
        cusps[i] = cusps_raw[i + 1];
    }
    Houses {
        system_code: system,
        system_name: swe::house_name(code),
        cusps,
        ascendant: ascmc[0],
        mc: ascmc[1],
        armc: ascmc[2],
        vertex: ascmc[3],
        equatorial_ascendant: ascmc[4],
        co_ascendant_koch: ascmc[5],
        co_ascendant_munkasey: ascmc[6],
        polar_ascendant: ascmc[7],
    }
}

/// Letter codes for the house systems we accept. Mirrors SwissEph; case
/// is normalised to upper.
pub fn valid_house_systems() -> &'static [(char, &'static str)] {
    &[
        ('P', "Placidus"),
        ('K', "Koch"),
        ('O', "Porphyry"),
        ('R', "Regiomontanus"),
        ('C', "Campanus"),
        ('A', "Equal (Asc)"),
        ('E', "Equal (alt)"),
        ('V', "Vehlow equal"),
        ('W', "Whole sign"),
        ('X', "Meridian / axial rotation"),
        ('M', "Morinus"),
        ('H', "Horizon / azimuth"),
        ('T', "Polich/Page (topocentric)"),
        ('B', "Alcabitius"),
        ('U', "Krusinski-Pisa-Goelzer"),
        ('Y', "APC"),
        ('N', "Equal MC"),
        ('D', "Equal (MC)"),
    ]
}

/// Return the canonical house-system letter for an input string, or `None`
/// if not recognised. Accepts a single ASCII letter (any case) or a
/// recognised name like "placidus", "whole_sign", "koch".
pub fn parse_house_system(s: &str) -> Option<char> {
    let trimmed = s.trim();
    if trimmed.len() == 1 {
        let c = trimmed.chars().next().unwrap().to_ascii_uppercase();
        if valid_house_systems().iter().any(|(k, _)| *k == c) {
            return Some(c);
        }
    }
    match trimmed.to_ascii_lowercase().replace([' ', '-'], "_").as_str() {
        "placidus" => Some('P'),
        "koch" => Some('K'),
        "porphyry" => Some('O'),
        "regiomontanus" => Some('R'),
        "campanus" => Some('C'),
        "equal" | "equal_asc" => Some('A'),
        "vehlow" | "vehlow_equal" => Some('V'),
        "whole_sign" | "whole" | "whole_signs" => Some('W'),
        "meridian" | "axial" | "axial_rotation" => Some('X'),
        "morinus" => Some('M'),
        "horizon" | "azimuth" | "azimut" => Some('H'),
        "topocentric" | "polich" | "polich_page" => Some('T'),
        "alcabitius" => Some('B'),
        "krusinski" | "krusinski_pisa_goelzer" => Some('U'),
        "apc" => Some('Y'),
        "equal_mc" => Some('N'),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct FixedZodiacPoint {
    pub degrees: f64,
}

impl FixedZodiacPoint {
    pub fn new(degrees: f64) -> Self {
        Self { degrees }
    }

    pub fn longitude(&self, _jd: Option<f64>) -> f64 {
        self.degrees
    }

    pub fn position(&self, jd: Option<f64>) -> PlanetLongitude {
        PlanetLongitude::new(self.longitude(jd))
    }

    pub fn sign(&self, jd: Option<f64>) -> &'static str {
        self.position(jd).sign()
    }
}

// ------------------------------------------------------------------------------------------------
// Moon phase data
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct MoonPhaseData {
    pub trend: &'static str,
    pub shape: &'static str,
    pub quarter: Option<i32>,
    pub quarter_english: Option<&'static str>,
}

// ------------------------------------------------------------------------------------------------
// Planet trait + concrete bodies
// ------------------------------------------------------------------------------------------------

/// Anything that has a longitude on the ecliptic at a moment in time.
///
/// Planets, the Sun, the Moon, and a `FixedZodiacPoint` are all implementors.
pub trait Body {
    fn longitude(&self, jd: f64) -> f64;
    fn name(&self) -> String;
    fn max_speed(&self) -> f64;
    /// Used by `next_angle_to_planet` as the default forward search horizon.
    fn aspect_lookahead(&self) -> f64 {
        365.0 * 100.0
    }
    /// Whether an aspect of the given angle to `other` is geometrically possible.
    fn aspect_possible(&self, _other: &dyn Body, _angle: f64) -> bool {
        true
    }
}

#[derive(Clone, Debug)]
pub struct Planet {
    pub id: i32,
    pub jd: f64,
    pub observer: Option<LatLong>,
}

impl Planet {
    pub fn new(id: i32, jd: Option<f64>, observer: Option<LatLong>) -> Self {
        init_swe();
        Self { id, jd: jd.unwrap_or_else(jd_now), observer }
    }

    pub fn name(&self) -> String {
        swe::get_planet_name(self.id)
    }

    /// Apparent diameter in arc minutes (ecliptical, geocentric).
    pub fn diameter(&self, jd: Option<f64>) -> f64 {
        let jd = jd.unwrap_or(self.jd);
        let mut attr = [0.0_f64; 20];
        let mut serr = [0_i8; 256];
        unsafe {
            raw::swe_pheno_ut(jd, self.id, SEFLG_SWIEPH, attr.as_mut_ptr(), serr.as_mut_ptr());
        }
        attr[3] * 60.0
    }

    pub fn longitude_at(&self, jd: f64) -> f64 {
        let r = swe::calc_ut(jd, self.id as u32, SEFLG_SWIEPH as u32)
            .expect("calc_ut failed");
        r.out[0]
    }

    pub fn latitude(&self, jd: Option<f64>) -> f64 {
        let jd = jd.unwrap_or(self.jd);
        let r = swe::calc_ut(jd, self.id as u32, SEFLG_SWIEPH as u32)
            .expect("calc_ut failed");
        r.out[1]
    }

    pub fn rectascension(&self, jd: Option<f64>) -> f64 {
        let jd = jd.unwrap_or(self.jd);
        let flags = (SEFLG_SWIEPH | SEFLG_EQUATORIAL) as u32;
        let r = swe::calc_ut(jd, self.id as u32, flags).expect("calc_ut failed");
        r.out[0]
    }

    pub fn declination(&self, jd: Option<f64>) -> f64 {
        let jd = jd.unwrap_or(self.jd);
        let flags = (SEFLG_SWIEPH | SEFLG_EQUATORIAL) as u32;
        let r = swe::calc_ut(jd, self.id as u32, flags).expect("calc_ut failed");
        r.out[1]
    }

    pub fn distance(&self, jd: Option<f64>) -> f64 {
        let jd = jd.unwrap_or(self.jd);
        let r = swe::calc_ut(jd, self.id as u32, SEFLG_SWIEPH as u32)
            .expect("calc_ut failed");
        r.out[2]
    }

    pub fn position(&self, jd: Option<f64>) -> PlanetLongitude {
        PlanetLongitude::new(self.longitude_at(jd.unwrap_or(self.jd)))
    }

    pub fn sign(&self, jd: Option<f64>) -> &'static str {
        self.position(jd).sign()
    }

    pub fn speed(&self, jd: Option<f64>) -> f64 {
        let jd = jd.unwrap_or(self.jd);
        let flags = (SEFLG_SWIEPH | SEFLG_SPEED) as u32;
        let r = swe::calc_ut(jd, self.id as u32, flags).expect("calc_ut failed");
        r.out[3]
    }

    pub fn is_rx(&self, jd: Option<f64>) -> bool {
        self.speed(jd) < 0.0
    }

    pub fn is_stationing(&self, jd: Option<f64>) -> bool {
        self.speed(jd).abs() < 0.2
    }

    /// Angle from `self` to `other`, modulo 360.
    pub fn angle_to(&self, other: &dyn Body, jd: f64) -> f64 {
        (self.longitude_at(jd) - other.longitude(jd)).rem_euclid(360.0)
    }

    /// Phase illumination as a 0..=1 fraction. Mirrors the Python base-class
    /// definition: `(180 - mod360_distance(angle_to_sun, 180)) / 180`.
    pub fn illumination(&self, jd: Option<f64>) -> f64 {
        let jd = jd.unwrap_or(self.jd);
        let sun = Sun::new();
        (180.0 - mod360_distance(self.angle_to(&sun.0, jd), 180.0)) / 180.0
    }

    pub fn next_rise(&self) -> PlanetEvent {
        let observer = self.observer.as_ref()
            .expect("Rise/set times require observer longitude and latitude");
        let jd = rise_trans(self.jd, self.id, observer, SE_CALC_RISE);
        PlanetEvent::new(format!("{} rises", self.name()), jd)
    }

    pub fn next_set(&self) -> PlanetEvent {
        let observer = self.observer.as_ref()
            .expect("Rise/set times require observer longitude and latitude");
        let jd = rise_trans(self.jd, self.id, observer, SE_CALC_SET);
        PlanetEvent::new(format!("{} sets", self.name()), jd)
    }

    pub fn last_rise(&self) -> PlanetEvent {
        let observer = self.observer.as_ref()
            .expect("Rise/set times require observer longitude and latitude");
        let jd = rise_trans(self.jd - 1.0, self.id, observer, SE_CALC_RISE);
        PlanetEvent::new(format!("{} rises", self.name()), jd)
    }

    pub fn last_set(&self) -> PlanetEvent {
        let observer = self.observer.as_ref()
            .expect("Rise/set times require observer longitude and latitude");
        let jd = rise_trans(self.jd - 1.0, self.id, observer, SE_CALC_SET);
        PlanetEvent::new(format!("{} sets", self.name()), jd)
    }

    /// Find the next time this body forms an exact `target_angle` to `other`.
    ///
    /// `lookahead` may be negative for a backwards search (matching the Python).
    pub fn next_angle_to_planet<B: Body + ?Sized>(
        &self,
        other: &B,
        target_angle: f64,
        jd: Option<f64>,
        lookahead: Option<f64>,
        sample_interval: Option<f64>,
        passes: Option<u32>,
        orb: Option<f64>,
    ) -> Option<(f64, f64, f64)> {
        let jd = jd.unwrap_or(self.jd);
        assert!(target_angle < 360.0);

        let lookahead = lookahead.unwrap_or_else(|| {
            self.aspect_lookahead().min(other.aspect_lookahead())
        });
        let (jd_start, jd_end) = if lookahead >= 0.0 {
            (jd, jd + lookahead)
        } else {
            (jd + lookahead, jd)
        };

        let mut next_angles = self.angles_to_planet_within_period(
            other,
            target_angle,
            jd_start,
            jd_end,
            sample_interval,
            passes,
            orb,
        );

        if next_angles.is_empty() {
            return None;
        }

        if lookahead < 0.0 {
            next_angles.reverse();
        }

        let (next_jd, value) = next_angles[0];
        let delta_jd = next_jd - jd;
        let angle_diff = mod360_distance(target_angle, value);
        Some((next_jd, delta_jd, angle_diff))
    }

    pub fn angles_to_planet_within_period<B: Body + ?Sized>(
        &self,
        other: &B,
        target_angle: f64,
        jd_start: f64,
        jd_end: f64,
        sample_interval: Option<f64>,
        passes: Option<u32>,
        orb: Option<f64>,
    ) -> Vec<(f64, f64)> {
        assert!(target_angle < 360.0);
        let passes = passes.unwrap_or(8);
        let sample_interval = sample_interval.unwrap_or_else(|| self.default_sample_interval());
        let orb = orb.unwrap_or(MAXIMUM_ERROR * 10.0);
        assert!(orb > 0.0 && orb < 360.0);

        let id = self.id;
        let mut eval: Box<dyn FnMut(f64) -> f64 + '_> = Box::new(move |d: f64| -> f64 {
            let me = swe::calc_ut(d, id as u32, SEFLG_SWIEPH as u32)
                .expect("calc_ut failed").out[0];
            (me - other.longitude(d)).rem_euclid(360.0)
        });
        let mut find_matches =
            |jds: &[f64], eval: &mut Box<dyn FnMut(f64) -> f64 + '_>|
            -> Option<Vec<(f64, f64)>> {
                find_local_minima(jds, &mut **eval, target_angle)
            };

        let result = approximate_event_date(
            jd_start,
            jd_end,
            &mut eval,
            &mut find_matches,
            &|v| mod360_distance(v, target_angle) <= orb,
            &mod360_distance,
            sample_interval,
            passes,
        );

        let mut events = result.unwrap_or_default();
        events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        events
    }

    pub fn retrogrades_within_period(
        &self,
        jd_start: f64,
        jd_end: f64,
        sample_interval: Option<f64>,
        passes: Option<u32>,
    ) -> Vec<(f64, f64)> {
        let passes = passes.unwrap_or(8);
        let sample_interval = sample_interval.unwrap_or_else(|| self.default_sample_interval());

        let id = self.id;
        let flags = (SEFLG_SWIEPH | SEFLG_SPEED) as u32;
        let mut eval: Box<dyn FnMut(f64) -> f64 + '_> = Box::new(move |d: f64| -> f64 {
            swe::calc_ut(d, id as u32, flags).expect("calc_ut failed").out[3]
        });
        let mut find_matches =
            |jds: &[f64], eval: &mut Box<dyn FnMut(f64) -> f64 + '_>|
            -> Option<Vec<(f64, f64)>> {
                find_zero_crossings(jds, &mut **eval)
            };

        let result = approximate_event_date(
            jd_start,
            jd_end,
            &mut eval,
            &mut find_matches,
            &|_| true,
            &|a, b| (a - b).abs(),
            sample_interval,
            passes,
        );
        let mut events = result.unwrap_or_default();
        events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        events
    }

    /// Returns (jd, "rx" | "direct").
    /// Whether this body has retrograde stations in the meaningful sense.
    /// The Sun and Moon never retrograde; the lunar mean node always
    /// regresses (so no station); the mean lunar apogee similarly has no
    /// rectifiable station.
    pub fn has_rx_stations(&self) -> bool {
        !matches!(
            self.id,
            SE_SUN | SE_MOON | SE_MEAN_NODE | SE_MEAN_APOG
        )
    }

    pub fn next_rx_event(
        &self,
        jd: Option<f64>,
        lookahead: Option<f64>,
    ) -> Option<(f64, &'static str)> {
        assert!(self.has_rx_stations(), "{} does not retrograde", self.name());
        let jd = jd.unwrap_or(self.jd);
        let lookahead = lookahead.unwrap_or_else(|| self.aspect_lookahead());
        let (jd_start, jd_end) = if lookahead >= 0.0 {
            (jd, jd + lookahead)
        } else {
            (jd + lookahead, jd)
        };

        let mut events = self.retrogrades_within_period(jd_start, jd_end, None, None);
        if events.is_empty() {
            return None;
        }
        if lookahead < 0.0 {
            events.reverse();
        }
        let (event_jd, speed) = events[0];
        let kind = if speed > 0.0 { "direct" } else { "rx" };
        Some((event_jd, kind))
    }

    pub fn default_sample_interval(&self) -> f64 {
        1.0 / (self.max_speed() * 3.0)
    }

    pub fn next_sign_change(&self, jd: Option<f64>) -> f64 {
        let jd = jd.unwrap_or(self.jd);
        let cur_sign = self.sign(Some(jd));
        let cur_idx = SIGNS.iter().position(|s| *s == cur_sign).unwrap();

        // Direction-aware targeting: a retrograde-moving body crosses the
        // *previous* sign boundary next. Mean-node-style perpetual
        // retrogrades require this to terminate at all.
        let going_retrograde = self.speed(Some(jd)) < 0.0;
        let next_idx = if going_retrograde {
            (cur_idx + 11) % 12
        } else {
            (cur_idx + 1) % 12
        };
        let target = FixedZodiacPoint::new(next_idx as f64 * 30.0);
        let result = self.next_angle_to_planet(
            &target,
            0.0,
            Some(jd),
            Some(self.sign_change_lookahead()),
            None,
            None,
            None,
        );
        let (event_jd, _, _) = result.expect("next_sign_change found nothing");
        // Nudge slightly past the boundary so callers don't see the previous sign.
        // For retrograde motion the nudge goes the other way.
        if going_retrograde {
            event_jd - MAXIMUM_ERROR
        } else {
            event_jd + MAXIMUM_ERROR
        }
    }

    pub fn time_left_in_sign(&self, jd: Option<f64>) -> f64 {
        let jd = jd.unwrap_or(self.jd);
        self.next_sign_change(Some(jd)) - jd
    }

    /// Default lookahead used for `next_sign_change`. Body-specific values
    /// keep the search grid bounded — the universal fallback would blow past
    /// `MAX_DATA_POINTS` for fast bodies like the Moon.
    pub fn sign_change_lookahead(&self) -> f64 {
        match self.id {
            SE_SUN => 35.0,
            SE_MOON => 2.7,
            SE_MERCURY => 75.0,
            SE_VENUS => 150.0,
            SE_MARS => 300.0,
            SE_JUPITER => 365.0 * 1.5,
            SE_SATURN => 365.0 * 3.5,
            SE_URANUS => 365.0 * 8.0,
            SE_NEPTUNE => 365.0 * 16.0,
            SE_PLUTO => 365.0 * 25.0,
            SE_MEAN_NODE | SE_TRUE_NODE => 365.0 * 2.0,
            SE_MEAN_APOG | SE_OSCU_APOG => 365.0,
            SE_CHIRON => 365.0 * 5.0,
            SE_CERES | SE_PALLAS | SE_JUNO | SE_VESTA => 365.0,
            _ => 365.0 * 30.0,
        }
    }

    /// Return the earliest forthcoming event for this body across the
    /// generic event types: rise, set, sign-ingress, and retrograde
    /// station. For the Moon, the next new/full moon is also considered.
    /// Returns `None` only if every individual lookup fails.
    ///
    /// Aspect events between specific bodies aren't included here — they
    /// require explicit partner choice; use `next_angle_to_planet`.
    pub fn next_event(&self) -> Option<PlanetEvent> {
        let mut candidates: Vec<PlanetEvent> = Vec::new();

        if self.observer.is_some() {
            candidates.push(self.next_rise());
            candidates.push(self.next_set());
        }

        // Sign ingress.
        let nsc = self.next_sign_change(None);
        let next_sign = self.sign(Some(nsc));
        candidates.push(PlanetEvent::new(
            format!("{} ingress into {}", self.name(), next_sign),
            nsc,
        ));

        // Retrograde station (where applicable).
        if self.has_rx_stations() {
            if let Some((rx_jd, kind)) = self.next_rx_event(None, None) {
                let desc = if kind == "rx" {
                    format!("{} stations retrograde", self.name())
                } else {
                    format!("{} stations direct", self.name())
                };
                candidates.push(PlanetEvent::new(desc, rx_jd));
            }
        }

        // Moon-specific: next new/full moon.
        if self.id == SE_MOON {
            let moon = Moon(self.clone());
            candidates.push(moon.next_new_or_full_moon(None));
        }

        candidates
            .into_iter()
            .min_by(|a, b| a.jd.partial_cmp(&b.jd).unwrap())
    }

    /// Mean orbital period (sidereal) in days. The Sun entry is Earth's
    /// heliocentric orbit; the Moon entry is its sidereal lunar month.
    /// Values from IERS / IAU constants.
    pub fn mean_orbital_period(&self) -> f64 {
        match self.id {
            SE_SUN => 365.256363004,        // Earth around Sun
            SE_MOON => 27.32166155,         // Moon around Earth (sidereal)
            SE_MERCURY => 87.9691,
            SE_VENUS => 224.701,
            SE_MARS => 686.971,
            SE_JUPITER => 4332.59,
            SE_SATURN => 10759.22,
            SE_URANUS => 30688.5,
            SE_NEPTUNE => 60182.0,
            SE_PLUTO => 90560.0,
            // Lunar node regression: 18.6 years.
            SE_MEAN_NODE | SE_TRUE_NODE => 6798.38,
            // Lunar apsidal precession: 8.85 years.
            SE_MEAN_APOG | SE_OSCU_APOG => 3232.6,
            SE_CHIRON => 18415.0,           // 50.42 yr
            SE_CERES => 1681.6,             // 4.60 yr
            SE_PALLAS => 1686.0,
            SE_JUNO => 1592.0,
            SE_VESTA => 1325.7,
            _ => f64::NAN,
        }
    }

    /// Mean orbital velocity relative to Earth's heliocentric velocity.
    /// Derived from Kepler's third law: v ∝ T^(-1/3), so the ratio is
    /// `(T_earth / T_body)^(1/3)`. Earth itself returns 1.0.
    pub fn relative_orbital_velocity(&self) -> f64 {
        const EARTH_PERIOD_DAYS: f64 = 365.256363004;
        (EARTH_PERIOD_DAYS / self.mean_orbital_period()).cbrt()
    }
}

// ------------------------------------------------------------------------------------------------
// Display impls — match the Python `__str__` outputs.
// ------------------------------------------------------------------------------------------------

impl std::fmt::Display for Planet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}", self.name(), jd2iso(self.jd))
    }
}

impl std::fmt::Display for Ascendant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}", self.name(), jd2iso(self.jd))
    }
}

impl std::fmt::Display for FixedZodiacPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Fixed zodiac point at {} degrees ({})",
            self.degrees,
            self.position(None)
        )
    }
}

// ------------------------------------------------------------------------------------------------
// Concrete body wrappers — these just specialise Planet for the Body trait and
// add body-specific tunables (max_speed, dignity, …).
// ------------------------------------------------------------------------------------------------

macro_rules! body_wrapper {
    ($t:ident, $id:ident) => {
        #[derive(Clone, Debug)]
        pub struct $t(pub Planet);

        impl $t {
            pub fn new() -> Self { Self::at(None, None) }
            pub fn at_jd(jd: f64) -> Self { Self::at(Some(jd), None) }
            pub fn with_observer(observer: LatLong) -> Self { Self::at(None, Some(observer)) }
            pub fn at(jd: Option<f64>, observer: Option<LatLong>) -> Self {
                Self(Planet::new($id, jd, observer))
            }
        }

        impl std::ops::Deref for $t {
            type Target = Planet;
            fn deref(&self) -> &Planet { &self.0 }
        }
    };
}

body_wrapper!(Sun, SE_SUN);
body_wrapper!(Moon, SE_MOON);
body_wrapper!(Mercury, SE_MERCURY);
body_wrapper!(Venus, SE_VENUS);
body_wrapper!(Mars, SE_MARS);
body_wrapper!(Jupiter, SE_JUPITER);
body_wrapper!(Saturn, SE_SATURN);
body_wrapper!(Uranus, SE_URANUS);
body_wrapper!(Neptune, SE_NEPTUNE);
body_wrapper!(Pluto, SE_PLUTO);
body_wrapper!(MeanNode, SE_MEAN_NODE);
body_wrapper!(TrueNode, SE_TRUE_NODE);
body_wrapper!(Lilith, SE_MEAN_APOG);
body_wrapper!(Chiron, SE_CHIRON);
body_wrapper!(Ceres, SE_CERES);
body_wrapper!(Pallas, SE_PALLAS);
body_wrapper!(Juno, SE_JUNO);
body_wrapper!(Vesta, SE_VESTA);

// ---- Body trait implementations ----------------------------------------------------------------

impl Body for Planet {
    fn longitude(&self, jd: f64) -> f64 { self.longitude_at(jd) }
    fn name(&self) -> String { swe::get_planet_name(self.id) }
    fn max_speed(&self) -> f64 {
        // Approximate maxima used by the event-finder to size the sample
        // grid. Real values vary; these are conservative-enough upper
        // bounds that the search doesn't miss extrema.
        match self.id {
            SE_SUN => 1.0197676,
            SE_MOON => 15.3882655,
            SE_MERCURY => 2.2026512,
            SE_VENUS => 1.2598435,
            SE_MARS => 0.7913920,
            SE_JUPITER => 0.2423810,
            SE_SATURN => 0.1308402,
            SE_URANUS => 0.063,
            SE_NEPTUNE => 0.040,
            SE_PLUTO => 0.040,
            // Mean lunar node moves ~0.053°/day retrograde; true node oscillates faster.
            SE_MEAN_NODE => 0.053,
            SE_TRUE_NODE => 0.6,
            // Black Moon Lilith (mean apogee) ~0.111°/day.
            SE_MEAN_APOG | SE_OSCU_APOG => 0.4,
            // Asteroids — Ceres/Pallas/Juno/Vesta peak around 0.5°/day.
            SE_CHIRON => 0.15,
            SE_CERES | SE_PALLAS | SE_JUNO | SE_VESTA => 0.5,
            _ => 0.1,
        }
    }
    fn aspect_lookahead(&self) -> f64 {
        match self.id {
            SE_SUN => 365.0 * 3.5,
            SE_MOON => 40.0,
            SE_MERCURY => 365.0 * 2.5,
            SE_VENUS => 365.0 * 3.5,
            SE_MARS => 365.0 * 3.5,
            SE_JUPITER => 365.0 * 23.0,
            SE_SATURN => 365.0 * 30.0 + 365.0 * 40.0,
            // Slow-moving outer points: roughly one full orbital period.
            SE_URANUS => 365.0 * 84.0,
            SE_NEPTUNE => 365.0 * 165.0,
            SE_PLUTO => 365.0 * 248.0,
            SE_MEAN_NODE | SE_TRUE_NODE => 365.0 * 19.0,
            SE_MEAN_APOG | SE_OSCU_APOG => 365.0 * 9.0,
            SE_CHIRON => 365.0 * 51.0,
            SE_CERES | SE_PALLAS | SE_JUNO | SE_VESTA => 365.0 * 5.0,
            _ => 365.0 * 100.0,
        }
    }
    fn aspect_possible(&self, other: &dyn Body, angle: f64) -> bool {
        match self.id {
            SE_MERCURY => match other.name().as_str() {
                "Sun" => angle < 27.8 || angle > (360.0 - 27.8),
                "Venus" => angle < 27.8 + 47.8 || angle > (360.0 - (27.8 + 47.8)),
                _ => true,
            },
            SE_VENUS => match other.name().as_str() {
                "Sun" => angle < 47.8 || angle > (360.0 - 47.8),
                "Mercury" => angle < 27.8 + 47.8 || angle > (360.0 - (27.8 + 47.8)),
                _ => true,
            },
            _ => true,
        }
    }
}

impl Body for FixedZodiacPoint {
    fn longitude(&self, _jd: f64) -> f64 { self.degrees }
    fn name(&self) -> String { format!("Fixed zodiac point at {}", self.degrees) }
    fn max_speed(&self) -> f64 { 0.0 }
    fn aspect_lookahead(&self) -> f64 { 1.0e10 }
}

// Forward Body impl for each concrete wrapper via Deref.
macro_rules! impl_body_via_deref {
    ($t:ident) => {
        impl Body for $t {
            fn longitude(&self, jd: f64) -> f64 { self.0.longitude_at(jd) }
            fn name(&self) -> String { self.0.name() }
            fn max_speed(&self) -> f64 { Body::max_speed(&self.0) }
            fn aspect_lookahead(&self) -> f64 { Body::aspect_lookahead(&self.0) }
            fn aspect_possible(&self, other: &dyn Body, angle: f64) -> bool {
                Body::aspect_possible(&self.0, other, angle)
            }
        }
    };
}
impl_body_via_deref!(Sun);
impl_body_via_deref!(Moon);
impl_body_via_deref!(Mercury);
impl_body_via_deref!(Venus);
impl_body_via_deref!(Mars);
impl_body_via_deref!(Jupiter);
impl_body_via_deref!(Saturn);
impl_body_via_deref!(Uranus);
impl_body_via_deref!(Neptune);
impl_body_via_deref!(Pluto);

// ---- Body-specific behaviour --------------------------------------------------------------------

impl Sun {
    pub fn dignity(&self, jd: Option<f64>) -> Option<&'static str> {
        match self.0.sign(jd) {
            "Leo" => Some("rulership"),
            "Aries" => Some("exaltation"),
            "Libra" => Some("detriment"),
            "Aquarius" => Some("fall"),
            _ => None,
        }
    }
    pub fn average_motion_per_year(&self) -> f64 { 360.0 }
}

impl Moon {
    pub fn dignity(&self, jd: Option<f64>) -> Option<&'static str> {
        match self.0.sign(jd) {
            "Cancer" => Some("rulership"),
            "Taurus" => Some("exaltation"),
            "Capricorn" => Some("detriment"),
            "Scorpio" => Some("fall"),
            _ => None,
        }
    }
    pub fn speed_ratio(&self, jd: Option<f64>) -> f64 {
        (self.0.speed(jd) - 11.76) / 3.57
    }
    pub fn diameter_ratio(&self, jd: Option<f64>) -> f64 {
        (self.0.diameter(jd) - 29.3) / 4.8
    }

    pub fn age(&self, jd: Option<f64>) -> f64 {
        let jd = jd.unwrap_or(self.0.jd);
        jd - self.last_new_moon(Some(jd)).jd
    }

    pub fn period_length(&self, jd: Option<f64>) -> f64 {
        let jd = jd.unwrap_or(self.0.jd);
        self.next_new_moon(Some(jd)).jd - self.last_new_moon(Some(jd)).jd
    }

    pub fn phase(&self, jd: Option<f64>) -> MoonPhaseData {
        let jd = jd.unwrap_or(self.0.jd);
        let sun = Sun::new();
        let angle = self.0.angle_to(&sun.0, jd);

        let quarter = if angle > 350.0 || angle < 10.0 {
            Some(0)
        } else if angle > 80.0 && angle < 100.0 {
            Some(1)
        } else if angle > 170.0 && angle < 190.0 {
            Some(2)
        } else if angle > 260.0 && angle < 290.0 {
            Some(3)
        } else {
            None
        };
        let quarter_english = quarter.map(|q| {
            ["new", "first quarter", "full", "third quarter"][q as usize]
        });

        let (trend, shape) = if angle > 0.0 && angle < 90.0 {
            ("waxing", "crescent")
        } else if angle >= 90.0 && angle < 180.0 {
            ("waxing", "gibbous")
        } else if angle >= 190.0 && angle < 270.0 {
            ("waning", "gibbous")
        } else {
            ("waning", "crescent")
        };

        MoonPhaseData { trend, shape, quarter, quarter_english }
    }

    pub fn next_new_moon(&self, jd: Option<f64>) -> PlanetEvent {
        let jd = jd.unwrap_or(self.0.jd);
        let sun = Sun::new();
        let (event_jd, _, _) = self.0.next_angle_to_planet(
            &sun.0, 0.0, Some(jd), None, None, None, None,
        ).expect("next_new_moon");
        let sign = PlanetLongitude::new(self.0.longitude_at(event_jd)).sign();
        PlanetEvent::new(format!("New moon in {}", sign), event_jd)
    }

    pub fn last_new_moon(&self, jd: Option<f64>) -> PlanetEvent {
        let jd = jd.unwrap_or(self.0.jd);
        let sun = Sun::new();
        let (event_jd, _, _) = self.0.next_angle_to_planet(
            &sun.0, 0.0, Some(jd), Some(-40.0), None, None, None,
        ).expect("last_new_moon");
        let sign = PlanetLongitude::new(self.0.longitude_at(event_jd)).sign();
        PlanetEvent::new(format!("New moon in {}", sign), event_jd)
    }

    pub fn next_full_moon(&self, jd: Option<f64>) -> PlanetEvent {
        let jd = jd.unwrap_or(self.0.jd);
        let sun = Sun::new();
        let (event_jd, _, _) = self.0.next_angle_to_planet(
            &sun.0, 180.0, Some(jd), None, None, None, None,
        ).expect("next_full_moon");
        let sign = PlanetLongitude::new(self.0.longitude_at(event_jd)).sign();
        PlanetEvent::new(format!("Full moon in {}", sign), event_jd)
    }

    pub fn last_full_moon(&self, jd: Option<f64>) -> PlanetEvent {
        let jd = jd.unwrap_or(self.0.jd);
        let sun = Sun::new();
        let (event_jd, _, _) = self.0.next_angle_to_planet(
            &sun.0, 180.0, Some(jd), Some(-40.0), None, None, None,
        ).expect("last_full_moon");
        let sign = PlanetLongitude::new(self.0.longitude_at(event_jd)).sign();
        PlanetEvent::new(format!("Full moon in {}", sign), event_jd)
    }

    pub fn next_new_or_full_moon(&self, jd: Option<f64>) -> PlanetEvent {
        let new = self.next_new_moon(jd);
        let full = self.next_full_moon(jd);
        if new.jd < full.jd { new } else { full }
    }

    pub fn last_new_or_full_moon(&self, jd: Option<f64>) -> PlanetEvent {
        let new = self.last_new_moon(jd);
        let full = self.last_full_moon(jd);
        if new.jd > full.jd { new } else { full }
    }

    /// Returns `(is_voc, until_jd)`: whether the Moon is void-of-course at
    /// `jd`, and the JD at which the answer changes.
    ///
    /// VoC is determined by searching for any major aspect (conjunction,
    /// sextile, square, trine, opposition — both dexter and sinister) the
    /// Moon will form to a partner body before its next sign change.
    ///
    /// `traditional_only = false` (the default for the wider modern
    /// definition) considers Sun, Mercury, Venus, Mars, Jupiter, Saturn,
    /// Uranus, Neptune, and Pluto. `traditional_only = true` restricts the
    /// search to the seven traditional planets (Sun through Saturn).
    ///
    /// If at least one aspect is upcoming, the answer is
    /// `(false, last_aspect_jd)` — VoC will *commence* at the latest
    /// aspect time. If no aspect is upcoming, `(true, next_sign_change_jd)`.
    pub fn is_void_of_course(
        &self,
        jd: Option<f64>,
        traditional_only: bool,
    ) -> (bool, f64) {
        let jd = jd.unwrap_or(self.0.jd);
        let nsc = self.0.next_sign_change(Some(jd));

        let partner_ids: &[i32] = if traditional_only {
            &[SE_SUN, SE_MERCURY, SE_VENUS, SE_MARS, SE_JUPITER, SE_SATURN]
        } else {
            &[
                SE_SUN, SE_MERCURY, SE_VENUS, SE_MARS, SE_JUPITER, SE_SATURN,
                SE_URANUS, SE_NEPTUNE, SE_PLUTO,
            ]
        };
        // Major aspects, including sinister mirrors of 60/90/120.
        let major_angles = [0.0, 60.0, 90.0, 120.0, 180.0, 240.0, 270.0, 300.0];

        let mut latest_aspect_jd: Option<f64> = None;

        for &partner_id in partner_ids {
            let partner = Planet::new(partner_id, Some(jd), None);
            for &angle in &major_angles {
                let aspects = self.0.angles_to_planet_within_period(
                    &partner, angle, jd, nsc, None, None, None,
                );
                for (a_jd, _) in aspects {
                    if a_jd > jd && a_jd < nsc {
                        latest_aspect_jd = match latest_aspect_jd {
                            Some(x) if x >= a_jd => Some(x),
                            _ => Some(a_jd),
                        };
                    }
                }
            }
        }

        match latest_aspect_jd {
            Some(t) => (false, t),
            None => (true, nsc),
        }
    }

    /// Brown lunation number. Lunation 1 begins at the first new moon of
    /// January 1923; the bundled Swiss Ephemeris computes that new moon
    /// at JD 2423436.6117 (1923-01-17 02:40 UTC). Snapping to
    /// `last_new_moon` and dividing by the mean synodic month gives the
    /// lunation index; `round()` absorbs the drift between mean and true
    /// synodic month accumulated across the run-up.
    pub fn lunation_number(&self, jd: Option<f64>) -> i64 {
        const BROWN_REF_JD: f64 = 2423436.611689;
        const SYNODIC_MONTH_DAYS: f64 = 29.530588853;
        let jd = jd.unwrap_or(self.0.jd);
        let last_new_moon_jd = self.last_new_moon(Some(jd)).jd;
        let n = (last_new_moon_jd - BROWN_REF_JD) / SYNODIC_MONTH_DAYS;
        n.round() as i64 + 1
    }
}

impl Mercury {
    pub fn dignity(&self, jd: Option<f64>) -> Option<&'static str> {
        match self.0.sign(jd) {
            "Gemini" => Some("rulership"),
            "Virgo" => Some("rulership/exaltation"),
            "Sagittarius" => Some("fall"),
            "Pisces" => Some("fall/detriment"),
            _ => None,
        }
    }
}

impl Venus {
    pub fn dignity(&self, jd: Option<f64>) -> Option<&'static str> {
        match self.0.sign(jd) {
            "Libra" | "Taurus" => Some("rulership"),
            "Pisces" => Some("exaltation"),
            "Virgo" => Some("detriment"),
            "Aries" | "Scorpio" => Some("fall"),
            _ => None,
        }
    }
}

impl Mars {
    pub fn dignity(&self, jd: Option<f64>) -> Option<&'static str> {
        match self.0.sign(jd) {
            "Aries" | "Scorpio" => Some("rulership"),
            "Capricorn" => Some("exaltation"),
            "Cancer" => Some("detriment"),
            "Libra" | "Taurus" => Some("fall"),
            _ => None,
        }
    }
}

impl Jupiter {
    pub fn dignity(&self, jd: Option<f64>) -> Option<&'static str> {
        match self.0.sign(jd) {
            "Sagittarius" | "Pisces" => Some("rulership"),
            "Cancer" => Some("exaltation"),
            "Capricorn" => Some("detriment"),
            "Gemini" | "Virgo" => Some("fall"),
            _ => None,
        }
    }
}

impl Saturn {
    pub fn dignity(&self, jd: Option<f64>) -> Option<&'static str> {
        match self.0.sign(jd) {
            "Capricorn" | "Aquarius" => Some("rulership"),
            "Libra" => Some("exaltation"),
            "Aries" => Some("detriment"),
            "Cancer" | "Leo" => Some("fall"),
            _ => None,
        }
    }
}

// Wrapper-side overrides aren't needed: Planet::sign_change_lookahead switches
// on the body id, so the wrappers inherit the right value via Deref.

// ------------------------------------------------------------------------------------------------
// Match-finder helpers (the moral equivalent of the inner Python closures).
// ------------------------------------------------------------------------------------------------

fn find_local_minima<E: FnMut(f64) -> f64 + ?Sized>(
    jds: &[f64],
    eval: &mut E,
    target_angle: f64,
) -> Option<Vec<(f64, f64)>> {
    if jds.len() < 4 {
        return None;
    }
    let angles: Vec<f64> = jds.iter().map(|&d| eval(d)).collect();

    // Convert angles into signed distance from target_angle (positive = approaching).
    let target_adjusted: Vec<f64> = angles.iter()
        .map(|&a| (a - target_angle).rem_euclid(360.0))
        .collect();
    let distances: Vec<f64> = target_adjusted.iter()
        .map(|&v| -(mod360_distance(180.0, v) - 180.0))
        .collect();

    // first derivative
    let grad: Vec<f64> = distances.windows(2).map(|w| w[1] - w[0]).collect();
    if grad.len() < 2 {
        return None;
    }
    // d(sign(grad)) shifted right by 1 (Python: np.roll(np.diff(np.sign(grad)), 1))
    let signs: Vec<f64> = grad.iter()
        .map(|v| if *v > 0.0 { 1.0 } else if *v < 0.0 { -1.0 } else { 0.0 })
        .collect();
    let sign_changes: Vec<f64> = signs.windows(2).map(|w| w[1] - w[0]).collect();
    // and second derivative of distances (similarly shifted)
    let grad2: Vec<f64> = grad.windows(2).map(|w| w[1] - w[0]).collect();

    // is_extremum[i] = sign_changes[i-1] != 0  (the np.roll by 1)
    // curves_left[i] = grad2[i-1] > 0
    // is_minimum[i] = is_extremum[i] && curves_left[i]
    let mut matches = Vec::new();
    let n = distances.len();
    // Valid index range: i must allow sign_changes[i-1] and grad2[i-1] to exist,
    // i.e. 1 <= i <= sign_changes.len() and 1 <= i <= grad2.len(). Both
    // sign_changes and grad2 have length n-2, so 1 <= i <= n-2.
    for i in 1..=n.saturating_sub(2) {
        let is_extremum = sign_changes.get(i - 1).copied().unwrap_or(0.0) != 0.0;
        let curves_left = grad2.get(i - 1).copied().unwrap_or(0.0) > 0.0;
        if is_extremum && curves_left {
            matches.push((jds[i], angles[i]));
        }
    }
    if matches.is_empty() {
        None
    } else {
        Some(matches)
    }
}

fn find_zero_crossings<E: FnMut(f64) -> f64 + ?Sized>(
    jds: &[f64],
    eval: &mut E,
) -> Option<Vec<(f64, f64)>> {
    if jds.len() < 3 {
        return None;
    }
    let speeds: Vec<f64> = jds.iter().map(|&d| eval(d)).collect();
    let signs: Vec<f64> = speeds.iter()
        .map(|v| if *v > 0.0 { 1.0 } else if *v < 0.0 { -1.0 } else { 0.0 })
        .collect();
    let sign_changes: Vec<f64> = signs.windows(2).map(|w| w[1] - w[0]).collect();

    let mut matches = Vec::new();
    for i in 1..speeds.len().saturating_sub(1) {
        if sign_changes.get(i - 1).copied().unwrap_or(0.0) != 0.0 {
            matches.push((jds[i], speeds[i]));
        }
    }
    if matches.is_empty() {
        None
    } else {
        Some(matches)
    }
}

// ------------------------------------------------------------------------------------------------
// Returns — find when a body returns to a given natal longitude.
// ------------------------------------------------------------------------------------------------

/// Find the next time `body_id` returns to its natal longitude (sampled at
/// `natal_jd`). `search_from_jd` is where the forward search begins.
/// Returns `None` if no return occurs within the body's typical period.
pub fn next_return(body_id: i32, natal_jd: f64, search_from_jd: f64) -> Option<f64> {
    init_swe();
    let natal_lon = swe::calc_ut(natal_jd, body_id as u32, SEFLG_SWIEPH as u32)
        .ok()?.out[0];
    let target = FixedZodiacPoint::new(natal_lon);
    let body = Planet::new(body_id, Some(search_from_jd), None);
    // Body-specific search horizons — solar return is annual, lunar return
    // is monthly, outer-planet returns can take decades. We give enough
    // headroom for one full period plus a margin.
    let lookahead = match body_id {
        SE_SUN => 380.0,
        SE_MOON => 31.0,
        SE_MERCURY => 100.0,
        SE_VENUS => 230.0,
        SE_MARS => 690.0,
        SE_JUPITER => 365.0 * 12.5,
        SE_SATURN => 365.0 * 30.0,
        _ => 365.0 * 200.0,
    };
    body.next_angle_to_planet(&target, 0.0, Some(search_from_jd), Some(lookahead), None, None, None)
        .map(|(jd, _, _)| jd)
}

// ------------------------------------------------------------------------------------------------
// Transits — given a natal chart, find which transiting bodies are currently
// within orb of forming an aspect to the natal positions.
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ActiveTransit {
    pub transit_body: String,
    pub natal_body: String,
    pub aspect_name: &'static str,
    pub aspect_mode: Option<&'static str>,
    pub exact_angle: f64,
    /// Difference between current angle and exact-aspect angle (degrees,
    /// always >= 0).
    pub orb_distance: f64,
    /// Whether the aspect is applying (orb shrinking) at `transit_jd`.
    pub applying: bool,
}

/// The 8 major aspects (incl. dexter+sinister mirrors of sextile/square/trine).
fn major_aspects() -> &'static [(f64, &'static str, Option<&'static str>)] {
    &[
        (0.0, "conjunction", None),
        (60.0, "sextile", Some("dexter")),
        (90.0, "square", Some("dexter")),
        (120.0, "trine", Some("dexter")),
        (180.0, "opposition", None),
        (240.0, "trine", Some("sinister")),
        (270.0, "square", Some("sinister")),
        (300.0, "sextile", Some("sinister")),
    ]
}

/// All bodies we typically transit-aspect against. Caller can override.
pub fn default_transit_bodies() -> [i32; 10] {
    [
        SE_SUN, SE_MOON, SE_MERCURY, SE_VENUS, SE_MARS,
        SE_JUPITER, SE_SATURN, SE_URANUS, SE_NEPTUNE, SE_PLUTO,
    ]
}

/// Compute active transits at `transit_jd` against natal positions at
/// `natal_jd`. Aspects within `orb` degrees of exact are reported.
/// `bodies` lists the SE_* IDs to consider on both sides — they intersect.
pub fn compute_transits(
    natal_jd: f64,
    transit_jd: f64,
    bodies: &[i32],
    orb: f64,
) -> Vec<ActiveTransit> {
    init_swe();
    // Pre-compute natal longitudes once.
    let natal_lon: Vec<f64> = bodies.iter()
        .map(|&id| swe::calc_ut(natal_jd, id as u32, SEFLG_SWIEPH as u32)
            .map(|r| r.out[0]).unwrap_or(f64::NAN))
        .collect();

    let mut out = Vec::new();
    let aspects = major_aspects();
    let dt = 1.0 / 24.0; // 1 hour for applying/separating discrimination

    for &t_id in bodies {
        let t_lon = match swe::calc_ut(transit_jd, t_id as u32, SEFLG_SWIEPH as u32) {
            Ok(r) => r.out[0],
            Err(_) => continue,
        };
        let t_lon_next = match swe::calc_ut(transit_jd + dt, t_id as u32, SEFLG_SWIEPH as u32) {
            Ok(r) => r.out[0],
            Err(_) => t_lon,
        };
        for (i, &n_id) in bodies.iter().enumerate() {
            if n_id == t_id { continue; }
            let n_lon = natal_lon[i];
            let angle = (t_lon - n_lon).rem_euclid(360.0);
            let angle_next = (t_lon_next - n_lon).rem_euclid(360.0);
            for &(target, name, mode) in aspects {
                let dist = mod360_distance(angle, target);
                if dist <= orb {
                    let dist_next = mod360_distance(angle_next, target);
                    let applying = dist_next < dist;
                    out.push(ActiveTransit {
                        transit_body: swe::get_planet_name(t_id),
                        natal_body: swe::get_planet_name(n_id),
                        aspect_name: name,
                        aspect_mode: mode,
                        exact_angle: target,
                        orb_distance: dist,
                        applying,
                    });
                }
            }
        }
    }
    // Sort by orb_distance ascending — tightest aspects first.
    out.sort_by(|a, b| a.orb_distance.partial_cmp(&b.orb_distance).unwrap());
    out
}

// ------------------------------------------------------------------------------------------------
// Fixed stars — wraps swe_fixstar_ut for any star defined in the bundled
// sefstars.txt catalog. Star names are looked up case-insensitively against
// the traditional name (e.g. "Sirius") or Bayer/Flamsteed designation
// (e.g. "alCMa").
// ------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct FixedStar {
    pub name: String,
    pub longitude: f64,
    pub latitude: f64,
    pub distance: f64,
    pub speed: f64,
    pub magnitude: f64,
}

/// Fetch ecliptic longitude, latitude, magnitude, etc. for a named star.
/// Returns `Err` with the SwissEph error string if the star isn't in the
/// catalog or the file isn't present.
pub fn fixed_star(name: &str, jd_ut: f64) -> Result<FixedStar, String> {
    init_swe();
    let mut star_buf = [0_i8; 256];
    // Copy the input name into the buffer; SwissEph rewrites it with the
    // canonical "trad_name,bayer" form on success.
    let bytes = name.as_bytes();
    let n = bytes.len().min(star_buf.len() - 1);
    for i in 0..n {
        star_buf[i] = bytes[i] as i8;
    }
    star_buf[n] = 0;

    let mut xx = [0.0_f64; 6];
    let mut serr = [0_i8; 256];
    let mut mag: f64 = f64::NAN;
    unsafe {
        let code = raw::swe_fixstar_ut(
            star_buf.as_mut_ptr(),
            jd_ut,
            (SEFLG_SWIEPH | SEFLG_SPEED) as i32,
            xx.as_mut_ptr(),
            serr.as_mut_ptr(),
        );
        if code < 0 {
            let err = std::ffi::CStr::from_ptr(serr.as_ptr())
                .to_string_lossy().into_owned();
            return Err(err);
        }
        // Magnitude lookup is a separate call.
        let mut mag_serr = [0_i8; 256];
        let _mcode = raw::swe_fixstar_mag(
            star_buf.as_mut_ptr(),
            &mut mag,
            mag_serr.as_mut_ptr(),
        );
    }
    // Decode the canonical name from star_buf.
    let canonical = unsafe {
        std::ffi::CStr::from_ptr(star_buf.as_ptr())
            .to_string_lossy().into_owned()
    };
    Ok(FixedStar {
        name: canonical,
        longitude: xx[0],
        latitude: xx[1],
        distance: xx[2],
        speed: xx[3],
        magnitude: mag,
    })
}

// ------------------------------------------------------------------------------------------------
// Sidereal zodiac / ayanamshas
// ------------------------------------------------------------------------------------------------

/// Parse an ayanamsha identifier into the SwissEph SE_SIDM_* mode integer.
pub fn parse_ayanamsha(s: &str) -> Option<(i32, &'static str)> {
    match s.to_ascii_lowercase().replace([' ', '-'], "_").as_str() {
        "fagan" | "fagan_bradley" => Some((0, "Fagan-Bradley")),
        "lahiri" => Some((1, "Lahiri")),
        "deluce" => Some((2, "DeLuce")),
        "raman" => Some((3, "Raman")),
        "ushashashi" => Some((4, "Ushashashi")),
        "krishnamurti" => Some((5, "Krishnamurti")),
        "djwhal_khul" => Some((6, "Djwhal Khul")),
        "yukteshwar" => Some((7, "Yukteshwar")),
        "jn_bhasin" => Some((8, "J.N. Bhasin")),
        "j2000" => Some((18, "J2000")),
        "galcent" | "galactic_center" => Some((17, "Galactic Center 0° Sag")),
        _ => None,
    }
}

/// Compute the ayanamsha (in degrees) for the given UT JD and SE_SIDM_* mode.
pub fn compute_ayanamsha(jd_ut: f64, mode: i32) -> f64 {
    init_swe();
    unsafe {
        raw::swe_set_sid_mode(mode, 0.0, 0.0);
        raw::swe_get_ayanamsa_ut(jd_ut)
    }
}

/// Project a tropical longitude into the sidereal frame given an ayanamsha
/// in degrees.
pub fn apply_ayanamsha(tropical_lon: f64, ayanamsha_deg: f64) -> f64 {
    (tropical_lon - ayanamsha_deg).rem_euclid(360.0)
}

// ------------------------------------------------------------------------------------------------
// Eclipses
// ------------------------------------------------------------------------------------------------

/// Eclipse kind. Solar eclipses can additionally be central/non-central; we
/// fold the central bit into the kind label for serialization simplicity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EclipseKind {
    SolarTotal,
    SolarAnnular,
    SolarHybrid,
    SolarPartial,
    LunarTotal,
    LunarPartial,
    LunarPenumbral,
}

impl EclipseKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EclipseKind::SolarTotal => "solar_total",
            EclipseKind::SolarAnnular => "solar_annular",
            EclipseKind::SolarHybrid => "solar_hybrid",
            EclipseKind::SolarPartial => "solar_partial",
            EclipseKind::LunarTotal => "lunar_total",
            EclipseKind::LunarPartial => "lunar_partial",
            EclipseKind::LunarPenumbral => "lunar_penumbral",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Eclipse {
    pub kind: EclipseKind,
    pub central: bool,
    /// JD of maximum eclipse.
    pub max_jd: f64,
    /// JD of first contact (penumbra/disc edge).
    pub first_contact_jd: Option<f64>,
    /// JD of last contact.
    pub last_contact_jd: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EclipseSearch {
    Solar,
    Lunar,
}

/// Find the next solar or lunar eclipse from `jd_start`. `backward = true`
/// searches into the past instead.
pub fn next_eclipse(
    jd_start: f64,
    search: EclipseSearch,
    backward: bool,
) -> Option<Eclipse> {
    init_swe();
    let mut tret = [0.0_f64; 10];
    let mut serr = [0_i8; 256];
    let backward_flag: i32 = if backward { 1 } else { 0 };
    let code = unsafe {
        match search {
            EclipseSearch::Solar => raw::swe_sol_eclipse_when_glob(
                jd_start, SEFLG_SWIEPH, 0,
                tret.as_mut_ptr(), backward_flag, serr.as_mut_ptr(),
            ),
            EclipseSearch::Lunar => raw::swe_lun_eclipse_when(
                jd_start, SEFLG_SWIEPH, 0,
                tret.as_mut_ptr(), backward_flag, serr.as_mut_ptr(),
            ),
        }
    };
    if code < 0 {
        return None;
    }
    let central = (code & SE_ECL_CENTRAL) != 0
        && (code & SE_ECL_NONCENTRAL) == 0;
    let kind = match search {
        EclipseSearch::Solar => {
            if (code & SE_ECL_ANNULAR_TOTAL) != 0 {
                EclipseKind::SolarHybrid
            } else if (code & SE_ECL_TOTAL) != 0 {
                EclipseKind::SolarTotal
            } else if (code & SE_ECL_ANNULAR) != 0 {
                EclipseKind::SolarAnnular
            } else if (code & SE_ECL_PARTIAL) != 0 {
                EclipseKind::SolarPartial
            } else {
                return None;
            }
        }
        EclipseSearch::Lunar => {
            if (code & SE_ECL_TOTAL) != 0 {
                EclipseKind::LunarTotal
            } else if (code & SE_ECL_PARTIAL) != 0 {
                EclipseKind::LunarPartial
            } else if (code & SE_ECL_PENUMBRAL) != 0 {
                EclipseKind::LunarPenumbral
            } else {
                return None;
            }
        }
    };
    // For solar, tret[1] = first contact, tret[4] = last contact.
    // For lunar, tret[1] = first partial-penumbral, tret[4] = last.
    let first = if tret[1] != 0.0 { Some(tret[1]) } else { None };
    let last = if tret[4] != 0.0 { Some(tret[4]) } else { None };
    Some(Eclipse {
        kind,
        central,
        max_jd: tret[0],
        first_contact_jd: first,
        last_contact_jd: last,
    })
}

/// Find all eclipses of the requested kind(s) within `[jd_start, jd_end]`.
/// `solar` and `lunar` flags select the categories; both default to true at
/// the caller's discretion. Stops after `limit` events.
pub fn eclipses_within_period(
    jd_start: f64,
    jd_end: f64,
    solar: bool,
    lunar: bool,
    limit: usize,
) -> Vec<Eclipse> {
    let mut out: Vec<Eclipse> = Vec::new();
    let categories: Vec<EclipseSearch> = match (solar, lunar) {
        (true, true) => vec![EclipseSearch::Solar, EclipseSearch::Lunar],
        (true, false) => vec![EclipseSearch::Solar],
        (false, true) => vec![EclipseSearch::Lunar],
        _ => return out,
    };
    for cat in categories {
        let mut t = jd_start;
        loop {
            let Some(e) = next_eclipse(t, cat, false) else { break; };
            if e.max_jd > jd_end {
                break;
            }
            t = e.max_jd + 1.0;
            out.push(e);
            if out.len() >= limit {
                break;
            }
        }
        if out.len() >= limit {
            break;
        }
    }
    out.sort_by(|a, b| a.max_jd.partial_cmp(&b.max_jd).unwrap());
    out.truncate(limit);
    out
}

// ------------------------------------------------------------------------------------------------
// Rise/set wrapper around the raw libswisseph-sys binding, since the high-level
// crate doesn't expose `swe_rise_trans`.
// ------------------------------------------------------------------------------------------------

fn rise_trans(jd_ut: f64, ipl: i32, observer: &LatLong, rsmi: i32) -> f64 {
    let mut tret = [0.0_f64; 10];
    let mut serr = [0_i8; 256];
    let mut geopos = [observer.long, observer.lat, 0.0];
    unsafe {
        let _code = raw::swe_rise_trans(
            jd_ut,
            ipl,
            std::ptr::null_mut(),
            SEFLG_SWIEPH,
            rsmi,
            geopos.as_mut_ptr(),
            0.0,
            0.0,
            tret.as_mut_ptr(),
            serr.as_mut_ptr(),
        );
        if _code < 0 {
            // Decode the error string for surfacing in panics.
            let cstr = CStr::from_ptr(serr.as_ptr());
            panic!("swe_rise_trans failed: {}", cstr.to_string_lossy());
        }
    }
    tret[0]
}
