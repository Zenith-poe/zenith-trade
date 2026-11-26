//! The mods of a copied item.
//!
//! The game writes mods in two ways, and one item can hold both.
//!
//! Described: a line in braces introduces each mod and gives its kind, its name, its tier and
//! its tags, as in `{ Prefix Modifier "Fecund" (Tier: 1) — Life }`. The lines up to the next
//! brace line or the end of the section are the mod's lines.
//!
//! Marked: no brace line; every line is a mod of its own and may end in a mark in
//! parentheses, as in `35% increased Movement Speed (fractured)`. These sections come after
//! the item level. A section where a line is marked `(implicit)` or `(enchant)` holds only
//! such mods; its other lines (the area names of a logbook) are not mods. Any other mark
//! shows the section of the explicit mods. When no line is marked and the item has no brace
//! line, the explicit mods are taken to be the first section after the item level that is
//! not a list of properties, on an identified magic, rare or unique item only: elsewhere,
//! and on an item without a rarity line, that section is help text or the item's story.
//! Nothing in this format ties the lines of one mod together.
//!
//! In both, a line wholly in parentheses explains a word of the mod and is left out.

use crate::item::{Item, Rarity};

/// What follows some lines whose value does not grow with other bonuses.
const UNSCALABLE: &str = " — Unscalable Value";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModKind {
    Implicit,
    Prefix,
    Suffix,
    Unique,
    Enchant,
    /// Anything else: the words before `Modifier` of a brace line this version does not
    /// know, as written; without brace lines, the mark of the line (`mutated`), or
    /// `explicit` for a line with no mark, with `(crafted)` or with `(fractured)`.
    Other(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModLine {
    /// The line as copied.
    pub text: String,
    /// The line with `#` for each number, without the roll ranges, the ` — Unscalable Value`
    /// tail and the mark. A sign stays where it is written: `+# to maximum Mana`.
    pub template: String,
    /// The numbers, in order. A number written with `-` in front is negative.
    pub values: Vec<f64>,
    /// For each number, the range it was rolled in when the line gives one.
    pub ranges: Vec<Option<(f64, f64)>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mod {
    pub kind: ModKind,
    pub name: Option<String>,
    pub tier: Option<u32>,
    pub tags: Vec<String>,
    pub crafted: bool,
    pub fractured: bool,
    /// An implicit that corrupting the item gave it: it decides the price, so it is never a
    /// fixed line of a unique.
    pub corruption: bool,
    pub lines: Vec<ModLine>,
}

impl Mod {
    /// Whether the Foulborn mechanic changed this mod: its lines are marked `(mutated)`, or its brace line says `Foulborn Unique Modifier`.
    /// The implicit a vestigial item has: as an enchant, it is what the item is sold for.
    pub fn vestigial(&self) -> bool {
        matches!(&self.kind, ModKind::Other(kind) if kind.starts_with("Vestigial"))
    }

    pub fn foulborn(&self) -> bool {
        matches!(&self.kind, ModKind::Other(kind) if kind == "mutated" || kind.starts_with("Foulborn")) || self.tags.iter().any(|tag| tag == "mutated")
    }
}

/// The prefixes and the suffixes a rare item can still take, in that order: three of each, two
/// for a jewel. A crafted or fractured mod fills its slot like any other. `None` for an item that
/// is not an identified rare.
pub fn open_affixes(item: &Item, mods: &[Mod]) -> Option<(u32, u32)> {
    if item.rarity != crate::item::Rarity::Rare || !item.identified {
        return None;
    }
    let cap: u32 = if item.class.contains("Jewel") { 2 } else { 3 };
    let count = |kind: ModKind| mods.iter().filter(|parsed| parsed.kind == kind).count() as u32;
    Some((cap.saturating_sub(count(ModKind::Prefix)), cap.saturating_sub(count(ModKind::Suffix))))
}

pub fn parse_mods(item: &Item) -> Vec<Mod> {
    let sections = item.sections.get(1..).unwrap_or_default();
    let described = sections.iter().flatten().any(|line| is_brace_line(line));
    // Marked mods come after the item level; before it, `(augmented)` and `(unmet)` end
    // properties and requirements.
    let item_level = sections
        .iter()
        .position(|section| section.iter().any(|line| line.starts_with("Item Level: ")));
    let after_item_level = |index: usize| item_level.is_some_and(|at| index > at);
    let marked_explicit = sections
        .iter()
        .enumerate()
        .any(|(index, section)| after_item_level(index) && marks_of(section) == Marks::Explicit);
    let mut unmarked_explicit = !described
        && !marked_explicit
        && item.identified
        && matches!(item.rarity, Rarity::Magic | Rarity::Rare | Rarity::Unique);

    let mut mods = Vec::new();
    for (index, section) in sections.iter().enumerate() {
        if section.iter().any(|line| is_brace_line(line)) {
            described_mods(section, &mut mods);
        } else if after_item_level(index) {
            match marks_of(section) {
                Marks::Own => marked_mods(section, true, &mut mods),
                Marks::Explicit => marked_mods(section, false, &mut mods),
                Marks::None if unmarked_explicit && !is_properties(section) => {
                    marked_mods(section, false, &mut mods);
                    unmarked_explicit = false;
                }
                Marks::None => {}
            }
        }
    }
    mods
}

/// The lines of a section without the reminder text the game writes over several lines in
/// brackets, under a keystone mod: `(You can only have one Herald` ... `Your Aura Skills are
/// Disabled)`. A reminder on one line is left out by `parse_line`.
fn without_reminders(section: &[String]) -> Vec<&String> {
    let mut inside = false;
    section
        .iter()
        .filter(|line| {
            let text = line.trim();
            if inside {
                inside = !text.ends_with(')');
                return false;
            }
            if text.starts_with('(') && !text.ends_with(')') {
                inside = true;
                return false;
            }
            true
        })
        .collect()
}

fn is_brace_line(line: &str) -> bool {
    line.starts_with('{')
}

/// The mods of a section that has brace lines. Lines before the first brace line belong to
/// no mod; a brace line that no line follows gives no mod.
fn described_mods(section: &[String], mods: &mut Vec<Mod>) {
    let mut current: Option<Mod> = None;
    for line in without_reminders(section) {
        if let Some(described) = parse_brace_line(line) {
            mods.extend(current.replace(described).filter(has_lines));
        } else if let Some(current) = current.as_mut()
            && let Some((line, mark)) = parse_line(line)
        {
            current.crafted |= mark == Some("crafted");
            current.fractured |= mark == Some("fractured");
            if mark == Some("mutated") && !current.tags.iter().any(|tag| tag == "mutated") {
                current.tags.push("mutated".to_owned());
            }
            current.lines.push(line);
        }
    }
    mods.extend(current.filter(has_lines));
}

fn has_lines(parsed: &Mod) -> bool {
    !parsed.lines.is_empty()
}

/// `{ <words> Modifier "<name>" (Tier: <n>) — <tag>, <tag> }`, where the name, the tier and
/// the tags may each be missing.
fn parse_brace_line(line: &str) -> Option<Mod> {
    let inner = line.trim_end().strip_prefix('{')?;
    let inner = inner.strip_suffix('}').unwrap_or(inner).trim();
    let (wording, rest) = match inner.split_once("Modifier") {
        Some((wording, rest)) => (wording.trim(), rest.trim_start()),
        None => (inner, ""),
    };

    let mut known = wording;
    let crafted = strip_words(&mut known, "Master Crafted ");
    let fractured = strip_words(&mut known, "Fractured ");
    let kind = match known {
        "Prefix" => ModKind::Prefix,
        "Suffix" => ModKind::Suffix,
        "Unique" => ModKind::Unique,
        "Enchant" => ModKind::Enchant,
        // An implicit that corrupting the item gave it.
        "Implicit" | "Corruption Implicit" => ModKind::Implicit,
        _ => ModKind::Other(wording.to_string()),
    };

    let (name, rest) = match rest.strip_prefix('"').and_then(|rest| rest.split_once('"')) {
        Some((name, rest)) => (Some(name), rest),
        None => (None, rest),
    };
    let mut parts = rest.split('—');
    let tier = parts
        .next()
        .and_then(|details| details.split_once("(Tier: "))
        .and_then(|(_, tier)| tier.split_once(')'))
        .and_then(|(tier, _)| tier.trim().parse().ok());
    // A part with a number, as in "— 178% Increased", is a note on the mod, not its tags.
    let tags = parts
        .filter(|part| !part.contains(|c: char| c.is_ascii_digit()))
        .flat_map(|part| part.split(','))
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect();

    Some(Mod {
        kind,
        // Some mods are written with the name "".
        name: name.filter(|name| !name.is_empty()).map(str::to_string),
        tier,
        tags,
        crafted,
        fractured,
        corruption: known.starts_with("Corruption") && known.ends_with("Implicit"),
        lines: Vec::new(),
    })
}

fn strip_words(text: &mut &str, words: &str) -> bool {
    match text.strip_prefix(words) {
        Some(rest) => {
            *text = rest;
            true
        }
        None => false,
    }
}

/// What the marks of a section without brace lines say about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Marks {
    /// No line is marked.
    None,
    /// A line is marked `(implicit)` or `(enchant)`: these mods have a section of their own.
    Own,
    /// A line carries another mark: the section of the explicit mods.
    Explicit,
}

fn marks_of(section: &[String]) -> Marks {
    let mut marks = Marks::None;
    for line in section {
        match split_mark(without_unscalable(line.trim())).1 {
            Some("implicit" | "enchant") => return Marks::Own,
            Some(_) => marks = Marks::Explicit,
            None => {}
        }
    }
    marks
}

/// Every line of the section as a mod of its own; with `marked_only`, the marked lines only.
fn marked_mods(section: &[String], marked_only: bool, mods: &mut Vec<Mod>) {
    for line in without_reminders(section) {
        let Some((line, mark)) = parse_line(line) else {
            continue;
        };
        if marked_only && mark.is_none() {
            continue;
        }
        let kind = match mark {
            Some("implicit") => ModKind::Implicit,
            Some("enchant") => ModKind::Enchant,
            Some("crafted" | "fractured") | None => ModKind::Other("explicit".to_string()),
            Some(other) => ModKind::Other(other.to_string()),
        };
        mods.push(Mod {
            kind,
            name: None,
            tier: None,
            tags: Vec::new(),
            crafted: mark == Some("crafted"),
            fractured: mark == Some("fractured"),
            corruption: false,
            lines: vec![line],
        });
    }
}

/// Whether every line is `<name>: <value>`, as in `Monster Level: 83`.
fn is_properties(section: &[String]) -> bool {
    section
        .iter()
        .all(|line| line.contains(": ") || line.trim_end().ends_with(':'))
}

/// A line of a mod and its mark, or `None` for an empty line or a help line.
fn parse_line(line: &str) -> Option<(ModLine, Option<&str>)> {
    let text = line.trim();
    if crate::item::Influence::from_line(text).is_some() {
        return None;
    }
    let (body, mark) = split_mark(without_unscalable(text));
    let body = without_unscalable(body);
    if body.is_empty() || (body.starts_with('(') && body.ends_with(')')) {
        return None;
    }

    let mut template = String::with_capacity(body.len());
    let mut values = Vec::new();
    let mut ranges = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find(|c: char| c.is_ascii_digit()) {
        let (before, from_number) = rest.split_at(start);
        let (number, after) = take_number(from_number);
        let (range, after) = take_range(after);
        template.push_str(before);
        template.push('#');
        values.push(if is_minus(before) { -number } else { number });
        ranges.push(range);
        rest = after;
    }
    template.push_str(rest);

    let line = ModLine {
        text: text.to_string(),
        template,
        values,
        ranges,
    };
    Some((line, mark))
}

fn without_unscalable(line: &str) -> &str {
    let line = line.strip_suffix(UNSCALABLE).unwrap_or(line).trim_end();
    // A reminder the game cut short: `Map is occupied by The Purifier(...)`.
    line.strip_suffix("(...)").unwrap_or(line).trim_end()
}

/// Splits `<text> (<mark>)`, where the mark is one word in lower case, as in `(implicit)`.
fn split_mark(line: &str) -> (&str, Option<&str>) {
    line.strip_suffix(')')
        .and_then(|line| line.rsplit_once(" ("))
        .filter(|(_, mark)| !mark.is_empty() && mark.chars().all(|c| c.is_ascii_lowercase()))
        .map_or((line, None), |(text, mark)| (text.trim_end(), Some(mark)))
}

/// Whether the text before a number ends in a minus sign rather than in the dash of a word
/// or of `8-15`.
fn is_minus(before: &str) -> bool {
    before
        .strip_suffix('-')
        .is_some_and(|before| !before.ends_with(|c: char| c.is_alphanumeric() || c == ')'))
}

/// The number `text` starts with, digits then an optional `.` and digits, and what follows.
fn take_number(text: &str) -> (f64, &str) {
    let digits = |text: &str| {
        text.find(|c: char| !c.is_ascii_digit())
            .unwrap_or(text.len())
    };
    let mut end = digits(text);
    if let Some(fraction) = text[end..].strip_prefix('.')
        && digits(fraction) > 0
    {
        end += 1 + digits(fraction);
    }
    (text[..end].parse().unwrap_or(0.0), &text[end..])
}

/// The roll range right after a number, `(92-100)` or `(20)`, and what follows it.
fn take_range(text: &str) -> (Option<(f64, f64)>, &str) {
    let parsed = text.strip_prefix('(').and_then(|inner| {
        let (low, inner) = take_bound(inner)?;
        if let Some(after) = inner.strip_prefix(')') {
            return Some(((low, low), after));
        }
        let (high, inner) = take_bound(inner.strip_prefix('-')?)?;
        Some(((low, high), inner.strip_prefix(')')?))
    });
    match parsed {
        Some((range, after)) => (Some(range), after),
        None => (None, text),
    }
}

/// One end of a roll range, which may be negative.
fn take_bound(text: &str) -> Option<(f64, &str)> {
    let (sign, digits) = match text.strip_prefix('-') {
        Some(digits) => (-1.0, digits),
        None => (1.0, text),
    };
    if !digits.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let (number, after) = take_number(digits);
    Some((sign * number, after))
}
