//! The properties of a copied item that a search can use: defences, weapon damage, sockets.
//!
//! They are `<name>: <value>` lines, as in `Armour: 18 (augmented)`; ` (augmented)` follows
//! a value that a mod or the quality changed. `Elemental Damage:` lists one range per
//! element, separated by commas. The game leaves a space after the sockets.
//!
//! The trade site compares armour, evasion, energy shield, ward and physical damage as they
//! would be at 20% quality, whatever quality the item has below that: an item of lower quality
//! is worth what it will be once its quality is raised, which costs next to nothing. The
//! `_q20` values are those. See [`at_q20`] for the rule.

use crate::item::Item;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Properties {
    pub armour: Option<u32>,
    pub evasion: Option<u32>,
    pub energy_shield: Option<u32>,
    pub ward: Option<u32>,
    /// The chance to block, in percent.
    pub block: Option<u32>,
    pub physical: Option<(f64, f64)>,
    /// One range per element, in the order written.
    pub elemental: Vec<(f64, f64)>,
    pub chaos: Option<(f64, f64)>,
    pub attacks_per_second: Option<f64>,
    /// A weapon's critical strike chance, in percent.
    pub critical_chance: Option<f64>,
    /// The sockets as written, without the space the game leaves after them: `R-B A A`.
    pub sockets: Option<String>,
    /// The size of the largest group of linked sockets; 1 when none is linked.
    pub links: Option<u32>,
    /// The quality, in percent; 0 when the item has none.
    pub quality: u32,
    /// A mod of the item says `Quality does not increase Defences`.
    pub quality_spares_defences: bool,
    /// A mod of the item says `Quality does not increase Physical Damage`.
    pub quality_spares_physical: bool,
    /// A map's tier: its `Map Tier:` line, or its base, `Map (Tier 16)`.
    pub map_tier: Option<u32>,
    /// What a map adds, in percent: `Atlas Drops`, `Item Rarity`, `Monster Pack Size`,
    /// `Item Quantity`.
    pub atlas_drops: Option<u32>,
    pub item_rarity: Option<u32>,
    pub pack_size: Option<u32>,
    pub item_quantity: Option<u32>,
    /// What else a map drops more of, in percent: `More Maps`, `More Scarabs`, `More Currency`,
    /// `More Divination Cards`.
    pub more_maps: Option<u32>,
    pub more_scarabs: Option<u32>,
    pub more_currency: Option<u32>,
    pub more_cards: Option<u32>,
}

/// The quality the trade site compares items at.
pub const TRADE_QUALITY: u32 = 20;

/// What a value the quality raises would be at 20% quality. The quality is a multiplier of its
/// own: the value is taken back to no quality and raised to 20%. An item above 20% keeps the
/// quality it has, so its value is left as it is.
pub fn at_q20(value: f64, quality: u32) -> f64 {
    value * f64::from(100 + quality.max(TRADE_QUALITY)) / f64::from(100 + quality)
}

impl Properties {
    /// The defences are compared at a quality the item does not have: it is under 20%, and no
    /// mod says the quality leaves them alone.
    pub fn defences_raised(&self) -> bool {
        self.quality < TRADE_QUALITY && !self.quality_spares_defences
    }

    /// The same for the physical damage.
    pub fn physical_raised(&self) -> bool {
        self.quality < TRADE_QUALITY && !self.quality_spares_physical
    }

    /// A defence as it would be at 20% quality, to the nearest whole number.
    fn defence_q20(&self, value: Option<u32>) -> Option<u32> {
        let value = value?;
        Some(if self.defences_raised() { at_q20(f64::from(value), self.quality).round() as u32 } else { value })
    }

    pub fn armour_q20(&self) -> Option<u32> {
        self.defence_q20(self.armour)
    }

    pub fn evasion_q20(&self) -> Option<u32> {
        self.defence_q20(self.evasion)
    }

    pub fn energy_shield_q20(&self) -> Option<u32> {
        self.defence_q20(self.energy_shield)
    }

    pub fn ward_q20(&self) -> Option<u32> {
        self.defence_q20(self.ward)
    }

    /// Physical damage per second as it would be at 20% quality.
    pub fn pdps_q20(&self) -> Option<f64> {
        let pdps = self.pdps()?;
        Some(if self.physical_raised() { at_q20(pdps, self.quality) } else { pdps })
    }

    /// Damage per second with the physical part as it would be at 20% quality: the quality does
    /// nothing for the elemental and the chaos damage.
    pub fn dps_q20(&self) -> Option<f64> {
        let dps = self.dps()?;
        Some(dps - self.pdps().unwrap_or(0.0) + self.pdps_q20().unwrap_or(0.0))
    }

    /// Physical damage per second.
    pub fn pdps(&self) -> Option<f64> {
        Some(mean(self.physical?) * self.attacks_per_second?)
    }

    /// Elemental damage per second, every element together.
    pub fn edps(&self) -> Option<f64> {
        if self.elemental.is_empty() {
            return None;
        }
        let damage: f64 = self.elemental.iter().copied().map(mean).sum();
        Some(damage * self.attacks_per_second?)
    }

    /// Damage per second: physical, elemental and chaos together.
    pub fn dps(&self) -> Option<f64> {
        let ranges = || {
            self.physical
                .iter()
                .chain(&self.elemental)
                .chain(&self.chaos)
                .copied()
        };
        ranges().next()?;
        let damage: f64 = ranges().map(mean).sum();
        Some(damage * self.attacks_per_second?)
    }
}

pub fn parse_properties(item: &Item) -> Properties {
    let mut properties = Properties::default();
    let lines = item.sections.iter().skip(1).flatten();
    properties.quality_spares_defences = lines.clone().any(|line| line.contains(SPARES_DEFENCES));
    properties.quality_spares_physical = lines.clone().any(|line| line.contains(SPARES_PHYSICAL));
    for (name, value) in lines.filter_map(|line| line.trim_end().split_once(": ")) {
        match name {
            // "Quality: +12% (augmented)". Another quality (`Quality (Attack Modifiers)`) has
            // another name and is not this one.
            "Quality" if properties.quality == 0 => properties.quality = whole(value).unwrap_or(0),
            "Armour" => keep_first(&mut properties.armour, whole(value)),
            "Evasion Rating" => keep_first(&mut properties.evasion, whole(value)),
            "Energy Shield" => keep_first(&mut properties.energy_shield, whole(value)),
            "Ward" => keep_first(&mut properties.ward, whole(value)),
            "Chance to Block" => keep_first(&mut properties.block, whole(value)),
            "Physical Damage" => keep_first(&mut properties.physical, damage(value).next()),
            "Elemental Damage" if properties.elemental.is_empty() => {
                properties.elemental = damage(value).collect();
            }
            "Chaos Damage" => keep_first(&mut properties.chaos, damage(value).next()),
            "Attacks per Second" => {
                let number = first_word(value).parse().ok();
                keep_first(&mut properties.attacks_per_second, number);
            }
            "Critical Strike Chance" => {
                let number = first_word(value).trim_end_matches('%').parse().ok();
                keep_first(&mut properties.critical_chance, number);
            }
            "Map Tier" => keep_first(&mut properties.map_tier, whole(value)),
            "Atlas Drops" => keep_first(&mut properties.atlas_drops, whole(value)),
            "Item Rarity" => keep_first(&mut properties.item_rarity, whole(value)),
            "Monster Pack Size" => keep_first(&mut properties.pack_size, whole(value)),
            "Item Quantity" => keep_first(&mut properties.item_quantity, whole(value)),
            "More Maps" => keep_first(&mut properties.more_maps, whole(value)),
            "More Scarabs" => keep_first(&mut properties.more_scarabs, whole(value)),
            "More Currency" => keep_first(&mut properties.more_currency, whole(value)),
            "More Divination Cards" => keep_first(&mut properties.more_cards, whole(value)),
            "Sockets" if properties.sockets.is_none() => {
                let sockets = value.trim();
                properties.sockets = Some(sockets.to_string());
                properties.links = sockets.split_whitespace().map(group_size).max();
            }
            _ => {}
        }
    }
    if properties.map_tier.is_none() {
        properties.map_tier = item.base.split_once("(Tier ").and_then(|(_, rest)| rest.trim_end_matches(')').parse().ok());
    }
    properties
}

/// The mods that take a number away from the quality.
const SPARES_DEFENCES: &str = "Quality does not increase Defences";
const SPARES_PHYSICAL: &str = "Quality does not increase Physical Damage";

fn keep_first<T>(slot: &mut Option<T>, value: Option<T>) {
    if slot.is_none() {
        *slot = value;
    }
}

fn mean((low, high): (f64, f64)) -> f64 {
    (low + high) / 2.0
}

fn first_word(value: &str) -> &str {
    value.split_whitespace().next().unwrap_or("")
}

/// The number of `18`, `18 (augmented)` or `25%`.
fn whole(value: &str) -> Option<u32> {
    first_word(value).trim_end_matches('%').parse().ok()
}

/// The ranges of `8-15` or `10-20 (augmented), 30-50 (augmented)`.
fn damage(value: &str) -> impl Iterator<Item = (f64, f64)> + '_ {
    value.split(',').filter_map(|range| {
        let (low, high) = first_word(range).split_once('-')?;
        Some((low.parse().ok()?, high.parse().ok()?))
    })
}

/// The number of sockets in one group, as in `R-B-B-R`.
fn group_size(group: &str) -> u32 {
    group.split('-').count() as u32
}
