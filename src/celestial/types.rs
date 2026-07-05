use chrono::{DateTime, Utc};
use serde::Serialize;

use super::{calc, zodiac};

/// The compact celestial block attached to every schedule item. Enough
/// for the UI to render moon phase + zodiac placement without another
/// server call.
#[derive(Debug, Clone, Serialize)]
pub struct ItemCelestial {
    pub moon_phase: f64,
    pub moon_phase_name: &'static str,
    pub moon_illumination_pct: u8,
    pub moon_sign: &'static str,
    pub moon_sign_symbol: &'static str,
    pub sun_sign: &'static str,
    pub sun_sign_symbol: &'static str,
    pub season: &'static str,
}

impl ItemCelestial {
    pub fn compute(t: DateTime<Utc>) -> Self {
        let moon_phase = calc::moon_phase(t);
        let moon_long = calc::moon_longitude_deg(t);
        let sun_long = calc::sun_longitude_deg(t);
        let moon_sign = zodiac::Sign::from_longitude(moon_long).info();
        let sun_sign = zodiac::Sign::from_longitude(sun_long).info();
        Self {
            moon_phase,
            moon_phase_name: calc::phase_name(moon_phase),
            moon_illumination_pct: calc::moon_illumination_pct(moon_phase),
            moon_sign: moon_sign.id,
            moon_sign_symbol: moon_sign.symbol,
            sun_sign: sun_sign.id,
            sun_sign_symbol: sun_sign.symbol,
            season: calc::season_from_longitude(sun_long),
        }
    }
}

/// A pointer to the next moment a placement enters a given sign.
#[derive(Debug, Clone, Serialize)]
pub struct NextIngress {
    pub sign: &'static str,
    pub sign_symbol: &'static str,
    pub at: DateTime<Utc>,
}

/// A cardinal point of the solar year — solstice or equinox.
#[derive(Debug, Clone, Serialize)]
pub struct NextCardinalPoint {
    /// e.g. "vernal_equinox", "summer_solstice"
    pub kind: &'static str,
    pub at: DateTime<Utc>,
}

/// Full sun information: position, sign metadata, seasonal markers.
#[derive(Debug, Clone, Serialize)]
pub struct SunInfo {
    pub longitude_deg: f64,
    pub sign: zodiac::SignInfo,
    pub season: &'static str,
    pub next_ingress: NextIngress,
    pub next_cardinal_point: NextCardinalPoint,
}

/// Full moon information: position, phase, sign metadata, upcoming events.
#[derive(Debug, Clone, Serialize)]
pub struct MoonInfo {
    pub longitude_deg: f64,
    pub phase: f64,
    pub phase_name: &'static str,
    pub illumination_pct: u8,
    pub sign: zodiac::SignInfo,
    pub next_ingress: NextIngress,
    pub next_new_moon: DateTime<Utc>,
    pub next_full_moon: DateTime<Utc>,
}

/// The sky at a moment. Returned by GET /v1/sky.
#[derive(Debug, Clone, Serialize)]
pub struct Sky {
    pub at: DateTime<Utc>,
    pub sun: SunInfo,
    pub moon: MoonInfo,
}

impl Sky {
    pub fn at(t: DateTime<Utc>) -> Self {
        let sun_long = calc::sun_longitude_deg(t);
        let moon_long = calc::moon_longitude_deg(t);
        let phase = calc::moon_phase(t);

        let sun_sign = zodiac::Sign::from_longitude(sun_long);
        let moon_sign = zodiac::Sign::from_longitude(moon_long);

        let sun_next_boundary = sun_sign.next_boundary_longitude();
        let moon_next_boundary = moon_sign.next_boundary_longitude();

        let (cardinal_kind, cardinal_target) = calc::next_cardinal_point(t);

        let sun = SunInfo {
            longitude_deg: sun_long,
            sign: *sun_sign.info(),
            season: calc::season_from_longitude(sun_long),
            next_ingress: NextIngress {
                sign: sun_sign.next().info().id,
                sign_symbol: sun_sign.next().info().symbol,
                at: calc::sun_reaches_longitude(t, sun_next_boundary),
            },
            next_cardinal_point: NextCardinalPoint {
                kind: cardinal_kind,
                at: calc::sun_reaches_longitude(t, cardinal_target),
            },
        };

        let moon = MoonInfo {
            longitude_deg: moon_long,
            phase,
            phase_name: calc::phase_name(phase),
            illumination_pct: calc::moon_illumination_pct(phase),
            sign: *moon_sign.info(),
            next_ingress: NextIngress {
                sign: moon_sign.next().info().id,
                sign_symbol: moon_sign.next().info().symbol,
                at: calc::moon_reaches_longitude(t, moon_next_boundary),
            },
            next_new_moon: calc::moon_reaches_phase(t, 0.0),
            next_full_moon: calc::moon_reaches_phase(t, 0.5),
        };

        Self { at: t, sun, moon }
    }
}
