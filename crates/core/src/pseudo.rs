//! Pseudo totals: the stats of the trade site that add up several lines of a rare. See
//! [`pseudo_totals`].

use crate::matching::{Local, Stats};
use crate::mods::{Mod, ModKind};

/// The group of the data pack that holds the pseudo stats.
const GROUP: &str = "pseudo";

/// Whether a filter id is a pseudo stat of the site.
pub fn is_pseudo(id: &str) -> bool {
    id.starts_with("pseudo.")
}

/// A stat of the item a line can feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Fire,
    Cold,
    Lightning,
    Chaos,
    Strength,
    Dexterity,
    Intelligence,
    Life,
    EnergyShield,
}

use Part::{Chaos, Cold, Dexterity, EnergyShield, Fire, Intelligence, Life, Lightning, Strength};

const ELEMENTAL: [Part; 3] = [Fire, Cold, Lightning];
const ALL_RESISTANCES: [Part; 4] = [Fire, Cold, Lightning, Chaos];
const ATTRIBUTES: [Part; 3] = [Strength, Dexterity, Intelligence];

/// What a line of a mod feeds, by its template.
fn parts_of(template: &str) -> &'static [Part] {
    match template {
        "+#% to Fire Resistance" => &[Fire],
        "+#% to Cold Resistance" => &[Cold],
        "+#% to Lightning Resistance" => &[Lightning],
        "+#% to Chaos Resistance" => &[Chaos],
        "+#% to Fire and Cold Resistances" => &[Fire, Cold],
        "+#% to Cold and Lightning Resistances" => &[Cold, Lightning],
        "+#% to Fire and Lightning Resistances" => &[Fire, Lightning],
        "+#% to all Elemental Resistances" => &ELEMENTAL,
        "+# to Strength" => &[Strength],
        "+# to Dexterity" => &[Dexterity],
        "+# to Intelligence" => &[Intelligence],
        "+# to Strength and Dexterity" => &[Strength, Dexterity],
        "+# to Strength and Intelligence" => &[Strength, Intelligence],
        "+# to Dexterity and Intelligence" => &[Dexterity, Intelligence],
        "+# to all Attributes" => &ATTRIBUTES,
        "+# to maximum Life" => &[Life],
        "+# to maximum Energy Shield" => &[EnergyShield],
        _ => &[],
    }
}

/// One mod that feeds the totals: the mod, what it feeds, its value and the range it was
/// rolled in.
struct Source {
    index: usize,
    parts: &'static [Part],
    value: f64,
    range: Option<(f64, f64)>,
}

/// A pseudo total of the item, as a filter of the search.
#[derive(Debug, Clone, PartialEq)]
pub struct Pseudo {
    /// The trade stat id, `pseudo.pseudo_total_...`.
    pub id: String,
    /// The line as the site words it, with the total: `+102% total Elemental Resistance`.
    pub label: String,
    /// The total.
    pub value: f64,
    /// The least and the most it could have been, when a line gives the range it was rolled in
    /// (a line that gives none counts for its value at both ends); `None` when none does.
    pub range: Option<(f64, f64)>,
    /// Whether the search starts with it.
    pub on: bool,
    /// The mods (indices into the item's mods) it adds up.
    pub covers: Vec<usize>,
}

/// The pseudo totals of a magic or rare item's mods, in the order the panel lists them. Each
/// needs its stat in the `pseudo` group of `stats`.
///
/// Only single-line mods count (implicits, enchants, crafted and fractured ones too), and not
/// unique mods. Every resistance or attribute a
/// line gives counts once (`+10% to all Elemental Resistances` is 30% of total resistance, `+14% to Fire and Cold
/// Resistances` is 28%). The
/// totals:
/// - total resistance with chaos (when a chaos line exists, on), and total elemental
///   resistance (on, unless the total with chaos is);
/// - total of all attributes: what the item gives to every attribute, the smallest of the
///   three totals, only when each is fed (off: `+30 to Dexterity` and `+23 to Intelligence`
///   give none); and the total of each attribute fed by several lines (on);
/// - total maximum Life: only the `+# to maximum Life` lines, nothing from Strength;
/// - total maximum Energy Shield: only the flat lines, and not on a piece of armour, where
///   they are local to it.
///
/// A total needs at least two lines (or one that feeds several parts, for the resistances and
/// attributes). The ones that are on make the lines they cover start off.
pub fn pseudo_totals(mods: &[Mod], local: Local, stats: &Stats) -> Vec<Pseudo> {
    let Some(texts) = stats.get(GROUP) else { return Vec::new() };
    let sources: Vec<Source> = mods
        .iter()
        .enumerate()
        .filter(|(_, m)| !matches!(m.kind, ModKind::Unique))
        .filter_map(|(index, m)| {
            let [line] = m.lines.as_slice() else { return None };
            let parts = parts_of(&line.template);
            let value = *line.values.first()?;
            let range = line.ranges.first().copied().flatten();
            (!parts.is_empty() && !(local != Local::None && parts == [EnergyShield]))
                .then_some(Source { index, parts, value, range })
        })
        .collect();

    // The sources that feed any of `of`, with how many parts each gives to them.
    let feeding = |of: &[Part]| -> Vec<(&Source, usize)> {
        sources
            .iter()
            .filter_map(|s| {
                let n = s.parts.iter().filter(|p| of.contains(p)).count();
                (n > 0).then_some((s, n))
            })
            .collect()
    };
    let mut out: Vec<Pseudo> = Vec::new();
    // `least`: the total is the smallest of the totals of each part (the site's total of all
    // attributes is what the item gives to every attribute), and needs every part fed.
    let mut add = |text: &str, of: &[Part], need_parts: usize, need_chaos: bool, on: bool, least: bool| -> bool {
        let fed = feeding(of);
        let count: usize = fed.iter().map(|(_, n)| n).sum();
        let chaos = fed.iter().any(|(s, _)| s.parts.contains(&Chaos));
        let Some(id) = texts.get(text).and_then(|ids| ids.first()) else { return false };
        if count < need_parts || (need_chaos && !chaos) {
            return false;
        }
        let (value, range) = if least {
            let each: Vec<_> = of.iter().map(|part| sum(&feeding(&[*part]))).collect();
            if each.iter().any(|(value, _)| *value <= 0.0) {
                return false;
            }
            let value = each.iter().map(|(value, _)| *value).fold(f64::INFINITY, f64::min);
            let range = each.iter().any(|(_, range)| range.is_some()).then(|| {
                each.iter().fold((f64::INFINITY, f64::INFINITY), |least, (value, range)| {
                    let (low, high) = range.unwrap_or((*value, *value));
                    (least.0.min(low), least.1.min(high))
                })
            });
            (value, range)
        } else {
            sum(&fed)
        };
        out.push(Pseudo {
            id: id.clone(),
            label: text.replacen('#', &format!("{value}"), 1),
            value,
            range,
            on,
            covers: fed.iter().map(|(s, _)| s.index).collect(),
        });
        true
    };

    let with_chaos = add("+#% total Resistance", &ALL_RESISTANCES, 2, true, true, false);
    add("+#% total Elemental Resistance", &ELEMENTAL, 2, false, !with_chaos, false);
    add("+# total to all Attributes", &ATTRIBUTES, 2, false, false, true);
    for (text, part) in [
        ("+# total to Strength", Strength),
        ("+# total to Dexterity", Dexterity),
        ("+# total to Intelligence", Intelligence),
    ] {
        // Several lines, not one that feeds several parts.
        if feeding(&[part]).len() >= 2 {
            add(text, &[part], 2, false, true, false);
        }
    }
    add("+# total maximum Life", &[Life], 2, false, true, false);
    add("+# total maximum Energy Shield", &[EnergyShield], 2, false, true, false);
    out
}

/// The sum of what `fed` gives, each source counted once per part it feeds, with the range
/// when one source gives one.
fn sum(fed: &[(&Source, usize)]) -> (f64, Option<(f64, f64)>) {
    let value = fed.iter().map(|(s, n)| s.value * *n as f64).sum();
    let range = fed.iter().any(|(s, _)| s.range.is_some()).then(|| {
        fed.iter().fold((0.0, 0.0), |sum, (s, n)| {
            let (low, high) = s.range.unwrap_or((s.value, s.value));
            (sum.0 + low * *n as f64, sum.1 + high * *n as f64)
        })
    });
    (value, range)
}
