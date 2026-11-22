//! The header of an item as the game copies it to the clipboard.
//!
//! The text is sections separated by a line of dashes. The first section holds the item
//! class, the rarity, then one line (the base) or two (the name, then the base). Some items
//! have no rarity line (a viscera): the name lines follow the item class. The item level, the
//! quality and the `Corrupted`, `Fractured Item` and `Unidentified` marks are lines of later
//! sections. Everything else is kept as it is in `Item::sections`: the body part of a
//! viscera (`Head`), for one.

const SEPARATOR: &str = "--------";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rarity {
    Normal,
    Magic,
    Rare,
    Unique,
    Gem,
    Currency,
    DivinationCard,
    /// A wording this version does not know, as written by the game.
    Other(String),
    /// The item has no rarity line: the name follows the item class, as on a viscera.
    None,
}

impl Rarity {
    fn from_wording(word: &str) -> Self {
        match word {
            "Normal" => Self::Normal,
            "Magic" => Self::Magic,
            "Rare" => Self::Rare,
            "Unique" => Self::Unique,
            "Gem" => Self::Gem,
            "Currency" => Self::Currency,
            "Divination Card" => Self::DivinationCard,
            other => Self::Other(other.to_string()),
        }
    }
}

/// What an item carries besides its mods: the lines `Shaper Item`, `Synthesised Item`...
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Influence {
    Shaper,
    Elder,
    Crusader,
    Redeemer,
    Hunter,
    Warlord,
    Searing,
    Eater,
    Synthesised,
}

impl Influence {
    pub const ALL: [Influence; 9] = [
        Self::Shaper,
        Self::Elder,
        Self::Crusader,
        Self::Redeemer,
        Self::Hunter,
        Self::Warlord,
        Self::Searing,
        Self::Eater,
        Self::Synthesised,
    ];

    pub(crate) fn from_line(line: &str) -> Option<Self> {
        Some(match line {
            "Shaper Item" => Self::Shaper,
            "Elder Item" => Self::Elder,
            "Crusader Item" => Self::Crusader,
            "Redeemer Item" => Self::Redeemer,
            "Hunter Item" => Self::Hunter,
            "Warlord Item" => Self::Warlord,
            "Searing Exarch Item" => Self::Searing,
            "Eater of Worlds Item" => Self::Eater,
            "Synthesised Item" => Self::Synthesised,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Shaper => "Shaper",
            Self::Elder => "Elder",
            Self::Crusader => "Crusader",
            Self::Redeemer => "Redeemer",
            Self::Hunter => "Hunter",
            Self::Warlord => "Warlord",
            Self::Searing => "Searing Exarch",
            Self::Eater => "Eater of Worlds",
            Self::Synthesised => "Synthesised",
        }
    }

    /// Its place in a set of influences held as bits.
    pub fn bit(self) -> u16 {
        1 << self as u16
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub class: String,
    pub rarity: Rarity,
    /// The name line, when the first section has one. A magic item has none: its only
    /// line holds the affixes and the base together.
    pub name: Option<String>,
    pub base: String,
    pub item_level: Option<u32>,
    pub quality: Option<u32>,
    pub corrupted: bool,
    /// One of its mods at least can no longer be changed: the item says `Fractured Item`.
    pub fractured: bool,
    pub identified: bool,
    /// The influences and the synthesis the item has, in the order of `Influence::ALL`.
    pub influences: Vec<Influence>,
    pub mirrored: bool,
    pub split: bool,
    /// `Stack Size: 14/20`: how many are held, and how many a stack holds at most.
    pub stack: Option<(u32, u32)>,
    /// Every section's lines as copied, the first section included.
    pub sections: Vec<Vec<String>>,
}

/// The level of a gem: the first `Level:` line, which is the gem itself (the requirements, whose
/// line has the same name, come after the properties).
pub fn gem_level(item: &Item) -> Option<u32> {
    let value = item.sections.iter().skip(1).flatten().find_map(|line| line.strip_prefix("Level: "))?;
    value.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// The text does not start with an item class line.
    NotAnItem,
    /// Nothing usable follows the item class: neither a rarity nor a name.
    MissingRarity,
    /// The first section ends after the rarity.
    MissingBase,
}

pub fn parse_item(text: &str) -> Result<Item, ParseError> {
    let sections = split_sections(text);

    let mut header = sections[0].iter().map(|line| line.trim_end());
    let class = header
        .next()
        .and_then(|line| line.strip_prefix("Item Class: "))
        .ok_or(ParseError::NotAnItem)?;
    let mut lines = header.map(|line| Some(line).filter(|line| !line.is_empty()));
    let after_class = lines.next().flatten().ok_or(ParseError::MissingRarity)?;
    let (rarity, first) = match after_class.strip_prefix("Rarity: ") {
        Some(wording) => {
            let first = lines.next().flatten().ok_or(ParseError::MissingBase)?;
            (Rarity::from_wording(wording), first)
        }
        // A rarity line that lost its wording is not a name.
        None if after_class == "Rarity:" => return Err(ParseError::MissingRarity),
        None => (Rarity::None, after_class),
    };
    let (name, base) = match lines.next().flatten() {
        Some(second) => (Some(first.to_string()), second),
        None => (None, first),
    };
    // A synthesised item is also named so: `Synthesised Vaal Regalia`.
    let (base, synthesised_name) = match base.strip_prefix("Synthesised ") {
        Some(rest) => (rest, true),
        None => (base, false),
    };

    let mut item_level = None;
    let mut quality = None;
    let mut corrupted = false;
    let mut fractured = false;
    let mut identified = true;
    let mut influences = Vec::new();
    let mut mirrored = false;
    let mut split = false;
    let mut stack = None;
    for line in sections.iter().skip(1).flatten().map(|line| line.trim_end()) {
        if let Some(value) = line.strip_prefix("Item Level: ") {
            item_level = item_level.or_else(|| value.parse().ok());
        } else if let Some(value) = line.strip_prefix("Quality: ") {
            quality = quality.or_else(|| parse_quality(value));
        } else if let Some(value) = line.strip_prefix("Stack Size: ") {
            stack = stack.or_else(|| parse_stack(value));
        } else if line == "Corrupted" {
            corrupted = true;
        } else if line == "Fractured Item" {
            fractured = true;
        } else if line == "Unidentified" {
            identified = false;
        } else if line == "Mirrored" {
            mirrored = true;
        } else if line == "Split" {
            split = true;
        } else if let Some(influence) = Influence::from_line(line)
            && !influences.contains(&influence)
        {
            influences.push(influence);
        }
    }

    if synthesised_name && !influences.contains(&Influence::Synthesised) {
        influences.push(Influence::Synthesised);
    }
    influences.sort_by_key(|influence| *influence as u8);

    Ok(Item {
        class: class.to_string(),
        rarity,
        name,
        base: base.to_string(),
        item_level,
        quality,
        corrupted,
        fractured,
        identified,
        influences,
        mirrored,
        split,
        stack,
        sections,
    })
}

/// Always returns at least one section, which may be empty.
fn split_sections(text: &str) -> Vec<Vec<String>> {
    let mut sections = vec![Vec::new()];
    for line in text.lines() {
        // `lines` removes "\r\n" but leaves a carriage return that ends the text.
        let line = line.trim_end_matches('\r');
        if line == SEPARATOR {
            sections.push(Vec::new());
        } else if let Some(section) = sections.last_mut() {
            section.push(line.to_string());
        }
    }
    sections
}

/// `14/20`, and `1,234/5,000`: the separator of thousands is a comma, a point or a space of
/// any width, as the game's language has it.
fn parse_stack(value: &str) -> Option<(u32, u32)> {
    let number = |part: &str| {
        let digits: String = part.chars().filter(|c| !matches!(c, ',' | '.' | ' ' | '\u{a0}' | '\u{202f}')).collect();
        digits.parse::<u32>().ok()
    };
    let (held, most) = value.trim().split_once('/')?;
    Some((number(held)?, number(most)?))
}

/// The number of `+20%` or `+20% (augmented)`.
fn parse_quality(value: &str) -> Option<u32> {
    let (number, _) = value.strip_prefix('+')?.split_once('%')?;
    number.parse().ok()
}

#[cfg(test)]
mod stack_tests {
    use super::*;

    fn stack_of(line: &str) -> Option<(u32, u32)> {
        let text = format!("Item Class: Stackable Currency\nRarity: Currency\nChaos Orb\n--------\n{line}\n--------\nRight click this item.\n");
        parse_item(&text).unwrap().stack
    }

    #[test]
    fn a_stack_size_is_read_with_or_without_separators() {
        assert_eq!(stack_of("Stack Size: 14/20"), Some((14, 20)));
        assert_eq!(stack_of("Stack Size: 1,234/5,000"), Some((1234, 5000)));
        assert_eq!(stack_of("Stack Size: 1\u{a0}234/5\u{a0}000"), Some((1234, 5000)));
        assert_eq!(stack_of("Stack Size: 1.234/5.000"), Some((1234, 5000)));
    }

    #[test]
    fn a_stack_size_that_cannot_be_read_is_none() {
        assert_eq!(stack_of("Stack Size: many"), None);
        assert_eq!(stack_of("Stack Size: 14/"), None);
        assert_eq!(stack_of("Quality: +20%"), None);
    }
}

