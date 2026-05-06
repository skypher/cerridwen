use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike, Utc};
use swisseph::swe;

use crate::defs::ASPECTS;

/// Julian day at the J2000.0 epoch (= 2000-01-01T12:00:00).
const J2000_JD: f64 = 2451545.0;
/// J2000.0 in Unix seconds (UTC).
const J2000_UNIX: i64 = 946_728_000;
/// Gregorian calendar flag (matches `SE_GREG_CAL` in swephexp.h).
const SE_GREG_CAL: i32 = 1;

/// Current Julian day. We match the Swiss Ephemeris UT convention here so that
/// values flowing into `swe_calc_ut`/`swe_rise_trans` are interpreted correctly.
pub fn jd_now() -> f64 {
    unix_to_jd(Utc::now())
}

fn unix_to_jd(dt: DateTime<Utc>) -> f64 {
    let secs = dt.timestamp() - J2000_UNIX;
    let nanos = dt.timestamp_subsec_nanos() as f64 / 1.0e9;
    J2000_JD + (secs as f64 + nanos) / 86400.0
}

/// Convert a Julian day into an ISO 8601 timestamp at second precision.
///
/// Uses `swe_revjul` so the calendar conversion matches sweph's own — which is
/// what the upstream cerridwen tests assume (their reference timestamps come
/// from sweph rise/set output and from astropy's scale-agnostic ISO formatting).
pub fn jd2iso(jd: f64) -> String {
    let (year, month, day, hour_frac) = swe::revjul(jd, SE_GREG_CAL);
    let total_seconds = (hour_frac * 3600.0).round() as i64;
    let mut h = total_seconds / 3600;
    let mut m = (total_seconds % 3600) / 60;
    let mut s = total_seconds % 60;
    let mut day = day;
    let mut month = month;
    let mut year = year;

    // Carry the second-rounded value in case it crossed a day boundary.
    if h >= 24 {
        // Rounding pushed us past midnight; redo via a half-second-bumped JD.
        let bumped = jd + 0.5 / 86400.0;
        let (y2, mo2, d2, hf2) = swe::revjul(bumped, SE_GREG_CAL);
        let ts2 = (hf2 * 3600.0).round() as i64;
        year = y2;
        month = mo2;
        day = d2;
        h = ts2 / 3600;
        m = (ts2 % 3600) / 60;
        s = ts2 % 60;
    }

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, h, m, s
    )
}

/// Convert an ISO timestamp into a Julian day.
pub fn iso2jd(iso: &str) -> Result<f64, String> {
    let s = iso.trim_end_matches('Z');
    let s = s.replace('T', " ");
    let formats = [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
    ];
    for fmt in &formats {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(&s, fmt) {
            return Ok(naive_to_jd(ndt));
        }
    }
    if let Ok(d) = NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
        let ndt = NaiveDateTime::new(d, NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        return Ok(naive_to_jd(ndt));
    }
    Err(format!("could not parse ISO date: {}", iso))
}

fn naive_to_jd(ndt: NaiveDateTime) -> f64 {
    let date = ndt.date();
    let time = ndt.time();
    let hour_frac = time.num_seconds_from_midnight() as f64 / 3600.0
        + (time.nanosecond() as f64) / 1.0e9 / 3600.0;
    swe::julday(
        date.year(),
        date.month() as i32,
        date.day() as i32,
        hour_frac,
        SE_GREG_CAL as u32,
    )
}


/// Accept either a Julian day decimal string or an ISO 8601 UTC timestamp.
pub fn parse_jd_or_iso_date(date: &str) -> Result<f64, String> {
    parse_jd_or_iso_date_in_tz(date, None)
}

/// Accept either a Julian day decimal string or an ISO 8601 timestamp.
///
/// If `tz` is provided (an IANA name like "Europe/Berlin"), the ISO string
/// is interpreted in that zone, then converted to UTC for the JD
/// calculation. JD inputs are unaffected — they're already absolute.
pub fn parse_jd_or_iso_date_in_tz(date: &str, tz: Option<&str>) -> Result<f64, String> {
    if let Ok(jd) = date.parse::<f64>() {
        return Ok(jd);
    }
    match tz {
        Some(tzname) => {
            let zone: chrono_tz::Tz = tzname.parse()
                .map_err(|_| format!("unknown timezone: {}", tzname))?;
            iso2jd_in_tz(date, zone)
        }
        None => iso2jd(date),
    }
}

fn iso2jd_in_tz(iso: &str, tz: chrono_tz::Tz) -> Result<f64, String> {
    let s = iso.trim_end_matches('Z');
    let s = s.replace('T', " ");
    let formats = [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
    ];
    for fmt in &formats {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(&s, fmt) {
            let local = match tz.from_local_datetime(&ndt) {
                chrono::LocalResult::Single(d) => d,
                chrono::LocalResult::Ambiguous(a, _) => a, // pick earliest
                chrono::LocalResult::None => {
                    return Err(format!("local time {} does not exist in {}", iso, tz.name()));
                }
            };
            let utc: DateTime<Utc> = local.with_timezone(&Utc);
            // unix_to_jd is private; replicate the calculation here:
            let secs = utc.timestamp() - 946_728_000;
            let nanos = utc.timestamp_subsec_nanos() as f64 / 1.0e9;
            return Ok(2451545.0 + (secs as f64 + nanos) / 86400.0);
        }
    }
    if let Ok(d) = NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
        let ndt = NaiveDateTime::new(d, NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        let local = tz.from_local_datetime(&ndt).single()
            .ok_or_else(|| format!("ambiguous local date {} in {}", iso, tz.name()))?;
        let utc: DateTime<Utc> = local.with_timezone(&Utc);
        let secs = utc.timestamp() - 946_728_000;
        return Ok(2451545.0 + secs as f64 / 86400.0);
    }
    Err(format!("could not parse ISO date: {}", iso))
}

/// Distance between two angles a and b in modulo-360 sense (always 0..=180).
pub fn mod360_distance(a: f64, b: f64) -> f64 {
    let mut a = a.rem_euclid(360.0);
    let mut b = b.rem_euclid(360.0);
    if a < b {
        std::mem::swap(&mut a, &mut b);
    }
    (a - b).min(b - a + 360.0)
}

/// Convert a fractional day count into (days, hours, minutes, seconds).
pub fn days_frac_to_dhms(days_frac: f64) -> (i64, i64, i64, i64) {
    let days = days_frac.floor() as i64;
    let hms_frac = days_frac - days as f64;
    let hours = (hms_frac * 24.0).floor() as i64;
    let minutes_frac = hms_frac - hours as f64 / 24.0;
    let minutes = (minutes_frac * 1440.0).floor() as i64;
    let seconds = ((minutes_frac - minutes as f64 / 1440.0) * 86400.0).floor() as i64;
    (days, hours, minutes, seconds)
}

/// Render a delta-days value for human display.
pub fn render_delta_days(delta_days: f64) -> String {
    let (days, hours, minutes, _) = days_frac_to_dhms(delta_days);
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{} days", days));
    }
    if hours > 0 {
        parts.push(format!("{} hours", hours));
    }
    if days == 0 && minutes > 0 {
        parts.push(format!("{} minutes", minutes));
    }
    if days == 0 && hours == 0 && minutes == 0 {
        return "less than a minute".to_string();
    }
    parts.join(" ")
}

pub fn aspect_name_to_angle(name: &str) -> Option<f64> {
    if name == "conjunction" {
        return Some(0.0);
    }
    if name == "opposition" {
        return Some(180.0);
    }
    ASPECTS
        .iter()
        .find(|a| a.name == name && a.mode == Some("dexter"))
        .map(|a| a.angle)
}

pub fn angle_to_aspect_name(angle: f64) -> Vec<&'static str> {
    ASPECTS
        .iter()
        .filter(|a| (a.angle - angle).abs() < 1e-9)
        .map(|a| a.name)
        .collect()
}
