//! A map, or what is read like one (an Expedition logbook, a Maven invitation): its name, its
//! tier, the properties that say how it rewards, and every mod line it carries.

use crate::item::{Item, Rarity};

/// The properties a map shows, in the words the game copies them with.
const PROPERTIES: [&str; 11] = [
    "Map Tier",
    "Item Quantity",
    "Item Rarity",
    "Monster Pack Size",
    "More Maps",
    "More Scarabs",
    "More Currency",
    "Quality",
    "Area Level",
    "Monster Level",
    "Item Level",
];

/// Classes the game gives to maps and to what opens one.
const CLASSES: [&str; 5] = ["Maps", "Misc Map Items", "Map Fragments", "Expedition Logbooks", "Memories"];

/// What a map says of itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapInfo {
    pub name: String,
    pub tier: Option<u32>,
    /// Name and value, in the order the item gives them: `("Item Quantity", "+119%")`.
    pub properties: Vec<(String, String)>,
    /// Every mod line, implicits included, without the notes the game adds after a dash.
    pub mods: Vec<String>,
}

/// Whether the item is read as a map.
pub fn is_map(item: &Item) -> bool {
    CLASSES.contains(&item.class.as_str())
}

/// The tier in a name such as `Blighted Map (Tier 16)`.
fn tier_in_name(name: &str) -> Option<u32> {
    let rest = name.split("(Tier ").nth(1)?;
    rest.split(')').next()?.trim().parse().ok()
}

/// A mod line without its note (`— Unscalable Value`) and its `(implicit)` mark.
fn clean(line: &str) -> String {
    let line = line.split(" \u{2014} ").next().unwrap_or(line);
    line.trim().trim_end_matches("(implicit)").trim().to_owned()
}

fn property_of(line: &str) -> Option<(String, String)> {
    let (name, value) = line.split_once(": ")?;
    let name = PROPERTIES.iter().find(|known| **known == name)?;
    let value = value.trim().trim_end_matches("(augmented)").trim();
    Some(((*name).to_owned(), value.to_owned()))
}

pub fn parse_map(item: &Item) -> Option<MapInfo> {
    if !is_map(item) {
        return None;
    }
    let mut properties: Vec<(String, String)> = Vec::new();
    let mut mods = Vec::new();
    let advanced = item.sections.iter().flatten().any(|line| line.starts_with('{'));
    let marked = item.sections.iter().flatten().any(|line| line.trim_end().ends_with("(implicit)"));
    for (index, section) in item.sections.iter().enumerate().skip(1) {
        let headed = section.first().is_some_and(|line| line.starts_with('{'));
        let properties_here: Vec<_> = section.iter().filter_map(|line| property_of(line)).collect();
        if !properties_here.is_empty() && !headed {
            for property in properties_here {
                if !properties.iter().any(|(name, _)| *name == property.0) {
                    properties.push(property);
                }
            }
            continue;
        }
        let closing = |line: &str| {
            line.starts_with("Travel to") || line.starts_with("Take this") || line.starts_with("Place into") || line.starts_with("Right click")
        };
        let mut in_mod = false;
        for line in section {
            let line = line.trim();
            if line.is_empty() || line.starts_with('(') || closing(line) {
                continue;
            }
            if line.starts_with('{') {
                in_mod = true;
                continue;
            }
            let wanted = if advanced {
                in_mod || line.ends_with("(implicit)")
            } else if marked {
                line.ends_with("(implicit)")
            } else {
                index > 1
                    && !line.contains(": ")
                    && !matches!(line, "Unidentified" | "Corrupted" | "Mirrored" | "Synthesised Item" | "Fractured Item")
            };
            if wanted {
                mods.push(clean(line));
            }
        }
    }
    let base = item.base.as_str();
    let whole = match (&item.rarity, &item.name) {
        (Rarity::Rare | Rarity::Unique, Some(name)) => format!("{name} {base}"),
        _ => base.to_owned(),
    };
    let tier = properties
        .iter()
        .find(|(name, _)| name == "Map Tier")
        .and_then(|(_, value)| value.parse().ok())
        .or_else(|| tier_in_name(&whole))
        .or_else(|| item.name.as_deref().and_then(tier_in_name));
    // The tier is said once: as the tier, not as a property.
    properties.retain(|(name, _)| name != "Map Tier");
    let name = whole.split(" (Tier ").next().unwrap_or(&whole).to_owned();
    Some(MapInfo { name, tier, properties, mods })
}
