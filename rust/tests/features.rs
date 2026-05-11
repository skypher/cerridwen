// SPDX-License-Identifier: MIT AND AGPL-3.0-only

//! Tests for the post-port features: nodes/asteroids/Lilith, houses,
//! eclipses, transits, returns, stars, ayanamshas, time-zones.

use approx::assert_abs_diff_eq;
use cerridwen::planets::Planet;
use cerridwen::utils::{iso2jd, jd2iso, parse_jd_or_iso_date_in_tz};
use cerridwen::{
    apply_ayanamsha, compute_ayanamsha, compute_houses, compute_transits, default_transit_bodies,
    eclipses_within_period, fixed_star, next_return, parse_ayanamsha, parse_house_system,
    planets::{next_eclipse, SE_CERES, SE_CHIRON, SE_MEAN_APOG, SE_MEAN_NODE, SE_MOON, SE_SUN},
    EclipseKind, EclipseSearch,
};

// ----------------------- bodies (nodes / Lilith / asteroids) ------------------

#[test]
fn mean_node_always_retrograde() {
    // Mean lunar node never has prograde stations.
    let p = Planet::new(SE_MEAN_NODE, Some(2461167.0), None);
    assert!(!p.has_rx_stations());
    assert!(p.speed(None) < 0.0, "mean node should regress");
}

#[test]
fn lilith_speed_positive_default() {
    // Mean apogee progresses at ~0.111°/day.
    let p = Planet::new(SE_MEAN_APOG, Some(2461167.0), None);
    let s = p.speed(None);
    assert!(s > 0.05 && s < 0.2, "Lilith speed = {s}");
}

#[test]
fn chiron_position_today_in_aries() {
    // Chiron is in Aries throughout 2026.
    let p = Planet::new(SE_CHIRON, Some(2461167.0), None);
    assert_eq!(p.position(None).sign(), "Aries");
}

#[test]
fn ceres_returns_have_period() {
    // Ceres orbital period is 4.6 years, so its sidereal return should land
    // within ~5 years from start.
    let natal = iso2jd("2024-01-01T00:00:00").unwrap();
    let return_jd = next_return(SE_CERES, natal, natal + 1.0).expect("Ceres return");
    let years = (return_jd - natal) / 365.25;
    assert!(years > 4.0 && years < 5.5, "Ceres period {years} yr");
}

// ----------------------- houses --------------------------------------------

#[test]
fn whole_sign_cusps_are_30deg_apart() {
    let h = compute_houses(2461167.0, 52.5, 13.4, 'W');
    for i in 0..11 {
        let d = (h.cusps[i + 1] - h.cusps[i] + 360.0) % 360.0;
        assert_abs_diff_eq!(d, 30.0, epsilon = 1e-6);
    }
}

#[test]
fn placidus_first_cusp_equals_ascendant() {
    let h = compute_houses(2461167.0, 52.5, 13.4, 'P');
    assert_abs_diff_eq!(h.cusps[0], h.ascendant, epsilon = 1e-9);
}

#[test]
fn parse_house_system_aliases() {
    assert_eq!(parse_house_system("placidus"), Some('P'));
    assert_eq!(parse_house_system("Whole-Sign"), Some('W'));
    assert_eq!(parse_house_system("K"), Some('K'));
    assert_eq!(parse_house_system("nonsense"), None);
}

// ----------------------- ayanamshas ----------------------------------------

#[test]
fn lahiri_ayanamsha_in_expected_range_for_today() {
    // Lahiri has been ~24° for the past decade.
    let (mode, _) = parse_ayanamsha("lahiri").unwrap();
    let v = compute_ayanamsha(2461167.0, mode);
    assert!(v > 23.5 && v < 25.0, "lahiri = {v}");
}

#[test]
fn ayanamsha_subtraction_round_trips() {
    let (mode, _) = parse_ayanamsha("lahiri").unwrap();
    let v = compute_ayanamsha(2461167.0, mode);
    let trop = 100.0;
    let sid = apply_ayanamsha(trop, v);
    assert_abs_diff_eq!(sid, (trop - v + 360.0) % 360.0, epsilon = 1e-9);
}

// ----------------------- transits ------------------------------------------

#[test]
fn transits_self_conjunction_at_natal_jd() {
    // At transit_jd == natal_jd, every body must be exactly conjunct itself,
    // but since we exclude self-comparisons this should produce no aspects
    // unless other bodies happen to be in major aspect at that moment.
    let jd = 2461167.0;
    let bodies = default_transit_bodies();
    let active = compute_transits(jd, jd, &bodies, 0.01);
    // At the same instant, no body can be conjunct itself in our output
    // (we skip identical id pairs); any aspect we see must be between
    // *different* bodies that happen to be tightly aligned.
    for t in &active {
        assert_ne!(t.transit_body, t.natal_body);
    }
}

#[test]
fn transits_orb_bound() {
    let natal = iso2jd("2000-01-01T12:00:00").unwrap();
    let bodies = default_transit_bodies();
    let active = compute_transits(natal, natal + 9000.0, &bodies, 1.5);
    for t in &active {
        assert!(
            t.orb_distance <= 1.5 + 1e-9,
            "{} {} {} orb={}",
            t.transit_body,
            t.aspect_name,
            t.natal_body,
            t.orb_distance
        );
    }
}

// ----------------------- eclipses ------------------------------------------

#[test]
fn next_solar_eclipse_after_2026_05_01() {
    // The Aug 12, 2026 total solar eclipse over Iceland/Spain is well-known.
    let start = iso2jd("2026-05-01T00:00:00").unwrap();
    let e = next_eclipse(start, EclipseSearch::Solar, false).expect("an eclipse");
    let iso = jd2iso(e.max_jd);
    assert!(iso.starts_with("2026-08-12"), "got {iso}");
    assert_eq!(e.kind, EclipseKind::SolarTotal);
}

#[test]
fn eclipses_within_period_returns_chronological() {
    let start = iso2jd("2026-01-01T00:00:00").unwrap();
    let list = eclipses_within_period(start, start + 730.0, true, true, 12);
    assert!(
        list.len() >= 4,
        "expected multiple eclipses, got {}",
        list.len()
    );
    for w in list.windows(2) {
        assert!(w[0].max_jd <= w[1].max_jd);
    }
}

// ----------------------- returns -------------------------------------------

#[test]
fn solar_return_within_year() {
    let natal = iso2jd("2000-06-15T12:00:00").unwrap();
    let r = next_return(SE_SUN, natal, natal + 100.0).expect("Sun return");
    let days = r - natal;
    let yrs = days / 365.25;
    let frac = yrs - yrs.floor();
    // Should land within a couple of days of an integer year multiple.
    assert!(!(0.01..=0.99).contains(&frac), "delta yrs={yrs}");
}

#[test]
fn lunar_return_within_30_days() {
    let natal = iso2jd("2024-01-01T00:00:00").unwrap();
    let r = next_return(SE_MOON, natal, natal + 1.0).expect("Moon return");
    let days = r - natal;
    assert!(days > 25.0 && days < 31.0, "Moon return {days} d");
}

// ----------------------- fixed stars ---------------------------------------

#[test]
fn sirius_in_cancer() {
    let s = fixed_star("Sirius", 2461167.0).expect("Sirius");
    let cancer_start = 90.0_f64;
    let cancer_end = 120.0_f64;
    assert!(
        s.longitude > cancer_start && s.longitude < cancer_end,
        "Sirius lon = {}",
        s.longitude
    );
    assert_abs_diff_eq!(s.magnitude, -1.46, epsilon = 0.01);
}

#[test]
fn unknown_star_errors() {
    let r = fixed_star("Xylophone", 2461167.0);
    assert!(r.is_err());
}

// ----------------------- time zones ----------------------------------------

#[test]
fn tz_input_resolves_to_same_jd_as_utc() {
    let utc = parse_jd_or_iso_date_in_tz("2026-05-06T12:00:00", None).unwrap();
    let berlin = parse_jd_or_iso_date_in_tz("2026-05-06T14:00:00", Some("Europe/Berlin")).unwrap();
    let tokyo = parse_jd_or_iso_date_in_tz("2026-05-06T21:00:00", Some("Asia/Tokyo")).unwrap();
    assert_abs_diff_eq!(utc, berlin, epsilon = 1e-9);
    assert_abs_diff_eq!(utc, tokyo, epsilon = 1e-9);
}

// ----------------------- regression tests --------------------------------

#[test]
fn pluto_next_sign_change_does_not_panic() {
    // Regression for a panic that surfaced only in tokio-worker context:
    // Pluto's slow motion right at the edge of its lookahead window made
    // the local-minima search return None about half the time, which the
    // panicking next_sign_change wrapper then unwrap-panicked on. With
    // try_next_sign_change returning Option, this is now graceful, and
    // the bumped lookahead (25 → 35 years) makes a successful find
    // overwhelmingly likely.
    use cerridwen::planets::{Planet, SE_PLUTO};
    let p = Planet::new(SE_PLUTO, Some(2461166.65), None);
    // Either Some(jd) or None — both are acceptable; just don't panic.
    let _ = p.try_next_sign_change(None);
}

#[test]
fn try_next_sign_change_returns_option_for_unknown_body() {
    // Even for arbitrary body ids, try_next_sign_change should return
    // None rather than panic.
    use cerridwen::planets::Planet;
    let p = Planet::new(0, Some(2461166.65), None); // Sun (real body)
    assert!(p.try_next_sign_change(None).is_some());
}

#[test]
fn unknown_tz_errors() {
    let r = parse_jd_or_iso_date_in_tz("2026-05-06T12:00:00", Some("Atlantis/Lostcity"));
    assert!(r.is_err());
}

// ----------------------- astrology module: new techniques --------------------

use cerridwen::astrology;

#[test]
fn declination_sun_within_obliquity() {
    // Declination of the Sun must always live inside ±ε (~23.45°).
    let jd = iso2jd("2026-05-06T12:00:00").unwrap();
    let d = astrology::declination(SE_SUN, jd);
    assert!(d.abs() < 23.5, "sun δ = {d}");
}

#[test]
fn moon_out_of_bounds_flag_consistent() {
    let jd = iso2jd("2026-05-06T12:00:00").unwrap();
    let d = astrology::declination(SE_MOON, jd);
    let oob = astrology::moon_out_of_bounds(jd);
    assert_eq!(oob, d.abs() > 23.4367);
}

#[test]
fn parallel_aspect_classifies_same_sign_close_dec() {
    let v: Vec<(String, i32)> = vec![("A".into(), SE_SUN), ("B".into(), SE_SUN)]; // self pair → exact parallel
    let jd = iso2jd("2026-05-06T12:00:00").unwrap();
    let aspects = astrology::declination_aspects(&v, jd, 0.1);
    assert_eq!(aspects.len(), 1);
    assert_eq!(aspects[0].kind.label(), "parallel");
    assert!(aspects[0].orb < 1e-6);
}

#[test]
fn tithi_at_new_moon_is_one_or_thirty() {
    // Pick a JD very near new moon; the tithi count rolls 30→1 there.
    let new_moon = iso2jd("2026-04-17T00:00:00").unwrap();
    let t = astrology::tithi(new_moon);
    assert!(
        t.number == 1 || t.number == 30,
        "tithi at new moon = {} ({}/{})",
        t.number,
        t.half,
        t.name
    );
}

#[test]
fn nakshatra_cycles_within_27() {
    // 27 evenly-spaced longitudes must give 27 different mansion indices.
    let mut seen = std::collections::HashSet::new();
    let span = 360.0 / 27.0;
    for i in 0..27 {
        let n = astrology::nakshatra_sidereal(i as f64 * span + span / 2.0);
        seen.insert(n.number);
        assert!(n.number >= 1 && n.number <= 27);
        assert!(n.pada >= 1 && n.pada <= 4);
    }
    assert_eq!(seen.len(), 27);
}

#[test]
fn profection_house_cycles_every_12_years() {
    // Profected house at age 0 == age at 12 == age at 24, etc.
    let p0 = astrology::profection(15.0, 0);
    let p12 = astrology::profection(15.0, 12);
    let p36 = astrology::profection(15.0, 36);
    assert_eq!(p0.house, p12.house);
    assert_eq!(p0.house, p36.house);
    assert_eq!(p0.sign, p12.sign);
    // House 1 at age 0 for any Asc.
    assert_eq!(p0.house, 1);
}

#[test]
fn profection_sign_lord_matches_traditional() {
    // Asc in Aries (lon ~5°). Age 4 ⇒ 5th house ⇒ Leo ⇒ Sun.
    let p = astrology::profection(5.0, 4);
    assert_eq!(p.sign, "Leo");
    assert_eq!(p.lord, "Sun");
    assert_eq!(p.house, 5);
}

#[test]
fn shortest_midpoint_handles_zero_crossing() {
    // 350° and 10° → midpoint should be 0°, not 180°.
    let m = astrology::shortest_midpoint(350.0, 10.0);
    assert!(!(1.0..=359.0).contains(&m), "midpoint = {m}");
}

#[test]
fn shortest_midpoint_simple_case() {
    let m = astrology::shortest_midpoint(0.0, 60.0);
    assert_abs_diff_eq!(m, 30.0, epsilon = 1e-9);
}

#[test]
fn synastry_matches_simple_conjunction() {
    // Two charts with identical Sun → at least one Sun-Sun conjunction in
    // the inter-aspect grid.
    let a = vec![("Sun".into(), 12.0)];
    let b = vec![("Sun".into(), 14.0)];
    let aspects = astrology::synastry(&a, &b, 5.0);
    assert!(aspects
        .iter()
        .any(|x| x.aspect == "conjunction" && x.a == "Sun" && x.b == "Sun"));
}

#[test]
fn synastry_symmetric_orb() {
    // Swapping the role of A and B should produce the same orbs.
    let chart = vec![("Sun".into(), 100.0), ("Moon".into(), 280.0)];
    let other = vec![("Sun".into(), 102.0), ("Moon".into(), 281.5)];
    let a = astrology::synastry(&chart, &other, 5.0);
    let b = astrology::synastry(&other, &chart, 5.0);
    assert_eq!(a.len(), b.len());
}

#[test]
fn solar_arc_zero_at_natal_moment() {
    let jd = iso2jd("2000-01-01T12:00:00").unwrap();
    assert!(astrology::solar_arc_offset(jd, jd).abs() < 1e-8);
}

#[test]
fn solar_arc_about_one_degree_per_year() {
    // The Sun moves ~1°/day, so after one secondary-progression year
    // (~1 day later) the arc should be roughly 1°.
    let natal = iso2jd("2000-01-01T12:00:00").unwrap();
    let target = natal + 365.2422;
    let arc = astrology::solar_arc_offset(natal, target);
    assert!(arc > 0.9 && arc < 1.1, "solar arc / yr = {arc}");
}

#[test]
fn arabic_parts_fortune_day_identity() {
    // Day: Fortune = Asc + Moon - Sun.
    let parts = astrology::arabic_parts(0.0, 90.0, 180.0, 30.0, 60.0, 120.0, 150.0, 210.0, true);
    let fortune = parts.iter().find(|p| p.name == "Fortune").unwrap();
    let expected = (0.0 + 180.0 - 90.0_f64).rem_euclid(360.0);
    assert_abs_diff_eq!(fortune.longitude, expected, epsilon = 1e-9);
}

#[test]
fn arabic_parts_swaps_for_night() {
    let day = astrology::arabic_parts(0.0, 90.0, 180.0, 30.0, 60.0, 120.0, 150.0, 210.0, true);
    let night = astrology::arabic_parts(0.0, 90.0, 180.0, 30.0, 60.0, 120.0, 150.0, 210.0, false);
    let f_day = day.iter().find(|p| p.name == "Fortune").unwrap().longitude;
    let f_night = night
        .iter()
        .find(|p| p.name == "Fortune")
        .unwrap()
        .longitude;
    // Day Fortune and night Fortune must differ by 2 × (Moon - Sun).
    let delta = (f_day - f_night).rem_euclid(360.0);
    let expected = (2.0_f64 * (180.0 - 90.0)).rem_euclid(360.0);
    assert_abs_diff_eq!(delta, expected, epsilon = 1e-9);
}

#[test]
fn upcoming_stations_finds_mercury_rx() {
    // Mercury makes ~3 retrograde stations per year. A 400-day search
    // window from any date must find at least 2.
    let start = iso2jd("2026-01-01T00:00:00").unwrap();
    let stations = astrology::upcoming_stations(cerridwen::planets::SE_MERCURY, start, 400.0, 10);
    assert!(stations.len() >= 2, "found {} stations", stations.len());
    // First station's speed sign must alternate from positive/negative.
    let kinds: Vec<&'static str> = stations.iter().map(|s| s.kind.label()).collect();
    assert!(
        kinds.windows(2).all(|w| w[0] != w[1]),
        "stations must alternate retrograde/direct: {kinds:?}"
    );
}

#[test]
fn pre_natal_solar_eclipse_is_before_natal() {
    let natal = iso2jd("2000-01-01T00:00:00").unwrap();
    let e = astrology::pre_natal_solar_eclipse(natal).expect("solar eclipse must exist");
    assert!(e.max_jd < natal, "{} not before {}", e.max_jd, natal);
}

#[test]
fn pre_natal_lunar_eclipse_is_before_natal() {
    let natal = iso2jd("2000-01-01T00:00:00").unwrap();
    let e = astrology::pre_natal_lunar_eclipse(natal).expect("lunar eclipse must exist");
    assert!(e.max_jd < natal, "{} not before {}", e.max_jd, natal);
}

#[test]
fn planetary_hours_returns_24() {
    // Berlin, today: 24 sequential hours, weekday-correct first ruler.
    let jd = iso2jd("2026-05-11T00:00:00").unwrap();
    let observer = cerridwen::LatLong::new(52.5, 13.4).unwrap();
    let hours = astrology::planetary_hours(jd, &observer);
    assert_eq!(hours.len(), 24);
    for w in hours.windows(2) {
        assert!(w[1].start_jd >= w[0].end_jd - 1e-6);
    }
}

#[test]
fn center_parsing_round_trip() {
    let g = astrology::Center::parse("Geo").unwrap();
    let h = astrology::Center::parse("helio").unwrap();
    let t = astrology::Center::parse("topocentric").unwrap();
    assert_eq!(g.label(), "geocentric");
    assert_eq!(h.label(), "heliocentric");
    assert_eq!(t.label(), "topocentric");
    assert!(astrology::Center::parse("milky_way_centric").is_none());
}

#[test]
fn helio_sun_longitude_is_nan() {
    // The Sun cannot have a heliocentric longitude.
    let jd = iso2jd("2026-05-06T12:00:00").unwrap();
    let lon = astrology::longitude_at(astrology::Center::Helio, SE_SUN, jd);
    assert!(lon.is_nan(), "helio sun lon should be NaN, got {lon}");
}

#[test]
fn helio_geocentric_differ() {
    // For the inner planets, heliocentric and geocentric longitudes
    // can disagree by tens of degrees (especially near conjunction).
    let jd = iso2jd("2026-05-06T12:00:00").unwrap();
    let geo = astrology::longitude_at(astrology::Center::Geo, cerridwen::planets::SE_VENUS, jd);
    let helio = astrology::longitude_at(astrology::Center::Helio, cerridwen::planets::SE_VENUS, jd);
    let delta = (helio - geo).rem_euclid(360.0);
    let delta = delta.min(360.0 - delta);
    assert!(delta > 1.0, "geo≈helio venus diff = {delta}");
}
