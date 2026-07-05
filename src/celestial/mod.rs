// Celestial time.
//
// Every schedule entry in Box Fraise carries celestial context: what
// phase the moon was in, what zodiac sign the sun and moon occupied,
// where in the seasonal cycle the sun sat. This isn't decoration —
// it's a first-class dimension of when a thing happened, alongside
// the wall-clock timestamp.
//
// The astronomy: direct implementations of Meeus (Astronomical
// Algorithms, 2nd ed.) simplified formulas. Accuracy is arcminutes
// for the sun and better than a degree for the moon over a century
// or two — plenty for zodiac-sign placement and seasonal markers,
// which is what a scheduling context needs.
//
// The interpretive layer (zodiac signs, elements, modalities,
// traditional rulers) follows the Hellenistic tradition documented
// by Ptolemy (Tetrabiblos) and codified through Vettius Valens and
// Firmicus Maternus. It predates the discovery of Uranus (1781),
// Neptune (1846), and Pluto (1930) — so the ruler set is exactly
// seven planets: Helios, Selene, Hermes, Aphrodite, Ares, Zeus,
// Kronos.

pub mod calc;
pub mod routes;
pub mod types;
pub mod zodiac;

pub use types::{ItemCelestial, MoonInfo, Sky, SunInfo};
