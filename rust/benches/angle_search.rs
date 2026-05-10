// SPDX-License-Identifier: MIT AND AGPL-3.0-only
//
// Benchmarks for the hot path most likely to regress: the angle/event
// finder used by next_new_moon, next_full_moon, retrograde stations,
// and sign changes.

use cerridwen::planets::{Moon, Sun};
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

fn next_new_moon(c: &mut Criterion) {
    c.bench_function("Moon::next_new_moon", |b| {
        b.iter(|| {
            let m = Moon::at_jd(black_box(2456794.9541666));
            let _ = black_box(m.next_new_moon(None));
        });
    });
}

fn next_angle_to_sun(c: &mut Criterion) {
    c.bench_function("Moon::next_angle_to_planet(Sun, 0)", |b| {
        b.iter(|| {
            let m = Moon::at_jd(black_box(2456868.0));
            let s = Sun::new();
            let _ = black_box(m.next_angle_to_planet(&s.0, 0.0, None, None, None, None, None));
        });
    });
}

criterion_group!(benches, next_new_moon, next_angle_to_sun);
criterion_main!(benches);
