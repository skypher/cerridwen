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
        "iso timestamps differ by {:.3}s (> {}s): actual={}, expected={}",
        diff_seconds, max_seconds, actual, expected,
    );
    if std::env::var("CERRIDWEN_REPORT_DRIFT").is_ok() {
        eprintln!("drift: {:.3}s — actual={}, expected={}", diff_seconds, actual, expected);
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
    assert_abs_diff_eq!(m.period_length(None), 29.517968974076211, epsilon = TOL);
}

#[test]
fn next_new_moon() {
    let m = Moon::at_jd(2456794.9541666);
    assert_abs_diff_eq!(m.next_new_moon(None).jd, 2456806.2779293722, epsilon = TOL);
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
    let (jd, _, _) = m.next_angle_to_planet(&sun.0, 0.0, None, None, None, None, None).unwrap();
    assert_iso_within(&jd2iso(jd), "2014-08-25 14:12:46", 10.0);
}

#[test]
fn angle_finder_symmetric_sun_moon() {
    let s = Sun::at_jd(2456868.0);
    let m = Moon::at_jd(2456868.0);
    let (jd1, _, _) = s.0.next_angle_to_planet(&m.0, 0.0, None, None, None, None, None).unwrap();
    let (jd2, _, _) = m.0.next_angle_to_planet(&s.0, 0.0, None, None, None, None, None).unwrap();
    assert_eq!(jd2iso(jd1), jd2iso(jd2));
}

#[test]
fn angle_finder_jupiter_saturn_great_conjunction_2020() {
    let j = Jupiter::at_jd(2456868.0);
    let s = Saturn::at_jd(2456868.0);
    let (jd1, _, _) = j.0.next_angle_to_planet(&s.0, 0.0, None, None, None, None, None).unwrap();
    let (jd2, _, _) = s.0.next_angle_to_planet(&j.0, 0.0, None, None, None, None, None).unwrap();
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
    assert!(m.0.next_angle_to_planet(&sun.0, 30.0, None, None, None, None, None).is_none());
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
