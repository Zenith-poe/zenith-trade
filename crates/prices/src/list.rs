//! The price list itself: read from an overview, searched by name, and kept
//! in a file.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// The kind and the name of the line that gives the divine rate.
const CURRENCY: &str = "Currency";
const DIVINE: &str = "Divine Orb";

/// The form of the saved file. A file of another form is refused, and so
/// fetched again.
const FORMAT: u32 = 1;

/// One price of the overview.
#[derive(Debug, Clone, PartialEq)]
pub struct Price {
    /// The name, exactly as the source gives it.
    pub name: String,
    /// The overview the price comes from, such as `Currency` or
    /// `UniqueArmour`.
    pub kind: Arc<str>,
    /// What tells apart the prices of one name, such as `Simple Robe, 6L`.
    pub variant: Option<String>,
    /// The price of one, in chaos orbs.
    pub chaos: f64,
    /// The price over the last seven days, as percentage change in chaos
    /// orbs from the first day that has data; `None` on a day with no data.
    /// Empty when there is no history.
    pub trend: Vec<Option<f64>>,
}

/// The prices of one league at one moment.
#[derive(Debug, Clone, PartialEq)]
pub struct PriceList {
    pub league: String,
    /// When the overview was downloaded (ISO 8601, UTC), such as
    /// `2025-01-20T10:00:00Z`.
    pub fetched: String,
    /// The price of a divine orb in chaos orbs.
    pub divine: Option<f64>,
    /// In the order of the overview.
    lines: Vec<Price>,
    /// Where the lines of a name are in `lines`.
    by_name: HashMap<String, Vec<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The text is not an overview.
    Format(String),
    /// The text reads as an overview and holds no price.
    Empty,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(reason) => write!(f, "not a price overview: {reason}"),
            Self::Empty => write!(f, "the price overview holds no price"),
        }
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileError {
    /// There is no file at the path.
    Missing,
    /// The file could not be read or written.
    Io(String),
    /// The file is not a price list.
    Format(String),
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => write!(f, "no price list on disk"),
            Self::Io(reason) => write!(f, "price list file: {reason}"),
            Self::Format(reason) => write!(f, "not a price list: {reason}"),
        }
    }
}

impl std::error::Error for FileError {}

// ---- the overview as the source gives it ------------------------------------

#[derive(Deserialize)]
struct Answer {
    #[serde(rename = "currencyOverviews", default)]
    currency: Vec<Overview>,
    #[serde(rename = "itemOverviews", default)]
    items: Vec<Overview>,
}

#[derive(Deserialize)]
struct Overview {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    lines: Vec<Line>,
    /// Which lines, by position, are traded for divine orbs more than for
    /// chaos orbs: the graph of such a line is its price in divine orbs.
    #[serde(rename = "divineTraded", default)]
    divine_traded: Option<Vec<usize>>,
}

#[derive(Deserialize)]
struct Line {
    name: Option<String>,
    variant: Option<String>,
    chaos: Option<f64>,
    /// Seven daily values, as percentage change from the first, `null` where
    /// there is no data; or nothing at all.
    #[serde(default)]
    graph: Option<Vec<Option<f64>>>,
}

impl Line {
    /// The price, when it is one that can be shown.
    fn price(&self) -> Option<f64> {
        self.chaos.filter(|chaos| chaos.is_finite() && *chaos > 0.0)
    }
}

/// Reads the answer of the source into a list for `league`, stamped
/// `fetched`.
///
/// The source gives the currency kinds twice, once in each of its lists: a
/// price is kept once per kind, name and variant. A line with no name or no
/// price above zero is left out. `divine` is the chaos value of the currency
/// named `Divine Orb`.
pub fn parse(body: &str, league: &str, fetched: &str) -> Result<PriceList, ParseError> {
    let answer: Answer =
        serde_json::from_str(body).map_err(|error| ParseError::Format(error.to_string()))?;
    let overviews: Vec<Overview> = answer.currency.into_iter().chain(answer.items).collect();

    let divine_line = overviews
        .iter()
        .filter(|overview| overview.kind == CURRENCY)
        .flat_map(|overview| &overview.lines)
        .find(|line| line.name.as_deref() == Some(DIVINE) && line.price().is_some());
    let divine = divine_line.and_then(Line::price);
    let divine_trend = divine_line
        .and_then(|line| line.graph.clone())
        .unwrap_or_default();

    let mut list = PriceList::new(league, fetched, divine);
    for overview in overviews {
        let kind: Arc<str> = Arc::from(overview.kind.as_str());
        let in_divine: HashSet<usize> = overview
            .divine_traded
            .unwrap_or_default()
            .into_iter()
            .collect();
        for (at, line) in overview.lines.into_iter().enumerate() {
            let Some(chaos) = line.price() else {
                continue;
            };
            let Some(name) = line.name.filter(|name| !name.is_empty()) else {
                continue;
            };
            let graph = line.graph.unwrap_or_default();
            let trend = if in_divine.contains(&at) {
                in_chaos(&graph, &divine_trend)
            } else {
                graph
            };
            list.push(Price {
                name,
                kind: Arc::clone(&kind),
                variant: line.variant,
                chaos,
                trend,
            });
        }
    }
    if list.lines.is_empty() {
        return Err(ParseError::Empty);
    }
    list.shrink();
    Ok(list)
}

/// A trend given in divine orbs as a trend in chaos orbs, with the trend of
/// the divine orb itself. A day is kept when both have data for it. Empty
/// when the two cannot be put together.
fn in_chaos(trend: &[Option<f64>], divine: &[Option<f64>]) -> Vec<Option<f64>> {
    if trend.len() != divine.len() {
        return Vec::new();
    }
    let values: Vec<Option<f64>> = trend
        .iter()
        .zip(divine)
        .map(|(own, rate)| Some((100.0 + (*own)?) * (100.0 + (*rate)?)))
        .collect();
    let Some(first) = values.iter().flatten().copied().next() else {
        return Vec::new();
    };
    if first <= 0.0 {
        return Vec::new();
    }
    values
        .into_iter()
        .map(|value| value.map(|value| hundredths((value / first - 1.0) * 100.0)))
        .collect()
}

/// Rounded to two decimals, as the source writes its own values.
fn hundredths(value: f64) -> f64 {
    // Adding zero turns a negative zero into zero.
    (value * 100.0).round() / 100.0 + 0.0
}

/// A trend as chaos amounts: one value per day, `None` where the trend has
/// none. The current price goes to the last day that has data, and the days
/// before it follow from the percentages. A trend that ends at nothing
/// (-100%) gives the current price alone.
pub fn sparkline(trend: &[Option<f64>], current_chaos: f64) -> Vec<Option<f64>> {
    let Some(last_at) = trend.iter().rposition(Option::is_some) else {
        return vec![None; trend.len()];
    };
    let base = 100.0 + trend[last_at].unwrap_or_default();
    trend
        .iter()
        .enumerate()
        .map(|(at, value)| match value {
            Some(_) if at == last_at => Some(current_chaos),
            Some(value) if base > 0.0 => Some(current_chaos * (100.0 + value) / base),
            _ => None,
        })
        .collect()
}

// ---- the saved file ---------------------------------------------------------

/// The file: the lines of each kind in turn, each line as name, variant,
/// chaos and trend, with no field names.
#[derive(Serialize)]
struct SavedRef<'a> {
    format: u32,
    league: &'a str,
    fetched: &'a str,
    divine: Option<f64>,
    kinds: Vec<(&'a str, Vec<SavedLineRef<'a>>)>,
}

#[derive(Serialize)]
struct SavedLineRef<'a>(&'a str, Option<&'a str>, f64, &'a [Option<f64>]);

#[derive(Deserialize)]
struct Saved {
    format: u32,
    league: String,
    fetched: String,
    divine: Option<f64>,
    kinds: Vec<(String, Vec<SavedLine>)>,
}

#[derive(Deserialize)]
struct SavedLine(String, Option<String>, f64, Vec<Option<f64>>);

impl PriceList {
    fn new(league: &str, fetched: &str, divine: Option<f64>) -> PriceList {
        PriceList {
            league: league.to_owned(),
            fetched: fetched.to_owned(),
            divine,
            lines: Vec::new(),
            by_name: HashMap::new(),
        }
    }

    /// Adds a price, unless one of the same kind, name and variant is there.
    fn push(&mut self, price: Price) {
        let at = self.by_name.entry(price.name.clone()).or_default();
        let known = at.iter().any(|&at| {
            let kept = &self.lines[at];
            kept.kind == price.kind && kept.variant == price.variant
        });
        if !known {
            at.push(self.lines.len());
            self.lines.push(price);
        }
    }

    /// Every price of exactly this name, whatever its kind and variant, in
    /// the order of the overview. Empty when the name is not listed.
    pub fn find(&self, name: &str) -> Vec<&Price> {
        self.by_name
            .get(name)
            .map(|at| at.iter().map(|&at| &self.lines[at]).collect())
            .unwrap_or_default()
    }

    /// Every price, in the order of the overview.
    pub fn prices(&self) -> &[Price] {
        &self.lines
    }

    /// How many prices the list holds.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Seconds from `fetched` to `now`: zero when `fetched` is after `now`,
    /// `u64::MAX` when `fetched` is not a time, so that such a list counts
    /// as old.
    pub fn age_s(&self, now: SystemTime) -> u64 {
        let Some(fetched) = unix_seconds(&self.fetched) else {
            return u64::MAX;
        };
        let now = now
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        now.saturating_sub(fetched)
    }

    /// An amount of chaos orbs in divine orbs, when the list has the rate.
    pub fn to_divine(&self, chaos: f64) -> Option<f64> {
        self.divine
            .filter(|rate| *rate > 0.0)
            .map(|rate| chaos / rate)
    }

    /// Reads the list at `path`.
    pub fn load(path: &Path) -> Result<PriceList, FileError> {
        let text = fs::read_to_string(path).map_err(|error| match error.kind() {
            ErrorKind::NotFound => FileError::Missing,
            _ => FileError::Io(error.to_string()),
        })?;
        let saved: Saved =
            serde_json::from_str(&text).map_err(|error| FileError::Format(error.to_string()))?;
        // The raw text is not kept: only the parsed list is.
        drop(text);
        if saved.format != FORMAT {
            return Err(FileError::Format(format!(
                "form {} is not the one this version reads ({FORMAT})",
                saved.format
            )));
        }
        let mut list = PriceList::new(&saved.league, &saved.fetched, saved.divine);
        for (kind, lines) in saved.kinds {
            let kind: Arc<str> = Arc::from(kind);
            for SavedLine(name, variant, chaos, trend) in lines {
                list.push(Price {
                    name,
                    kind: Arc::clone(&kind),
                    variant,
                    chaos,
                    trend,
                });
            }
        }
        list.shrink();
        Ok(list)
    }

    /// Gives back the room the lists were grown with.
    fn shrink(&mut self) {
        self.lines.shrink_to_fit();
        for price in &mut self.lines {
            price.trend.shrink_to_fit();
        }
        self.by_name.shrink_to_fit();
        for at in self.by_name.values_mut() {
            at.shrink_to_fit();
        }
    }

    /// Writes the list at `path`, creating the folder when needed. The list
    /// goes to a temporary file beside it first (`<name>.tmp`), then takes
    /// the place of the old one, so a failure leaves the old one whole.
    pub fn save(&self, path: &Path) -> Result<(), FileError> {
        let mut kinds: Vec<(&str, Vec<SavedLineRef>)> = Vec::new();
        for price in &self.lines {
            if kinds.last().is_none_or(|(kind, _)| *kind != &*price.kind) {
                kinds.push((&price.kind, Vec::new()));
            }
            if let Some((_, lines)) = kinds.last_mut() {
                lines.push(SavedLineRef(
                    &price.name,
                    price.variant.as_deref(),
                    price.chaos,
                    &price.trend,
                ));
            }
        }
        let text = serde_json::to_string(&SavedRef {
            format: FORMAT,
            league: &self.league,
            fetched: &self.fetched,
            divine: self.divine,
            kinds,
        })
        .map_err(|error| FileError::Format(error.to_string()))?;

        if let Some(folder) = path
            .parent()
            .filter(|folder| !folder.as_os_str().is_empty())
        {
            fs::create_dir_all(folder).map_err(io)?;
        }
        let mut name = path.as_os_str().to_owned();
        name.push(".tmp");
        let temporary = PathBuf::from(name);
        write_whole(&temporary, &text)
            .and_then(|()| fs::rename(&temporary, path))
            .map_err(|error| {
                let _ = fs::remove_file(&temporary);
                io(error)
            })
    }

    /// `%LOCALAPPDATA%\zenith\prices-<league>.json`. Without that variable,
    /// the same under the temporary folder. Letters, digits, spaces, `-`
    /// and `_` of the league name are kept; anything else is written as `%`
    /// and its bytes in hexadecimal, so that two leagues never share a file
    /// and no name leaves the folder.
    pub fn path_for(league: &str) -> PathBuf {
        let mut name = String::from("prices-");
        for character in league.chars() {
            if character.is_alphanumeric() || matches!(character, ' ' | '-' | '_') {
                name.push(character);
            } else {
                let mut bytes = [0; 4];
                for byte in character.encode_utf8(&mut bytes).bytes() {
                    name.push_str(&format!("%{byte:02X}"));
                }
            }
        }
        name.push_str(".json");
        std::env::var_os("LOCALAPPDATA")
            .filter(|root| !root.is_empty())
            .map_or_else(std::env::temp_dir, PathBuf::from)
            .join("zenith")
            .join(name)
    }
}

fn io(error: std::io::Error) -> FileError {
    FileError::Io(error.to_string())
}

/// Writes a file and waits until it is on the disk.
fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

// ---- time -------------------------------------------------------------------

/// Seconds since 1970 of a time written `2025-01-20T10:00:00Z`, with or
/// without a fraction of a second, which is dropped.
fn unix_seconds(text: &str) -> Option<u64> {
    let (date, time) = text.strip_suffix('Z')?.split_once('T')?;
    let mut date = date.splitn(3, '-');
    let year = digits(date.next()?, 4)?;
    let month = digits(date.next()?, 2)?;
    let day = digits(date.next()?, 2)?;
    let (clock, fraction) = time.split_once('.').unwrap_or((time, "0"));
    if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let mut clock = clock.splitn(3, ':');
    let hour = digits(clock.next()?, 2)?;
    let minute = digits(clock.next()?, 2)?;
    let second = digits(clock.next()?, 2)?;
    let in_range = (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && hour < 24
        && minute < 60
        && second <= 60;
    if !in_range {
        return None;
    }
    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second;
    u64::try_from(seconds).ok()
}

/// A number written with exactly `count` digits.
fn digits(text: &str, count: usize) -> Option<i64> {
    (text.len() == count && text.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// Days from 1970-01-01 to a date of the Gregorian calendar (Howard
/// Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}
