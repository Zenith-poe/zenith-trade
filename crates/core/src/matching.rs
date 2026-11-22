//! Mods to the stats of the trade site, and the filters a search starts with.
//!
//! The site lists its stats in groups (`explicit`, `implicit`, `fractured`, ...), each stat
//! as a text with `#` for every number: `+# to maximum Life`. A mod is searchable when one
//! of its texts is in the list of its group.
//!
//! A mod of several lines is tried as one text, the lines joined by a line feed, then line
//! by line; the first line in the list gives the id. Failing that, its first line is tried
//! as the start of a longer text, for the mods the older format writes line by line.
//!
//! Each text is tried as written, then in the wordings the site may list instead:
//!
//! - `-#` as `+#`, `reduced` as `increased` and `less` as `more`, which turn the value
//!   around;
//! - `an additional Arrow` as `# additional Arrows`, with the value 1.
//!
//! All of that with `#` for every number first, then with some of the numbers written out:
//! the site keeps the numbers that never vary, as in `Has 1 Socket`.
//!
//! A stat of the item itself, not of the character, is listed with ` (Local)` after its
//! text, sometimes beside the plain text and sometimes alone; see [`Local`].

use std::collections::BTreeMap;
use std::ops::Bound;

use crate::mods::{Mod, ModKind, ModLine};
use crate::props::Properties;

/// `stats[group][text]` lists the trade ids of a text, as the data pack holds them.
pub type Stats = BTreeMap<String, BTreeMap<String, Vec<String>>>;

/// The ids of a group, by text.
type Texts = BTreeMap<String, Vec<String>>;

/// What the site writes after the text of a stat of the item itself.
const LOCAL: &str = " (Local)";

/// The words that make a text a local one on a piece of armour.
const DEFENCES: [&str; 4] = ["Armour", "Evasion", "Energy Shield", "Ward"];

/// The `Filter::id` of a property. They are the names the site gives these filters; a
/// stat id has a dot, so the two cannot be confused.
pub const ARMOUR: &str = "ar";
pub const EVASION: &str = "ev";
pub const ENERGY_SHIELD: &str = "es";
pub const PHYSICAL_DPS: &str = "pdps";
pub const ELEMENTAL_DPS: &str = "edps";
/// Every damage together: physical, elemental and chaos.
pub const TOTAL_DPS: &str = "dps";
/// A weapon's critical strike chance.
pub const CRITICAL_CHANCE: &str = "crit";
/// A weapon's attacks per second.
pub const ATTACKS_PER_SECOND: &str = "aps";

/// Which texts of an item are tried with ` (Local)` first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Local {
    /// A weapon: every text. Attack speed, accuracy and leech are local on a weapon only.
    Weapon,
    /// A piece of armour: the texts that name a defence.
    Armour,
    /// Anything else: none.
    None,
}

/// What the lists say about one mod.
#[derive(Debug, Clone, PartialEq)]
pub struct Matched {
    /// The trade id; `None` when the mod is in no list, so not searchable.
    pub id: Option<String>,
    /// The mean of the numbers the text that matched leaves open, as the stat counts it:
    /// negative for `20% reduced` found as `#% increased`. `None` without such a number or
    /// without an id.
    pub value: Option<f64>,
    /// The least and the most `value` could have been, counted as it is: the mean of the low
    /// ends of the ranges the numbers were rolled in, and the mean of their high ends. A number
    /// that has no range with two ends counts for itself at both. `None` when no number has
    /// such a range, or without a value.
    pub range: Option<(f64, f64)>,
    /// The mod as a person reads it, without the roll ranges; one line per line of the mod,
    /// separated by line feeds.
    pub label: String,
}

/// One line of the search panel: a property or a mod, and the bounds asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    /// A trade stat id, or one of the property ids of this module.
    pub id: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// Whether the search uses it.
    pub on: bool,
    pub label: String,
    pub tier: Option<u32>,
}

/// The stats a hybrid mod adds to a local stat, by trade stat id: the site sums every line that
/// gives the same stat, so a "pure" tier of the stat must not have these. A short list for the
/// common local hybrids (the data pack does not tell them apart reliably): a % defence with
/// Stun and Block Recovery, and a % physical damage with accuracy rating.
pub fn hybrid_partners(id: &str) -> &'static [&'static str] {
    const STUN: &str = "explicit.stat_2511217560";
    const ACCURACY: &str = "explicit.stat_691932474";
    match id {
        // Local % Armour, % Evasion Rating, % Energy Shield.
        "explicit.stat_1062208444" | "explicit.stat_124859000" | "explicit.stat_4015621042" => &[STUN],
        // Local % physical damage.
        "explicit.stat_1509134228" => &[ACCURACY],
        _ => &[],
    }
}

pub fn match_mod(m: &Mod, local: Local, stats: &Stats) -> Matched {
    let label = label(m);
    let found = stats
        .get(group(m))
        .and_then(|texts| find_mod(m, local, texts));
    match found {
        Some(found) => Matched {
            id: Some(found.id),
            value: found.value,
            range: found.range,
            label,
        },
        None => Matched {
            id: None,
            value: None,
            range: None,
            label,
        },
    }
}

/// Whether the value of a property filter is the one the item would have at 20% quality and
/// not the one it has now: a defence or a physical damage per second of an item under 20%
/// quality, unless one of its mods takes that number away from the quality. The total damage
/// per second is, as soon as a part of it is physical.
pub fn at_trade_quality(id: &str, props: &Properties) -> bool {
    match id {
        ARMOUR | EVASION | ENERGY_SHIELD => props.defences_raised(),
        PHYSICAL_DPS | TOTAL_DPS => props.physical.is_some() && props.physical_raised(),
        _ => false,
    }
}

/// A damage per second as it is read: one decimal under 100, a whole number from there.
fn shown_dps(value: f64) -> f64 {
    if value < 100.0 {
        (value * 10.0).round() / 10.0
    } else {
        value.round()
    }
}

/// The filters a search starts with: the defences and, on a weapon, the damage per second;
/// then the pseudo totals (see [`crate::pseudo`]); then every matched mod, switched on when it is of the first two tiers or fractured. A mod
/// in no list is not a filter.
///
/// The defences and the physical damage are the ones the item would have at 20% quality,
/// which is how the site compares them: see [`crate::props`]. Each is asked from that value:
/// see [`Slack`] for a looser start.
///
/// A weapon that deals one kind of damage has the filter of that kind, switched on. One that
/// deals several (physical, elemental, chaos) has the total, switched on, and the physical and
/// the elemental ones, switched off: such a weapon is wanted for all of its damage. A weapon
/// that deals chaos damage alone has the total alone.
pub fn default_filters(
    mods: &[Mod],
    props: &Properties,
    local: Local,
    stats: &Stats,
) -> Vec<Filter> {
    let weapon = local == Local::Weapon;
    let kinds = [
        props.physical.is_some(),
        !props.elemental.is_empty(),
        props.chaos.is_some(),
    ];
    let physical_dps = props.pdps_q20().filter(|_| weapon);
    let elemental_dps = props.edps().filter(|_| weapon);
    // The larger of physical and elemental damage is the one a buyer is after; total, crit and
    // attack speed are there to switch on.
    let physical_leads = physical_dps.unwrap_or(0.0) >= elemental_dps.unwrap_or(0.0);
    let lone_chaos = kinds == [false, false, true];
    let whole = |value: Option<u32>| value.map(f64::from);
    let properties = [
        (ARMOUR, "Armour", whole(props.armour_q20()), true),
        (EVASION, "Evasion", whole(props.evasion_q20()), true),
        (
            ENERGY_SHIELD,
            "Energy shield",
            whole(props.energy_shield_q20()),
            true,
        ),
        (
            PHYSICAL_DPS,
            "Physical DPS",
            physical_dps,
            physical_leads,
        ),
        (
            ELEMENTAL_DPS,
            "Elemental DPS",
            elemental_dps,
            !physical_leads,
        ),
        (
            TOTAL_DPS,
            "Total DPS",
            props.dps_q20().filter(|_| weapon),
            lone_chaos,
        ),
        (
            CRITICAL_CHANCE,
            "Critical strike chance",
            props.critical_chance.filter(|_| weapon),
            false,
        ),
        (
            ATTACKS_PER_SECOND,
            "Attacks per second",
            props.attacks_per_second.filter(|_| weapon),
            false,
        ),
    ];
    let mut filters = Vec::new();
    for (id, name, value, on) in properties {
        if let Some(value) = value {
            filters.push(Filter {
                id: id.to_string(),
                min: Some(value),
                max: None,
                on,
                label: if id == CRITICAL_CHANCE || id == ATTACKS_PER_SECOND {
                    format!("{name} {value}")
                } else {
                    format!("{name} {}", shown_dps(value))
                },
                tier: None,
            });
        }
    }
    // The pseudo totals come before the mods, and the lines the switched-on ones add up start off.
    let pseudo = crate::pseudo::pseudo_totals(mods, local, stats);
    let covered: Vec<usize> = pseudo.iter().filter(|p| p.on).flat_map(|p| p.covers.clone()).collect();
    for p in pseudo {
        filters.push(Filter {
            id: p.id,
            min: Some(p.value),
            max: None,
            on: p.on,
            label: p.label,
            tier: None,
        });
    }
    let first_mod = filters.len();
    let mut sums: Vec<(String, Option<f64>)> = Vec::new();
    for m in mods {
        let matched = match_mod(m, local, stats);
        let Some(id) = matched.id else { continue };
        let on = matches!(m.tier, Some(1 | 2)) || m.fractured || m.corruption;
        match filters.iter_mut().find(|filter| filter.id == id) {
            Some(filter) => {
                // The same stat on another line: the site adds them up.
                let sum = sums
                    .iter_mut()
                    .find(|(seen, _)| *seen == id)
                    .expect("recorded with its filter");
                sum.1 = add_values(sum.1, matched.value);
                filter.min = sum.1;
                filter.on |= on;
                filter.label = format!("{} + {}", filter.label, matched.label);
            }
            None => {
                sums.push((id.clone(), matched.value));
                filters.push(Filter {
                    id,
                    min: matched.value,
                    max: None,
                    on,
                    label: matched.label,
                    tier: m.tier,
                });
            }
        }
    }
    // A mod covered by a total that is on starts off, whatever its tier.
    for index in covered {
        let id = mods.get(index).and_then(|m| match_mod(m, local, stats).id);
        if let Some(filter) = filters[first_mod..].iter_mut().find(|filter| Some(&filter.id) == id.as_ref()) {
            filter.on = false;
        }
    }
    filters
}

/// The filters of a unique item, which is searched by its name: every matched mod, switched
/// off, with the value the item has for the minimum it is asked from once it is switched
/// on. The same unique is wanted with this roll or a better one, so nothing is taken off the
/// value. A mod in no list is not a filter.
pub fn unique_filters(mods: &[Mod], local: Local, stats: &Stats) -> Vec<Filter> {
    mods.iter()
        .filter_map(|m| {
            let matched = match_mod(m, local, stats);
            Some(Filter {
                id: matched.id?,
                min: matched.value,
                max: None,
                // What corrupting gave decides the price, and a Foulborn mod says which Foulborn
                // version the item is: asked for, with its exact value.
                on: m.corruption || m.foulborn() || m.kind == ModKind::Enchant,
                label: matched.label,
                tier: m.tier,
            })
        })
        .collect()
}

/// The sum of two values of one stat; a line without a number adds nothing.
fn add_values(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a + b),
        (a, b) => a.or(b),
    }
}

/// How far under its value a filter's minimum starts, as the player chose it: not at all by
/// default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Slack {
    #[default]
    Exact,
    Five,
    Ten,
    Twenty,
}

/// A value under this is never loosened: 90% of it rounded down would ask for a level less of
/// `+2 to Level of ...`.
const SMALL: f64 = 10.0;

impl Slack {
    pub const ALL: [Slack; 4] = [Slack::Exact, Slack::Five, Slack::Ten, Slack::Twenty];

    /// The share taken off, in percent.
    pub fn percent(self) -> u32 {
        match self {
            Slack::Exact => 0,
            Slack::Five => 5,
            Slack::Ten => 10,
            Slack::Twenty => 20,
        }
    }

    /// The minimum asked for a value: the value itself when exact or small; otherwise that much
    /// under it, rounded down, or for a negative value that much over it, rounded toward zero.
    pub fn lower(self, value: f64) -> f64 {
        let percent = f64::from(self.percent());
        if percent == 0.0 || value.abs() < SMALL {
            return value;
        }
        let bound = if value < 0.0 { (value * (100.0 + percent) / 100.0).trunc() } else { (value * (100.0 - percent) / 100.0).floor() };
        // Not "-0".
        bound + 0.0
    }
}

/// The group of the site a mod is looked up in. The marks win over the kind.
fn group(m: &Mod) -> &'static str {
    if m.fractured {
        return "fractured";
    }
    if m.crafted {
        return "crafted";
    }
    match &m.kind {
        ModKind::Implicit => "implicit",
        ModKind::Enchant => "enchant",
        // A brace wording the parser keeps whole, as in "Searing Exarch Implicit".
        ModKind::Other(wording) if wording.ends_with("Implicit") => "implicit",
        ModKind::Prefix | ModKind::Suffix | ModKind::Unique | ModKind::Other(_) => "explicit",
    }
}

/// One number of a text: its value, and the range it was rolled in when the item gives one.
#[derive(Debug, Clone, Copy)]
struct Number {
    value: f64,
    range: Option<(f64, f64)>,
}

impl Number {
    /// The two ends of its range, as written, when they differ: `15(20)` has none.
    fn ends(&self) -> Option<(f64, f64)> {
        self.range.filter(|(low, high)| low != high)
    }
}

/// The numbers of a line, in order.
fn numbers(line: &ModLine) -> Vec<Number> {
    line.values
        .iter()
        .enumerate()
        .map(|(index, value)| Number {
            value: *value,
            range: line.ranges.get(index).copied().flatten(),
        })
        .collect()
}

/// What the lists hold for a text: see [`Matched`].
struct Found {
    id: String,
    value: Option<f64>,
    range: Option<(f64, f64)>,
}

/// The id of a mod, its value and the range of it: the lines as one text, then each line,
/// then the first line as the start of a longer text.
fn find_mod(m: &Mod, local: Local, texts: &Texts) -> Option<Found> {
    if m.lines.len() > 1 {
        let templates: Vec<&str> = m.lines.iter().map(|line| line.template.as_str()).collect();
        let all: Vec<Number> = m.lines.iter().flat_map(numbers).collect();
        let joined = templates.join("\n");
        if let Some(found) = find_text(&joined, &all, texts, Place::Whole(local)) {
            return Some(found);
        }
    }
    m.lines
        .iter()
        .find_map(|line| find_text(&line.template, &numbers(line), texts, Place::Whole(local)))
        .or_else(|| {
            let line = m.lines.first()?;
            find_text(&line.template, &numbers(line), texts, Place::FirstLine)
        })
}

/// What a text of the lists must be for a text of the item to match it.
#[derive(Debug, Clone, Copy)]
enum Place {
    /// The same text, in its local form first where the item calls for it.
    Whole(Local),
    /// The only text that starts with it and goes on with other lines. The older format
    /// writes such a mod as lines that nothing ties together.
    FirstLine,
}

/// How the value of a stat follows from the numbers of the text that matched.
#[derive(Debug, Clone, Copy)]
enum Turn {
    /// The mean of the numbers.
    Kept,
    /// The list has the opposite wording: the mean, negated.
    Negated,
    /// The list counts what the item writes as `an`: one more number, 1.
    Counted,
}

/// The id of one text, its value and the range of it. The text is tried with `#` for every
/// number, then with numbers written out, fewest first; each time as written, then in other
/// wordings.
fn find_text(text: &str, numbers: &[Number], texts: &Texts, place: Place) -> Option<Found> {
    numberings(text, numbers).find_map(|(text, open)| {
        wordings(&text).into_iter().find_map(|(text, turn)| {
            let id = match place {
                Place::Whole(local) => find_whole(&text, local, texts),
                Place::FirstLine => find_first_line(&text, texts),
            }?;
            // The value, or one end of its range: what `pick` takes of each open number.
            let counted = |pick: &dyn Fn(&Number) -> f64| {
                let picked: Vec<f64> = open.iter().map(pick).collect();
                match turn {
                    Turn::Kept => mean(&picked),
                    Turn::Negated => mean(&picked).map(|value| -value),
                    Turn::Counted => mean(&[picked.as_slice(), &[1.0]].concat()),
                }
            };
            let value = counted(&|number| number.value);
            let range = if open.iter().any(|number| number.ends().is_some()) {
                let low = counted(&|number| number.ends().map_or(number.value, |(low, _)| low));
                let high = counted(&|number| number.ends().map_or(number.value, |(_, high)| high));
                // Turned around, or written from the high end: from the lower to the higher.
                low.zip(high)
                    .map(|(low, high)| (low.min(high), low.max(high)))
            } else {
                None
            };
            Some(Found {
                id: id.clone(),
                value,
                range,
            })
        })
    })
}

/// The most numbers of a text for which every way of writing some of them out is tried.
const MOST_NUMBERS: usize = 6;

/// A text with `#` for every number, then with some numbers written out as the item has
/// them, fewest first: the lists keep the numbers that never vary, as in
/// `#% chance to gain Onslaught for 4 seconds on Kill`. Each comes with the numbers its
/// `#` still stand for.
fn numberings<'a>(
    text: &'a str,
    numbers: &'a [Number],
) -> impl Iterator<Item = (String, Vec<Number>)> + 'a {
    let parts: Vec<&str> = text.split('#').collect();
    let count = numbers.len();
    // Bit `n` set: the number `n` is written out.
    let mut written_out: Vec<u32> = if parts.len() == count + 1 && count <= MOST_NUMBERS {
        (1..1 << count).collect()
    } else {
        Vec::new()
    };
    written_out.sort_by_key(|numbers| numbers.count_ones());
    std::iter::once(0).chain(written_out).map(move |written| {
        if written == 0 {
            return (text.to_string(), numbers.to_vec());
        }
        let mut numbered = String::with_capacity(text.len());
        let mut open = Vec::new();
        for (index, (part, number)) in parts.iter().zip(numbers).enumerate() {
            numbered.push_str(part);
            if written & (1 << index) != 0 {
                // The text keeps the sign.
                numbered.push_str(&number.value.abs().to_string());
            } else {
                numbered.push('#');
                open.push(*number);
            }
        }
        numbered.push_str(parts[count]);
        (numbered, open)
    })
}

/// A text as written, then in the wordings the lists may have instead.
fn wordings(text: &str) -> Vec<(String, Turn)> {
    let mut wordings = vec![(text.to_string(), Turn::Kept)];
    let turned = [
        swap_sign(text),
        replace_word(text, "reduced", "increased"),
        replace_word(text, "less", "more"),
    ];
    wordings.extend(
        turned
            .into_iter()
            .flatten()
            .map(|text| (text, Turn::Negated)),
    );
    wordings.extend(count_additional(text).map(|text| (text, Turn::Counted)));
    wordings
}

/// `text` with its first `-#` written `+#`. The lists write a sign as `+`, at the start as
/// in `+#% to Fire Resistance` or inside as in `Minions have +#% to Fire Resistance`. The
/// dash of `#-#` is not a sign.
fn swap_sign(text: &str) -> Option<String> {
    let at = text.match_indices("-#").map(|(at, _)| at).find(|&at| {
        text[..at]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace)
    })?;
    Some(format!("{}+{}", &text[..at], &text[at + 1..]))
}

/// `... an additional Arrow` as `... # additional Arrows`: the item writes one of them in
/// words where the lists have a number.
fn count_additional(text: &str) -> Option<String> {
    const WORDS: &str = "an additional ";
    let at = find_word(text, WORDS.trim_end())?;
    let rest = text[at..].strip_prefix(WORDS)?;
    let end = rest
        .find(|c: char| !c.is_alphanumeric())
        .unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    let (noun, after) = rest.split_at(end);
    Some(format!("{}# additional {noun}s{after}", &text[..at]))
}

/// The id of a text exactly as given, in its local form first where the item calls for it.
fn find_whole<'a>(text: &str, local: Local, texts: &'a Texts) -> Option<&'a String> {
    let local_first = match local {
        Local::Weapon => true,
        Local::Armour => DEFENCES.iter().any(|word| find_word(text, word).is_some()),
        Local::None => false,
    };
    local_first
        .then(|| texts.get(&format!("{text}{LOCAL}")))
        .flatten()
        .and_then(|ids| pick(ids))
        .or_else(|| texts.get(text).and_then(|ids| pick(ids)))
}

/// The id of the only text that starts with `text` and goes on with other lines; none when
/// several do.
fn find_first_line<'a>(text: &str, texts: &'a Texts) -> Option<&'a String> {
    let start = format!("{text}\n");
    let mut longer = texts
        .range::<str, _>((Bound::Included(start.as_str()), Bound::Unbounded))
        .take_while(|(longer, _)| longer.starts_with(&start));
    let (_, ids) = longer.next()?;
    if longer.next().is_some() {
        return None;
    }
    pick(ids)
}

/// Which id to search with when a text has several: the first the site lists.
fn pick(ids: &[String]) -> Option<&String> {
    ids.first()
}

/// The mean of the numbers as the text writes them: a sign is part of the text, as in
/// `-#% to Fire Resistance`, so the number itself is taken without it.
fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let sum: f64 = values.iter().map(|value| value.abs()).sum();
    Some(sum / values.len() as f64)
}

/// Where `word` first stands in `text` as a word of its own, not inside a longer one.
fn find_word(text: &str, word: &str) -> Option<usize> {
    text.match_indices(word).map(|(at, _)| at).find(|&at| {
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// `text` with the first `from` that is a word of its own replaced by `to`.
fn replace_word(text: &str, from: &str, to: &str) -> Option<String> {
    let at = find_word(text, from)?;
    Some(format!("{}{to}{}", &text[..at], &text[at + from.len()..]))
}

fn label(m: &Mod) -> String {
    let lines: Vec<String> = m.lines.iter().map(readable).collect();
    lines.join("\n")
}

/// The line with its numbers and without its roll ranges: the template, filled in.
fn readable(line: &ModLine) -> String {
    if line.template.matches('#').count() != line.values.len() {
        return line.text.clone();
    }
    let mut values = line.values.iter();
    let mut filled = String::with_capacity(line.text.len());
    for part in line.template.split_inclusive('#') {
        match (part.strip_suffix('#'), values.next()) {
            // The template keeps the sign.
            (Some(before), Some(value)) => {
                filled.push_str(before);
                filled.push_str(&value.abs().to_string());
            }
            _ => filled.push_str(part),
        }
    }
    filled
}
