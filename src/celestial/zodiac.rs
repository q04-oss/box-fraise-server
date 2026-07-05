// Hellenistic zodiac.
//
// The twelve signs (τὰ δώδεκα ζῴδια), each 30° of ecliptic longitude
// starting from the vernal equinox (0° Aries). Metadata carries the
// four classical elements, three modalities, and traditional (pre-
// modern) planetary rulers.
//
// Traditional rulership assignments follow Ptolemy's Tetrabiblos
// (Book I, chapter 17): Sun rules Leo; Moon rules Cancer; the
// remaining ten signs share the five naked-eye planets in pairs,
// arranged symmetrically around the Sun/Moon axis.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Element {
    Fire,
    Earth,
    Air,
    Water,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Cardinal,
    Fixed,
    Mutable,
}

/// Traditional (Hellenistic) planetary ruler. Names given both in
/// classical Greek transliteration and the modern astronomical body.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Ruler {
    pub greek: &'static str,  // e.g. "Zeus"
    pub modern: &'static str, // e.g. "Jupiter"
}

/// Full metadata for a zodiac sign. Field names sequenced to match
/// the way a Hellenistic astrologer would describe a placement.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct SignInfo {
    pub id: &'static str,        // snake_case canonical id
    pub name: &'static str,      // English / Latin name
    pub greek: &'static str,     // Polytonic Greek
    pub romanized: &'static str, // ASCII Greek transliteration
    pub meaning: &'static str,   // e.g. "Crab"
    pub symbol: &'static str,    // Unicode glyph
    pub element: Element,
    pub modality: Modality,
    pub traditional_ruler: Ruler,
    /// Starting ecliptic longitude in degrees (0, 30, 60, ..., 330).
    pub start_longitude: f64,
}

/// Zodiac sign, indexed 0..12 in the traditional order (starting from
/// Aries at the vernal equinox).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sign(pub u8);

impl Sign {
    pub fn from_longitude(longitude_deg: f64) -> Self {
        let normalised = normalize_360(longitude_deg);
        let idx = (normalised / 30.0).floor() as u8;
        Self(idx.min(11))
    }

    pub fn info(&self) -> &'static SignInfo {
        &SIGNS[self.0 as usize]
    }

    pub fn next(&self) -> Sign {
        Sign((self.0 + 1) % 12)
    }

    /// Longitude of the boundary AFTER this sign (i.e. the start of
    /// the next sign). Used to compute the "next ingress" moment.
    pub fn next_boundary_longitude(&self) -> f64 {
        ((self.0 as f64) + 1.0) * 30.0
    }
}

fn normalize_360(deg: f64) -> f64 {
    let m = deg.rem_euclid(360.0);
    if m < 0.0 {
        m + 360.0
    } else {
        m
    }
}

// ── Sign table ─────────────────────────────────────────────────────────
//
// Order: Aries → Pisces. Elemental pattern (fire, earth, air, water)
// repeats every four signs. Modalities cycle (cardinal, fixed, mutable)
// every three.

const HELIOS: Ruler = Ruler {
    greek: "Helios",
    modern: "Sun",
};
const SELENE: Ruler = Ruler {
    greek: "Selene",
    modern: "Moon",
};
const HERMES: Ruler = Ruler {
    greek: "Hermes",
    modern: "Mercury",
};
const APHRODITE: Ruler = Ruler {
    greek: "Aphrodite",
    modern: "Venus",
};
const ARES: Ruler = Ruler {
    greek: "Ares",
    modern: "Mars",
};
const ZEUS: Ruler = Ruler {
    greek: "Zeus",
    modern: "Jupiter",
};
const KRONOS: Ruler = Ruler {
    greek: "Kronos",
    modern: "Saturn",
};

pub const SIGNS: [SignInfo; 12] = [
    SignInfo {
        id: "aries",
        name: "Aries",
        greek: "Κριός",
        romanized: "Krios",
        meaning: "Ram",
        symbol: "♈",
        element: Element::Fire,
        modality: Modality::Cardinal,
        traditional_ruler: ARES,
        start_longitude: 0.0,
    },
    SignInfo {
        id: "taurus",
        name: "Taurus",
        greek: "Ταῦρος",
        romanized: "Tauros",
        meaning: "Bull",
        symbol: "♉",
        element: Element::Earth,
        modality: Modality::Fixed,
        traditional_ruler: APHRODITE,
        start_longitude: 30.0,
    },
    SignInfo {
        id: "gemini",
        name: "Gemini",
        greek: "Δίδυμοι",
        romanized: "Didymoi",
        meaning: "Twins",
        symbol: "♊",
        element: Element::Air,
        modality: Modality::Mutable,
        traditional_ruler: HERMES,
        start_longitude: 60.0,
    },
    SignInfo {
        id: "cancer",
        name: "Cancer",
        greek: "Καρκίνος",
        romanized: "Karkinos",
        meaning: "Crab",
        symbol: "♋",
        element: Element::Water,
        modality: Modality::Cardinal,
        traditional_ruler: SELENE,
        start_longitude: 90.0,
    },
    SignInfo {
        id: "leo",
        name: "Leo",
        greek: "Λέων",
        romanized: "Leon",
        meaning: "Lion",
        symbol: "♌",
        element: Element::Fire,
        modality: Modality::Fixed,
        traditional_ruler: HELIOS,
        start_longitude: 120.0,
    },
    SignInfo {
        id: "virgo",
        name: "Virgo",
        greek: "Παρθένος",
        romanized: "Parthenos",
        meaning: "Maiden",
        symbol: "♍",
        element: Element::Earth,
        modality: Modality::Mutable,
        traditional_ruler: HERMES,
        start_longitude: 150.0,
    },
    SignInfo {
        id: "libra",
        name: "Libra",
        greek: "Ζυγός",
        romanized: "Zygos",
        meaning: "Scales",
        symbol: "♎",
        element: Element::Air,
        modality: Modality::Cardinal,
        traditional_ruler: APHRODITE,
        start_longitude: 180.0,
    },
    SignInfo {
        id: "scorpio",
        name: "Scorpio",
        greek: "Σκορπιός",
        romanized: "Skorpios",
        meaning: "Scorpion",
        symbol: "♏",
        element: Element::Water,
        modality: Modality::Fixed,
        traditional_ruler: ARES,
        start_longitude: 210.0,
    },
    SignInfo {
        id: "sagittarius",
        name: "Sagittarius",
        greek: "Τοξότης",
        romanized: "Toxotes",
        meaning: "Archer",
        symbol: "♐",
        element: Element::Fire,
        modality: Modality::Mutable,
        traditional_ruler: ZEUS,
        start_longitude: 240.0,
    },
    SignInfo {
        id: "capricorn",
        name: "Capricorn",
        greek: "Αἰγόκερως",
        romanized: "Aigokeros",
        meaning: "Goat-horned",
        symbol: "♑",
        element: Element::Earth,
        modality: Modality::Cardinal,
        traditional_ruler: KRONOS,
        start_longitude: 270.0,
    },
    SignInfo {
        id: "aquarius",
        name: "Aquarius",
        greek: "Ὑδροχόος",
        romanized: "Hydrochoos",
        meaning: "Water-bearer",
        symbol: "♒",
        element: Element::Air,
        modality: Modality::Fixed,
        traditional_ruler: KRONOS,
        start_longitude: 300.0,
    },
    SignInfo {
        id: "pisces",
        name: "Pisces",
        greek: "Ἰχθύες",
        romanized: "Ichthyes",
        meaning: "Fishes",
        symbol: "♓",
        element: Element::Water,
        modality: Modality::Mutable,
        traditional_ruler: ZEUS,
        start_longitude: 330.0,
    },
];
