// SPDX-License-Identifier: MIT AND AGPL-3.0-only

//! Numerical tests ported from cerridwen/tests.py.
//!
//! Reference values come from running the original Python suite, which
//! validated against USNO data and astropy. The Rust port uses
//! `libswisseph-sys`, which bundles a slightly different Swiss Ephemeris
//! version than `pyswisseph`, so the long-baseline Jupiter-Saturn 2020
//! conjunction drifts by ~3 s; everything else matches to the second.
//! We use a 10 s tolerance for ISO comparisons.

use approx::assert_abs_diff_eq;
use cerridwen::planets::{Jupiter, Mercury, Moon, Saturn, Sun};
use cerridwen::utils::{iso2jd, jd2iso, parse_jd_or_iso_date};
use cerridwen::LatLong;

const TOL: f64 = 1e-3;

/// Assert two ISO timestamps refer to instants no more than `max_seconds` apart.
fn assert_iso_within(actual: &str, expected: &str, max_seconds: f64) {
    let a = iso2jd(actual).expect("actual is valid ISO");
    let e = iso2jd(expected).expect("expected is valid ISO");
    let diff_seconds = (a - e).abs() * 86400.0;
    assert!(
        diff_seconds <= max_seconds,
        "iso timestamps differ by {diff_seconds:.3}s (> {max_seconds}s): actual={actual}, expected={expected}",
    );
    if std::env::var("CERRIDWEN_REPORT_DRIFT").is_ok() {
        eprintln!("drift: {diff_seconds:.3}s — actual={actual}, expected={expected}");
    }
}

#[test]
fn moon_age() {
    let m = Moon::at_jd(2456794.949305556);
    assert_abs_diff_eq!(m.age(None), 18.189345157705247, epsilon = TOL);
}

#[test]
fn moon_speed() {
    let jd = iso2jd("1983-07-01 7:40:00").unwrap();
    let m = Moon::at_jd(jd);
    assert_abs_diff_eq!(m.speed(None), 11.989598784682862, epsilon = 1e-3);
}

#[test]
fn moon_period_length() {
    let m = Moon::at_jd(2456794.949305556);
    assert_abs_diff_eq!(m.period_length(None), 29.517_968_974_076_21, epsilon = TOL);
}

#[test]
fn next_new_moon() {
    let m = Moon::at_jd(2456794.9541666);
    assert_abs_diff_eq!(
        m.next_new_moon(None).jd,
        2_456_806.277_929_372,
        epsilon = TOL
    );
}

#[test]
fn next_full_moon() {
    let m = Moon::at_jd(2456731.376389);
    assert_abs_diff_eq!(m.next_full_moon(None).jd, 2456733.2141234726, epsilon = TOL);
}

#[test]
fn rise_set() {
    let obs = LatLong::new(52.0, 13.0).unwrap();
    let m = Moon::at(Some(2456798.2), Some(obs));
    assert_iso_within(&m.next_rise().iso_date(), "2014-05-20 23:37:17", 10.0);
    let s = Sun::at(Some(2456799.9), Some(obs));
    assert_iso_within(&s.next_rise().iso_date(), "2014-05-23 03:03:05", 10.0);
}

#[test]
fn iso_jd_roundtrip() {
    let now = cerridwen::jd_now();
    let iso = jd2iso(now);
    assert_eq!(jd2iso(iso2jd(&iso).unwrap()), iso);
}

#[test]
fn parse_date_valid_jd() {
    parse_jd_or_iso_date("1").unwrap();
    parse_jd_or_iso_date("2456799.9897213").unwrap();
}

#[test]
fn parse_date_valid_iso() {
    parse_jd_or_iso_date("2014-05-20T23:37:17").unwrap();
    parse_jd_or_iso_date("2014-05-20 23:37:17").unwrap();
}

#[test]
fn parse_date_invalid_garbage_t() {
    assert!(parse_jd_or_iso_date("2014-05-20T23:37:17X").is_err());
}

#[test]
fn parse_date_invalid_garbage_jd() {
    assert!(parse_jd_or_iso_date("123garbage.5").is_err());
}

#[test]
fn next_sign_change() {
    let m = Moon::at_jd(2456867.914486644);
    let nsc = m.next_sign_change(None);
    assert_iso_within(&jd2iso(nsc), "2014-07-31 16:09:11", 10.0);
    let landed = Moon::at_jd(nsc);
    assert_eq!(landed.position(None).sign(), "Libra");
}

#[test]
fn angle_finder_new_moon_virgo_2014() {
    let m = Moon::at_jd(2456868.0);
    let sun = Sun::new();
    let (jd, _, _) = m
        .next_angle_to_planet(&sun.0, 0.0, None, None, None, None, None)
        .unwrap();
    assert_iso_within(&jd2iso(jd), "2014-08-25 14:12:46", 10.0);
}

#[test]
fn angle_finder_symmetric_sun_moon() {
    let s = Sun::at_jd(2456868.0);
    let m = Moon::at_jd(2456868.0);
    let (jd1, _, _) =
        s.0.next_angle_to_planet(&m.0, 0.0, None, None, None, None, None)
            .unwrap();
    let (jd2, _, _) =
        m.0.next_angle_to_planet(&s.0, 0.0, None, None, None, None, None)
            .unwrap();
    assert_eq!(jd2iso(jd1), jd2iso(jd2));
}

#[test]
fn angle_finder_jupiter_saturn_great_conjunction_2020() {
    let j = Jupiter::at_jd(2456868.0);
    let s = Saturn::at_jd(2456868.0);
    let (jd1, _, _) =
        j.0.next_angle_to_planet(&s.0, 0.0, None, None, None, None, None)
            .unwrap();
    let (jd2, _, _) =
        s.0.next_angle_to_planet(&j.0, 0.0, None, None, None, None, None)
            .unwrap();
    assert_eq!(jd2iso(jd1), jd2iso(jd2));
    assert_iso_within(&jd2iso(jd1), "2020-12-21 18:20:29", 10.0);
}

#[test]
fn rx_finder_forwards() {
    let jd = iso2jd("2014-10-03 7:40:00").unwrap();
    let res = Mercury::at_jd(jd).next_rx_event(None, Some(10.0)).unwrap();
    assert_eq!(res.1, "rx");
    assert_iso_within(&jd2iso(res.0), "2014-10-04 17:02:15", 10.0);
}

#[test]
fn rx_finder_backwards() {
    let jd = iso2jd("2014-10-30 7:40:00").unwrap();
    let res = Mercury::at_jd(jd).next_rx_event(None, Some(-10.0)).unwrap();
    assert_eq!(res.1, "direct");
    assert_iso_within(&jd2iso(res.0), "2014-10-25 19:16:33", 10.0);
}

#[test]
#[should_panic]
fn rx_finder_sun_not_allowed() {
    Sun::new().0.next_rx_event(None, None);
}

#[test]
#[should_panic]
fn rx_finder_moon_not_allowed() {
    Moon::new().0.next_rx_event(None, None);
}

#[test]
fn sign_change_mercury_1() {
    let m = Mercury::at_jd(2445548.93216);
    let jd = m.next_sign_change(None);
    let landed = Mercury::at_jd(jd);
    assert_eq!(landed.0.sign(None), "Libra");
}

#[test]
fn sign_change_mercury_2() {
    let m = Mercury::at_jd(2447727.9);
    let jd = m.next_sign_change(None);
    let landed = Mercury::at_jd(jd);
    assert_eq!(landed.0.sign(None), "Virgo");
}

#[test]
fn mercury_semisextile_sun_impossible() {
    let m = Mercury::at_jd(2460932.0);
    let sun = Sun::at_jd(2460932.0);
    assert!(m
        .0
        .next_angle_to_planet(&sun.0, 30.0, None, None, None, None, None)
        .is_none());
}

// ------------------------------------------------------------------------------------------------
// Methods that were `NotImplementedError` in Python.
// ------------------------------------------------------------------------------------------------

#[test]
fn relative_orbital_velocity_sun_is_one() {
    // Sun entry uses Earth's heliocentric period, so the relative
    // velocity of "Sun = Earth's orbit" is 1 by construction.
    let s = Sun::new();
    assert_abs_diff_eq!(s.0.relative_orbital_velocity(), 1.0, epsilon = 1e-9);
}

#[test]
fn relative_orbital_velocity_mercury_faster_than_earth() {
    // Mercury's orbital velocity is ~47.4 km/s, Earth's ~29.78 km/s →
    // ratio ≈ 1.59. The cube-root formula gives the same number.
    let m = Mercury::new();
    let v = m.0.relative_orbital_velocity();
    assert!((1.59..1.61).contains(&v), "got {v}");
}

#[test]
fn relative_orbital_velocity_mars_slower_than_earth() {
    use cerridwen::planets::Mars;
    let m = Mars::new();
    let v = m.0.relative_orbital_velocity();
    assert!((0.80..0.82).contains(&v), "got {v}");
}

#[test]
fn lunation_number_today_is_in_expected_range() {
    // Brown lunation 1 was Jan 1923. Lunations advance ~12.37 per year.
    // Today (May 2026) we expect somewhere in the 1270s.
    let m = Moon::new();
    let n = m.lunation_number(None);
    assert!((1270..=1290).contains(&n), "got {n}");
}

#[test]
fn lunation_number_at_brown_epoch() {
    // Half a day after Brown's reference new moon (sweph: 2423436.6117);
    // we need the new moon to be far enough into the search window for
    // the local-minimum detector to register it.
    let m = Moon::at_jd(2423437.2);
    assert_eq!(m.lunation_number(None), 1);
}

#[test]
fn lunation_number_one_synodic_month_later() {
    // ~1 synodic month after Brown's epoch → lunation 2.
    let m = Moon::at_jd(2423437.2 + 29.5);
    assert_eq!(m.lunation_number(None), 2);
}

#[test]
fn next_event_sun_is_an_ingress() {
    // Sun has no rise/set without an observer, no rx, no moon-phase —
    // only ingress remains, so next_event must return the Sun's next sign
    // change wrapped as an ingress event.
    let s = Sun::new();
    let ev = s.0.next_event().expect("Sun always has a next ingress");
    assert!(ev.description.contains("ingress"));
    let nsc = s.0.next_sign_change(None);
    assert!(
        (ev.jd - nsc).abs() < 1e-9,
        "expected ev.jd ≈ next sign change"
    );
}

#[test]
fn next_event_moon_picks_earliest() {
    // The Moon has a sign change every ~2.5 days and a new/full moon
    // every ~14.7 days, so the earliest of {ingress, new_or_full} is the
    // ingress most of the time. We just verify the function returns a
    // candidate within the expected outer bound (next_new_or_full_moon).
    let m = Moon::new();
    let ev = m.0.next_event().expect("Moon always has a next event");
    let n_or_f = m.next_new_or_full_moon(None);
    assert!(ev.jd <= n_or_f.jd + 1e-9);
}

#[test]
fn void_of_course_basic() {
    // Pick a Moon JD where it is mid-sign and verify the function returns
    // something coherent: VoC time must lie within (now, next_sign_change].
    let jd = 2456867.0;
    let m = Moon::at_jd(jd);
    let nsc = m.0.next_sign_change(None);
    let (_voc, until) = m.is_void_of_course(None, false);
    assert!(
        until > jd && until <= nsc + 1e-6,
        "until={until} jd={jd} nsc={nsc}"
    );
}

#[test]
fn void_of_course_traditional_only_invariant() {
    // The traditional flag uses a subset of the partners modern uses.
    // Therefore: if the Moon is VoC under the modern definition (no
    // aspects to any of the 9 bodies), it must also be VoC under the
    // traditional definition (no aspects to 6 of those 9).
    let m = Moon::at_jd(2456867.0);
    let (voc_modern, _) = m.is_void_of_course(None, false);
    let (voc_trad, _) = m.is_void_of_course(None, true);
    if voc_modern {
        assert!(
            voc_trad,
            "modern says VoC but traditional disagrees — impossible"
        );
    }
}

#[test]
fn zero_jd() {
    // JD=0 falls in 4713 BC and exercises the BC ephemeris files
    // (`seplm48.se1` etc.). The exact longitude varies by ~0.01° between
    // pyswisseph and libswisseph-sys ephemeris versions, so we assert a
    // loose tolerance.
    let m = Mercury::at_jd(0.0);
    let lon = m.position(None).absolute_degrees;
    assert_abs_diff_eq!(lon, 222.4, epsilon = 0.05);
}
