use cerridwen::{
    compute_moon_data_with, compute_sun_data, jd2iso, render_delta_days, MoonOptions, VERSION,
};
use chrono::Local;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "cerridwen", version = VERSION,
          about = "Print sun and moon data for the current moment")]
struct Args {
    /// Restrict the void-of-course calculation to the seven traditional
    /// planets (Sun..Saturn), excluding Uranus, Neptune, and Pluto.
    #[arg(long)]
    voc_traditional_only: bool,
}

fn main() {
    let args = Args::parse();

    let sun = compute_sun_data(None, None);
    println!("Julian day: {}", sun.jd);
    println!("Universal time (UTC): {}", sun.iso_date);
    println!(
        "Local time: {}",
        Local::now().format("%a %b %e %H:%M:%S %Y")
    );

    let (sign, deg, min, sec) = sun.position.rel_tuple();
    println!(
        "Sun: {} / {} {} {}' {}\"",
        sun.position.absolute_degrees,
        deg,
        &sign[..3],
        min,
        sec,
    );

    if let Some(ev) = &sun.next_event {
        println!(
            "next sun event: {} (in {})",
            ev.description,
            render_delta_days(ev.delta_days(None)),
        );
    }

    let moon = compute_moon_data_with(
        None,
        None,
        MoonOptions {
            voc_traditional_only: args.voc_traditional_only,
        },
    );
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

    println!("lunation number: {}", moon.lunation_number);

    let voc = &moon.void_of_course;
    let voc_label = if voc.traditional_only {
        " (traditional)"
    } else {
        ""
    };
    if voc.is_void {
        println!(
            "void of course{}: yes — until {} ({})",
            voc_label, voc.until_iso, voc.until_jd,
        );
    } else {
        println!(
            "void of course{}: no — VoC will start at {} ({})",
            voc_label, voc.until_iso, voc.until_jd,
        );
    }

    if let Some(ev) = &moon.next_event {
        println!(
            "next moon event: {} (in {})",
            ev.description,
            render_delta_days(ev.delta_days(None)),
        );
    }
}
