use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What a device sends every few seconds while somebody is running.
///
/// Short on purpose. This arrives over LTE-M from a battery the size of
/// a stamp, and every byte is radio time — which is why positions are
/// POSTed rather than pushed down a held-open socket. A persistent
/// connection is cheap for a phone on wifi and expensive for a modem
/// that would rather sleep between transmissions.
#[derive(Deserialize)]
pub struct At {
    pub lat: f64,
    pub lon: f64,
    /// Distance so far. Computed on the device, because it knows the
    /// positions it discarded between transmissions.
    #[serde(default)]
    pub m: i64,
    /// Beats per minute, read off whatever strap is paired. Absent when
    /// nothing is paired, which is allowed — a run without a strap is
    /// still a run.
    #[serde(default)]
    pub bpm: Option<u16>,
}

#[derive(Serialize)]
pub struct Started {
    pub id: Uuid,
}
