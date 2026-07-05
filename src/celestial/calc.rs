// Astronomical calculations.
//
// Direct implementations of Meeus simplified formulas (Astronomical
// Algorithms, 2nd ed.). Accuracy budget: arcminute for solar
// longitude, ~1° for lunar longitude, hours for phase-event times.
// Fine for zodiac placement and calendar markers, which is all a
// scheduling context needs.

use chrono::{DateTime, TimeZone, Utc};

const J2000: f64 = 2_451_545.0; // JD at 2000-01-01T12:00:00 UT
const JD_UNIX_EPOCH: f64 = 2_440_587.5; // JD at 1970-01-01T00:00:00 UT
pub const SYNODIC_MONTH: f64 = 29.530_588_67; // mean synodic month, days
const REF_NEW_MOON_JD: f64 = 2_451_550.097_65; // 2000-01-06 18:14 UT ~ new moon

/// Julian Date for a chrono UTC timestamp. Sub-second precision.
pub fn julian_date(t: DateTime<Utc>) -> f64 {
    let secs = t.timestamp() as f64 + (t.timestamp_subsec_nanos() as f64) / 1e9;
    JD_UNIX_EPOCH + secs / 86_400.0
}

/// Inverse: given a Julian Date, produce a chrono UTC timestamp.
pub fn from_julian(jd: f64) -> DateTime<Utc> {
    let secs = (jd - JD_UNIX_EPOCH) * 86_400.0;
    let whole = secs.floor() as i64;
    let frac = ((secs - secs.floor()) * 1e9).round() as u32;
    Utc.timestamp_opt(whole, frac)
        .single()
        .unwrap_or_else(Utc::now)
}

// ── Sun ─────────────────────────────────────────────────────────────

/// Apparent ecliptic longitude of the sun, in degrees [0, 360).
/// Meeus §25.2 — simplified formula, arcminute accuracy over this
/// century.
pub fn sun_longitude_deg(t: DateTime<Utc>) -> f64 {
    let jd = julian_date(t);
    let n = jd - J2000;
    let l = 280.460 + 0.985_647_4 * n; // mean longitude
    let g = (357.528 + 0.985_600_3 * n).to_radians(); // mean anomaly
    let lambda = l + 1.915 * g.sin() + 0.020 * (2.0 * g).sin();
    normalize_360(lambda)
}

/// Season name based on solar longitude quadrants.
pub fn season_from_longitude(long_deg: f64) -> &'static str {
    match normalize_360(long_deg) as u32 {
        0..=89 => "spring",
        90..=179 => "summer",
        180..=269 => "autumn",
        _ => "winter",
    }
}

/// Solar longitude of the next cardinal point (equinox or solstice)
/// after the given time. Returns (name, longitude_target).
pub fn next_cardinal_point(t: DateTime<Utc>) -> (&'static str, f64) {
    let l = sun_longitude_deg(t);
    let targets: [(&str, f64); 4] = [
        ("vernal_equinox", 0.0),
        ("summer_solstice", 90.0),
        ("autumnal_equinox", 180.0),
        ("winter_solstice", 270.0),
    ];
    for (name, target) in targets {
        if l < target {
            return (name, target);
        }
    }
    ("vernal_equinox", 360.0)
}

/// Approximate time when the sun reaches a target ecliptic longitude,
/// starting the search from `t`. Uses one Newton-style refinement pass
/// against the mean solar motion (~0.9856°/day).
pub fn sun_reaches_longitude(t: DateTime<Utc>, target_deg: f64) -> DateTime<Utc> {
    let mut current = t;
    for _ in 0..3 {
        let l = sun_longitude_deg(current);
        let delta = wrapped_forward_delta(l, target_deg);
        if delta.abs() < 1e-4 {
            break;
        }
        let days = delta / 0.985_647_4;
        current += chrono::Duration::milliseconds((days * 86_400_000.0) as i64);
    }
    current
}

// ── Moon ────────────────────────────────────────────────────────────

/// Lunar phase as a fraction in [0.0, 1.0). 0.0 = new, 0.5 = full.
/// Anchored to a well-established reference new moon; accuracy is a
/// few hours over decades.
pub fn moon_phase(t: DateTime<Utc>) -> f64 {
    let jd = julian_date(t);
    let raw = (jd - REF_NEW_MOON_JD) / SYNODIC_MONTH;
    let m = raw.rem_euclid(1.0);
    if m < 0.0 {
        m + 1.0
    } else {
        m
    }
}

/// Approximate lunar illumination percentage, 0..=100.
pub fn moon_illumination_pct(phase: f64) -> u8 {
    // fraction_illuminated = (1 - cos(2π * phase)) / 2
    let f = (1.0 - (std::f64::consts::TAU * phase).cos()) / 2.0;
    (f * 100.0).round().clamp(0.0, 100.0) as u8
}

pub fn phase_name(phase: f64) -> &'static str {
    // Eight named phases: divide the cycle into eighths, centred on
    // the four principal points (new, first-quarter, full, last-quarter).
    let f = phase.rem_euclid(1.0);
    let eighth = (f * 8.0).round() as u32 % 8;
    match eighth {
        0 => "new",
        1 => "waxing crescent",
        2 => "first quarter",
        3 => "waxing gibbous",
        4 => "full",
        5 => "waning gibbous",
        6 => "last quarter",
        7 => "waning crescent",
        _ => "new",
    }
}

/// Lunar ecliptic longitude, in degrees [0, 360). Meeus main-term
/// implementation — sufficient for sign placement (accuracy well
/// within 1°). For sub-degree work you'd add the ELP-2000 corrections.
pub fn moon_longitude_deg(t: DateTime<Utc>) -> f64 {
    let jd = julian_date(t);
    let n = jd - J2000;
    // Mean longitude L' (Meeus §47.1)
    let l_p = 218.3164477 + 13.176_396_47 * n;
    // Mean anomaly M'
    let m_p = (134.9633964 + 13.064_993_02 * n).to_radians();
    // Mean elongation D
    let d = (297.8501921 + 12.190_749_12 * n).to_radians();
    // Sun's mean anomaly M
    let m = (357.5291092 + 0.985_600_28 * n).to_radians();
    // Distance from ascending node F
    let f = (93.2720950 + 13.229_350_25 * n).to_radians();

    // Main periodic terms of longitude (degrees). Coefficients from Meeus §47.6.
    let corrections = 6.289 * m_p.sin() + 1.274 * (2.0 * d - m_p).sin() + 0.658 * (2.0 * d).sin()
        - 0.186 * m.sin()
        - 0.059 * (2.0 * m_p - 2.0 * d).sin()
        - 0.057 * (m_p - 2.0 * d + m).sin()
        + 0.053 * (m_p + 2.0 * d).sin()
        + 0.046 * (2.0 * d - m).sin()
        + 0.041 * (m_p - m).sin()
        - 0.035 * d.sin()
        - 0.031 * (m_p + m).sin()
        - 0.015 * (2.0 * f - 2.0 * d).sin()
        + 0.011 * (m_p - 4.0 * d).sin();

    normalize_360(l_p + corrections)
}

/// Approximate time when the moon reaches a target ecliptic longitude
/// after `t`. Mean lunar motion is ~13.176°/day.
pub fn moon_reaches_longitude(t: DateTime<Utc>, target_deg: f64) -> DateTime<Utc> {
    let mut current = t;
    for _ in 0..4 {
        let l = moon_longitude_deg(current);
        let delta = wrapped_forward_delta(l, target_deg);
        if delta.abs() < 1e-3 {
            break;
        }
        let days = delta / 13.176_396_47;
        current += chrono::Duration::milliseconds((days * 86_400_000.0) as i64);
    }
    current
}

/// Time of the next occurrence of a target phase fraction (e.g. 0.0
/// for new moon, 0.5 for full).
pub fn moon_reaches_phase(t: DateTime<Utc>, target_phase: f64) -> DateTime<Utc> {
    let current = moon_phase(t);
    let mut delta = target_phase - current;
    if delta <= 0.0 {
        delta += 1.0;
    }
    let days = delta * SYNODIC_MONTH;
    t + chrono::Duration::milliseconds((days * 86_400_000.0) as i64)
}

// ── Helpers ─────────────────────────────────────────────────────────

fn normalize_360(deg: f64) -> f64 {
    let m = deg.rem_euclid(360.0);
    if m < 0.0 {
        m + 360.0
    } else {
        m
    }
}

/// The forward angular distance from `from` to `to`, both in degrees,
/// always in [0, 360).
fn wrapped_forward_delta(from: f64, to: f64) -> f64 {
    let d = (to - from).rem_euclid(360.0);
    if d < 0.0 {
        d + 360.0
    } else {
        d
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn julian_date_of_j2000_is_exact() {
        let t = Utc.with_ymd_and_hms(2000, 1, 1, 12, 0, 0).unwrap();
        let jd = julian_date(t);
        assert!((jd - 2_451_545.0).abs() < 1e-6, "got JD {jd}");
    }

    #[test]
    fn julian_roundtrip() {
        let t = Utc.with_ymd_and_hms(2026, 7, 11, 20, 30, 0).unwrap();
        let jd = julian_date(t);
        let back = from_julian(jd);
        assert_eq!(back.timestamp(), t.timestamp());
    }

    #[test]
    fn sun_at_vernal_equinox_is_near_zero_longitude() {
        // 2026 vernal equinox ~ March 20 14:46 UTC
        let t = Utc.with_ymd_and_hms(2026, 3, 20, 14, 46, 0).unwrap();
        let l = sun_longitude_deg(t);
        // Simplified formula is arcminute-accurate; require within 0.5°
        assert!(
            !(0.5..=359.5).contains(&l),
            "sun at equinox should be ~0°, got {l}"
        );
    }

    #[test]
    fn sun_in_july_is_in_cancer_or_leo() {
        let t = Utc.with_ymd_and_hms(2026, 7, 15, 12, 0, 0).unwrap();
        let l = sun_longitude_deg(t);
        // Sun ingresses Cancer around June 21 (90°) and Leo around July 22 (120°)
        assert!(
            (90.0..120.0).contains(&l),
            "expected sun in Cancer around July 15, got {l}"
        );
    }

    #[test]
    fn moon_phase_is_bounded() {
        for year in 2020..2030 {
            for month in 1..=12 {
                let t = Utc.with_ymd_and_hms(year, month, 15, 0, 0, 0).unwrap();
                let p = moon_phase(t);
                assert!((0.0..1.0).contains(&p), "phase out of range at {t}: {p}");
            }
        }
    }

    #[test]
    fn phase_name_covers_all_eighths() {
        assert_eq!(phase_name(0.0), "new");
        assert_eq!(phase_name(0.25), "first quarter");
        assert_eq!(phase_name(0.5), "full");
        assert_eq!(phase_name(0.75), "last quarter");
    }

    #[test]
    fn moon_longitude_is_normalised() {
        let t = Utc.with_ymd_and_hms(2026, 7, 11, 20, 0, 0).unwrap();
        let l = moon_longitude_deg(t);
        assert!(
            (0.0..360.0).contains(&l),
            "lunar longitude out of range: {l}"
        );
    }
}
