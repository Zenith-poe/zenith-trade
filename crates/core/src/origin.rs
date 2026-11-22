//! Where a mod comes from, when the item's text says so reliably: an influence (an explicit mod
//! only influenced gear can roll is known by its name), an Eldritch implicit, the synthesis, the
//! Betrayal veil, an essence, the Delve.

use crate::mods::{Mod, ModKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Shaper,
    Elder,
    Crusader,
    Redeemer,
    Hunter,
    Warlord,
    /// A Searing Exarch implicit.
    Exarch,
    /// An Eater of Worlds implicit.
    Eater,
    Synthesised,
    /// A veiled mod, not yet unveiled.
    Veiled,
    /// A mod the Betrayal unveiling gave.
    Unveiled,
    /// A mod only an essence gives.
    Essence,
    /// A mod of the Delve.
    Delve,
}

/// The names of the mods that only influenced gear rolls, as the game data names them: each
/// influence has one prefix and one suffix name, shared by every item class.
const INFLUENCED: [(&str, Origin); 12] = [
    ("The Shaper's", Origin::Shaper),
    ("of Shaping", Origin::Shaper),
    ("The Elder's", Origin::Elder),
    ("of the Elder", Origin::Elder),
    ("Crusader's", Origin::Crusader),
    ("of the Crusade", Origin::Crusader),
    ("Redeemer's", Origin::Redeemer),
    ("of Redemption", Origin::Redeemer),
    ("Hunter's", Origin::Hunter),
    ("of the Hunt", Origin::Hunter),
    ("Warlord's", Origin::Warlord),
    ("of the Conquest", Origin::Warlord),
];

/// The names the Betrayal unveiling gives, none of which is the name of a mod found any other way.
const UNVEILED: [&str; 23] = [
    "Brinerot", "Catarina's", "Chosen", "Elreon's", "Gravicius'", "Haku's", "It's", "Jorgin's", "Korell's", "Leo's", "Rin's", "Tora's", "Vagan's",
    "Vorici's", "Zorath's", "of Aisling", "of Cameria", "of Catarina", "of Hillock", "of Janus", "of Riker", "of Zorath", "of the Order",
];

impl Origin {
    pub fn label(self) -> &'static str {
        match self {
            Self::Shaper => "Shaper",
            Self::Elder => "Elder",
            Self::Crusader => "Crusader",
            Self::Redeemer => "Redeemer",
            Self::Hunter => "Hunter",
            Self::Warlord => "Warlord",
            Self::Exarch => "Searing Exarch",
            Self::Eater => "Eater of Worlds",
            Self::Synthesised => "Synthesised",
            Self::Veiled => "Veiled",
            Self::Unveiled => "Unveiled",
            Self::Essence => "Essence",
            Self::Delve => "Delve",
        }
    }
}

impl Mod {
    /// What makes the mod special, if the text says. A crafted mod is the bench's, whatever its name.
    pub fn origin(&self) -> Option<Origin> {
        if let ModKind::Other(wording) = &self.kind {
            let wording = wording.to_ascii_lowercase();
            if wording.contains("searing exarch") {
                return Some(Origin::Exarch);
            }
            if wording.contains("eater of worlds") {
                return Some(Origin::Eater);
            }
            if wording.contains("synthesised") {
                return Some(Origin::Synthesised);
            }
            if wording.starts_with("veiled") {
                return Some(Origin::Veiled);
            }
        }
        if self.crafted || !matches!(self.kind, ModKind::Prefix | ModKind::Suffix) {
            return None;
        }
        let name = self.name.as_deref()?;
        if let Some((_, origin)) = INFLUENCED.iter().find(|(influenced, _)| *influenced == name) {
            return Some(*origin);
        }
        match name {
            "Essences" | "of the Essence" => Some(Origin::Essence),
            "of the Underground" | "Subterranean" => Some(Origin::Delve),
            "Veiled" => Some(Origin::Veiled),
            _ if UNVEILED.contains(&name) => Some(Origin::Unveiled),
            _ => None,
        }
    }
}
