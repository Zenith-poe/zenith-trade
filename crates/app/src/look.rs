//! How the panel looks: one of six colour themes and one of four ways to mark a
//! searched mod. The choice is kept in the settings file and read by the painter on every frame,
//! so a change shows at once.

use std::sync::atomic::{AtomicU8, Ordering::Relaxed};

use eframe::egui::Color32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Forge,
    Midnight,
    Grove,
    Frost,
    Eldritch,
    Gilded,
}

pub const THEMES: [Theme; 6] = [Theme::Forge, Theme::Midnight, Theme::Grove, Theme::Frost, Theme::Eldritch, Theme::Gilded];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Bar,
    Band,
    Badge,
    Plain,
}

pub const MARKERS: [Marker; 4] = [Marker::Bar, Marker::Band, Marker::Badge, Marker::Plain];

impl Theme {
    /// The word of the settings file, lower case.
    pub fn key(self) -> &'static str {
        match self {
            Theme::Forge => "forge",
            Theme::Midnight => "midnight",
            Theme::Grove => "grove",
            Theme::Frost => "frost",
            Theme::Eldritch => "eldritch",
            Theme::Gilded => "gilded",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Theme::Forge => "Forge",
            Theme::Midnight => "Midnight",
            Theme::Grove => "Grove",
            Theme::Frost => "Frost",
            Theme::Eldritch => "Eldritch",
            Theme::Gilded => "Gilded",
        }
    }

    pub fn from_key(word: &str) -> Option<Self> {
        THEMES.into_iter().find(|theme| theme.key() == word)
    }

    pub fn palette(self) -> Palette {
        let p = |night: u32, plate: u32, raise: u32, hair: u32, line: u32, star: u32, gold: u32, gold_hi: u32, mute: u32, ember: u32| Palette {
            night: rgb(night),
            plate: rgb(plate),
            raise: rgb(raise),
            hair: rgb(hair),
            line: rgb(line),
            star: rgb(star),
            gold: rgb(gold),
            gold_hi: rgb(gold_hi),
            mute: rgb(mute),
            ember: rgb(ember),
        };
        match self {
            Theme::Forge => p(0x1F1B18, 0x2E2925, 0x3A342F, 0x473F38, 0x6A5D52, 0xF0E8DC, 0xE08E4F, 0xF5AC70, 0xA39585, 0xEF5D6B),
            Theme::Midnight => p(0x0F1526, 0x18213A, 0x212C4C, 0x2B3657, 0x3F4D78, 0xE9E5D6, 0xC9AA5E, 0xE3C87E, 0x8591B2, 0xDD6E54),
            Theme::Grove => p(0x0E211D, 0x16302B, 0x1F3F38, 0x2A4B43, 0x3F6B60, 0xE8EBDD, 0xD2B25C, 0xEBCB78, 0x86A69A, 0xE67A5E),
            Theme::Frost => p(0x17222E, 0x22303F, 0x2C3D50, 0x374B61, 0x52708C, 0xEAF1F5, 0x8FD3E8, 0xB9E8F5, 0x8EA2B5, 0xF0806A),
            Theme::Eldritch => p(0x181226, 0x241C38, 0x30264A, 0x3D3159, 0x5B4A82, 0xEDE6F2, 0xE3B7A0, 0xF3CDB8, 0x9A8DB5, 0xF0647A),
            Theme::Gilded => p(0x1D1913, 0x2D271F, 0x3A3227, 0x4A4032, 0x6E5F47, 0xF2EAD8, 0xD9B45A, 0xF0CF79, 0xA89B82, 0xE2674F),
        }
    }
}

impl Marker {
    pub fn key(self) -> &'static str {
        match self {
            Marker::Bar => "bar",
            Marker::Band => "band",
            Marker::Badge => "badge",
            Marker::Plain => "plain",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Marker::Bar => "Edge bar",
            Marker::Band => "Band",
            Marker::Badge => "Tier badge",
            Marker::Plain => "Plain",
        }
    }

    pub fn from_key(word: &str) -> Option<Self> {
        MARKERS.into_iter().find(|marker| marker.key() == word)
    }
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub night: Color32,
    pub plate: Color32,
    pub raise: Color32,
    pub hair: Color32,
    pub line: Color32,
    pub star: Color32,
    pub gold: Color32,
    pub gold_hi: Color32,
    pub mute: Color32,
    pub ember: Color32,
}

static THEME: AtomicU8 = AtomicU8::new(0);
static MARKER: AtomicU8 = AtomicU8::new(0);

/// Makes `theme` and `marker` the look of the panel, from the next frame on.
pub fn set(theme: Theme, marker: Marker) {
    THEME.store(THEMES.iter().position(|t| *t == theme).unwrap_or(0) as u8, Relaxed);
    MARKER.store(MARKERS.iter().position(|m| *m == marker).unwrap_or(0) as u8, Relaxed);
}

pub fn theme() -> Theme {
    THEMES.get(THEME.load(Relaxed) as usize).copied().unwrap_or(Theme::Forge)
}

pub fn marker() -> Marker {
    MARKERS.get(MARKER.load(Relaxed) as usize).copied().unwrap_or(Marker::Bar)
}

pub fn palette() -> Palette {
    theme().palette()
}
