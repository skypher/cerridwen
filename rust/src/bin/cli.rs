use cerridwen::{compute_moon_data, compute_sun_data, render_delta_days, jd2iso, VERSION};
use chrono::Local;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "cerridwen", version = VERSION,
          about = "Print sun and moon data for the current moment")]
struct Args {}

fn main() {
    let _ = Args::parse();

    let sun = compute_sun_data(None, None);
    println!("Julian day: {}", sun.jd);
    println!("Universal time (UTC): {}", sun.iso_date);
    println!("Local time: {}", Local::now().format("%a %b %e %H:%M:%S %Y"));

    let (sign, deg, min, sec) = sun.position.rel_tuple();
    println!(
        "Sun: {} / {} {} {}' {}\"",
        sun.position.absolute_degrees,
        deg,
        &sign[..3],
        min,
        sec,
    );

    let moon = compute_moon_data(None, None);
    let (sign, deg, min, sec) = moon.position.rel_tuple();
    println!(
        "Moon: {} / {} {} {}' {}\"",
        moon.position.absolute_degrees,
        deg,
        &sign[..3],
        min,
        sec,
    );

    let phase = format!("{} {}", moon.phase.trend, moon.phase.shape);
    let quarter = moon.phase.quarter_english.unwrap_or("none");
    println!(
        "phase: {}, quarter: {}, illum: {}%",
        phase,
        quarter,
        (moon.illumination * 100.0) as i64,
    );

    let next_new = &moon.next_new_moon;
    println!(
        "next new moon: {}: in {} ({} / {})",
        next_new.description,
        render_delta_days(next_new.delta_days(None)),
        jd2iso(next_new.jd),
        next_new.jd,
    );

    let next_full = &moon.next_full_moon;
    println!(
        "next full moon: {}: in {} ({} / {})",
        next_full.description,
        render_delta_days(next_full.delta_days(None)),
        jd2iso(next_full.jd),
        next_full.jd,
    );
}
