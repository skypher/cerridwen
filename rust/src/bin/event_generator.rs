// SPDX-License-Identifier: MIT AND AGPL-3.0-only

use cerridwen::planets::{Body, Jupiter, Mars, Mercury, Moon, Planet, Saturn, Sun, Venus};
use cerridwen::utils::jd2iso;
use cerridwen::ASPECTS;
use clap::Parser;
use rusqlite::Connection;

#[derive(Parser, Debug)]
#[command(
    name = "cerridwen-event-generator",
    about = "Generate aspects/ingresses/retrogrades into a sqlite events table"
)]
struct Args {
    /// Julian-day start of the period to generate.
    #[arg(long)]
    jd_start: f64,
    /// Julian-day end of the period.
    #[arg(long)]
    jd_end: f64,
    /// Path to the sqlite database to write into.
    #[arg(long, default_value = "events.db")]
    db: String,
}

fn main() -> rusqlite::Result<()> {
    let args = Args::parse();

    let conn = Connection::open(&args.db)?;
    conn.execute("DROP TABLE IF EXISTS events", [])?;
    conn.execute(
        "CREATE TABLE events (jd REAL, type TEXT, subtype TEXT, planet TEXT, data TEXT)",
        [],
    )?;

    // Match Python: the same fixed roster.
    let planets: Vec<(&str, Planet)> = vec![
        ("Moon", Moon::new().0),
        ("Sun", Sun::new().0),
        ("Mercury", Mercury::new().0),
        ("Venus", Venus::new().0),
        ("Mars", Mars::new().0),
        ("Jupiter", Jupiter::new().0),
        ("Saturn", Saturn::new().0),
    ];

    let span = args.jd_end - args.jd_start;
    let mut flush_counter: i64 = 0;

    let mut pump = |event_function: &mut dyn FnMut(
        f64,
    ) -> Option<(
        f64,
        String,
        String,
        String,
        String,
    )>|
     -> rusqlite::Result<()> {
        let mut jd = args.jd_start;
        while jd < args.jd_end {
            let event = event_function(jd);
            let (event_jd, event_type, event_subtype, event_planet, event_data) = match event {
                Some(e) => e,
                None => {
                    // Python skipped a period when no aspect found between Mercury/Venus.
                    jd += 365.0 * 2.4;
                    continue;
                }
            };
            assert!(event_jd >= jd);
            let pct = (jd - args.jd_start) / span * 100.0;
            println!(
                "{:.2}% {} {} {} {} {} {}",
                pct,
                event_jd,
                jd2iso(event_jd),
                event_type,
                event_subtype,
                event_planet,
                event_data
            );
            conn.execute(
                "INSERT INTO events VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    event_jd,
                    event_type,
                    event_subtype,
                    event_planet,
                    event_data
                ],
            )?;
            jd = event_jd + 1.0;
            flush_counter += 1;
            if flush_counter % 100 == 0 {
                // rusqlite has no implicit transaction here; queries are auto-committed.
            }
        }
        Ok(())
    };

    // ---- aspects -----
    for (_pn1, p1) in &planets {
        for (_pn2, p2) in &planets {
            if p2.max_speed() < p1.max_speed() {
                for aspect in ASPECTS.iter() {
                    if !p1.aspect_possible(p2, aspect.angle) {
                        continue;
                    }
                    let p1_owned = p1.clone();
                    let p2_owned = p2.clone();
                    let aspect_angle = aspect.angle;
                    let aspect_name = aspect.name.to_string();
                    let aspect_mode = aspect.mode.unwrap_or("").to_string();
                    let mut event_fn =
                        move |jd: f64| -> Option<(f64, String, String, String, String)> {
                            let res = p1_owned.next_angle_to_planet(
                                &p2_owned,
                                aspect_angle,
                                Some(jd),
                                None,
                                None,
                                None,
                                None,
                            );
                            res.map(|(event_jd, _, _)| {
                                (
                                    event_jd,
                                    aspect_name.clone(),
                                    aspect_mode.clone(),
                                    p1_owned.name(),
                                    p2_owned.name(),
                                )
                            })
                        };
                    pump(&mut event_fn)?;
                }
            }
        }
    }

    // ---- ingresses -----
    for (_, p) in &planets {
        let p_owned = p.clone();
        let mut event_fn = move |jd: f64| -> Option<(f64, String, String, String, String)> {
            let event_jd = p_owned.next_sign_change(Some(jd));
            Some((
                event_jd,
                "ingress".into(),
                String::new(),
                p_owned.name(),
                p_owned.sign(Some(event_jd)).into(),
            ))
        };
        pump(&mut event_fn)?;
    }

    // ---- retrogrades -----
    for (name, p) in &planets {
        if *name == "Moon" || *name == "Sun" {
            continue;
        }
        let p_owned = p.clone();
        let mut event_fn = move |jd: f64| -> Option<(f64, String, String, String, String)> {
            let (event_jd, kind) = p_owned.next_rx_event(Some(jd), None)?;
            Some((
                event_jd,
                kind.into(),
                String::new(),
                p_owned.name(),
                p_owned.sign(Some(event_jd)).into(),
            ))
        };
        pump(&mut event_fn)?;
    }

    Ok(())
}
