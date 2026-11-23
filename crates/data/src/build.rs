//! Builds a pack from the three lists of the trade site.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Deserialize;
use serde_json::Value;

use crate::classes::CLASSES;
use crate::pack::Pack;

/// Which list could not be used, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildError {
    Stats(String),
    Items(String),
    Filters(String),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stats(reason) => write!(f, "stats list: {reason}"),
            Self::Items(reason) => write!(f, "items list: {reason}"),
            Self::Filters(reason) => write!(f, "filters list: {reason}"),
        }
    }
}

impl std::error::Error for BuildError {}

/// The outer shape the three lists share.
#[derive(Deserialize)]
struct Listing<G> {
    result: Vec<G>,
}

#[derive(Deserialize)]
struct Group<E> {
    id: String,
    #[serde(default = "Vec::new")]
    entries: Vec<E>,
}

#[derive(Deserialize)]
struct Stat {
    id: Option<String>,
    text: Option<String>,
}

#[derive(Deserialize)]
struct Entry {
    /// The base type, or the key of a variant when `text` is there too.
    #[serde(rename = "type")]
    base: Option<String>,
    /// Only unique items have a name.
    name: Option<String>,
    /// What the site displays when that is not the base type alone.
    text: Option<String>,
    /// The discriminator of a variant: what tells a transfigured gem from its base.
    disc: Option<String>,
}

#[derive(Deserialize)]
struct FilterGroup {
    #[serde(default)]
    filters: Vec<Filter>,
}

#[derive(Deserialize)]
struct Filter {
    id: Option<String>,
    option: Option<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    #[serde(default)]
    options: Vec<Opt>,
}

#[derive(Deserialize)]
struct Opt {
    /// A text for most options, null for "Any", a number in some filters.
    #[serde(default)]
    id: Value,
}

/// Builds the pack from the bodies of the `stats`, `items` and `filters`
/// lists. `built` is the time of the build (ISO 8601, UTC).
///
/// A list that is not JSON, has no `result`, or holds nothing usable is an
/// error: an empty pack must never replace a good one. One entry missing a
/// field is skipped.
pub fn build(
    stats_json: &str,
    items_json: &str,
    filters_json: &str,
    built: &str,
) -> Result<Pack, BuildError> {
    let stats = stats(stats_json).map_err(BuildError::Stats)?;
    let (bases, uniques, gems, blight_areas) = items(items_json).map_err(BuildError::Items)?;
    let known = category_ids(filters_json).map_err(BuildError::Filters)?;
    let categories = CLASSES
        .iter()
        .filter(|(_, id)| known.contains(*id))
        .map(|(class, id)| ((*class).to_owned(), (*id).to_owned()))
        .collect();
    Ok(Pack {
        built: built.to_owned(),
        stats,
        bases,
        uniques,
        gems,
        blight_areas,
        categories,
    })
}

fn parse<'a, G: Deserialize<'a>>(json: &'a str) -> Result<Vec<G>, String> {
    serde_json::from_str::<Listing<G>>(json)
        .map(|listing| listing.result)
        .map_err(|error| error.to_string())
}

/// Kind, then text, to the ids carrying that text, in the site's order.
fn stats(json: &str) -> Result<BTreeMap<String, BTreeMap<String, Vec<String>>>, String> {
    let mut kinds: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    let mut found = false;
    for group in parse::<Group<Stat>>(json)? {
        let texts = kinds.entry(group.id).or_default();
        for stat in group.entries {
            let (Some(id), Some(text)) = (stat.id, stat.text) else {
                continue;
            };
            let ids = texts.entry(text).or_default();
            if !ids.contains(&id) {
                ids.push(id);
            }
            found = true;
        }
    }
    if found {
        Ok(kinds)
    } else {
        Err("no stat in it".to_owned())
    }
}

type Bases = BTreeMap<String, Vec<String>>;
type Uniques = Vec<(String, String)>;
type Gems = BTreeMap<String, (String, String)>;

/// Base type names per group, and the unique items, each sorted and
/// without repeats.
///
/// An entry with a name is a unique item; its other variants (`disc`)
/// repeat the same name and base. An entry with a text and no name is a
/// variant whose `type` is a key rather than a name, and is left out.
fn items(json: &str) -> Result<(Bases, Uniques, Gems, BTreeMap<String, String>), String> {
    let mut bases: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut uniques: BTreeSet<(String, String)> = BTreeSet::new();
    let mut gems = Gems::new();
    let mut areas = BTreeMap::new();
    for group in parse::<Group<Entry>>(json)? {
        let is_gem = group.id == "gem";
        let names = bases.entry(group.id).or_default();
        for entry in group.entries {
            let Some(base) = entry.base else {
                continue;
            };
            match (entry.name, entry.text) {
                (Some(name), _) => {
                    uniques.insert((name, base));
                }
                (None, None) => {
                    names.insert(base);
                }
                (None, Some(text)) => {
                    if let Some(area) = text.strip_prefix("Blighted Map (").and_then(|rest| rest.strip_suffix(')')) {
                        areas.insert(area.to_owned(), base);
                    } else if let (true, Some(disc)) = (is_gem, entry.disc) {
                        gems.insert(text, (base, disc));
                    }
                }
            }
        }
    }
    if uniques.is_empty() && bases.values().all(BTreeSet::is_empty) {
        return Err("no item in it".to_owned());
    }
    let bases = bases
        .into_iter()
        .map(|(group, names)| (group, names.into_iter().collect()))
        .collect();
    Ok((bases, uniques.into_iter().collect(), gems, areas))
}

/// The ids the item category filter offers.
fn category_ids(json: &str) -> Result<BTreeSet<String>, String> {
    let ids: BTreeSet<String> = parse::<FilterGroup>(json)?
        .into_iter()
        .flat_map(|group| group.filters)
        .filter(|filter| filter.id.as_deref() == Some("category"))
        .filter_map(|filter| filter.option)
        .flat_map(|choice| choice.options)
        .filter_map(|option| option.id.as_str().map(str::to_owned))
        .collect();
    if ids.is_empty() {
        Err("no item category in it".to_owned())
    } else {
        Ok(ids)
    }
}
