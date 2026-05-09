use crate::defs::{DEBUG_EVENT_APPROXIMATION, MAXIMUM_ERROR, MAX_DATA_POINTS};

/// Recursive sample-and-refine event finder.
///
/// * `eval` re-evaluates the underlying scalar function at any JD (e.g. angle
///   between two planets, or planet speed). Used during refinement.
/// * `find_matches` is called on a uniform JD grid and returns candidate matches
///   as `(jd, value)` pairs.
/// * `match_filter` is applied to a candidate's value when the recursion ends
///   without further refinement; returning false discards the candidate.
/// * `distance_function` decides whether the value near a candidate has
///   stabilised; once below `MAXIMUM_ERROR` we stop recursing.
#[allow(clippy::too_many_arguments)]
pub fn approximate_event_date<E, F, M, D>(
    jd_start: f64,
    jd_end: f64,
    eval: &mut E,
    find_matches: &mut F,
    match_filter: &M,
    distance_function: &D,
    sample_interval: f64,
    passes: u32,
) -> Option<Vec<(f64, f64)>>
where
    E: FnMut(f64) -> f64,
    F: FnMut(&[f64], &mut E) -> Option<Vec<(f64, f64)>>,
    M: Fn(f64) -> bool,
    D: Fn(f64, f64) -> f64,
{
    let span = (jd_end - jd_start).abs();
    let num_points = (span / sample_interval) as usize;
    if num_points > MAX_DATA_POINTS {
        if DEBUG_EVENT_APPROXIMATION {
            eprintln!(
                "data point maximum ({}) exceeded (have {}), aborting pass.",
                MAX_DATA_POINTS, num_points
            );
        }
        return None;
    }

    let mut jds = Vec::with_capacity(num_points + 1);
    let mut t = jd_start;
    while t < jd_end {
        jds.push(t);
        t += sample_interval;
    }
    if jds.is_empty() {
        return None;
    }

    let matches = find_matches(&jds, eval)?;

    let mut refined: Vec<(f64, f64)> = Vec::new();
    for (jd, value) in matches {
        let fuzz = sample_interval * 2.0;
        let distance = distance_function(eval(jd - fuzz), eval(jd + fuzz));
        let precision_reached = distance < MAXIMUM_ERROR;

        if passes > 0 && !precision_reached {
            let new_interval = sample_interval / 100.0;
            let extra_fuzz = fuzz + new_interval * 100.0;
            let result = approximate_event_date(
                jd - extra_fuzz,
                jd + extra_fuzz,
                eval,
                find_matches,
                match_filter,
                distance_function,
                new_interval,
                passes - 1,
            );
            match result {
                Some(inner) if !inner.is_empty() => {
                    refined.extend(inner);
                }
                _ => {
                    if !match_filter(value) {
                        continue;
                    }
                    refined.push((jd, value));
                }
            }
        } else {
            if !match_filter(value) {
                continue;
            }
            refined.push((jd, value));
        }
    }

    Some(refined)
}
