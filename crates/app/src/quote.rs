//! Prices read from the cached price list: what a stackable is worth with no search at all, and
//! what a unique is worth before its listings arrive. Everything here is pure: the list is read
//! and fetched by the worker.

use zenith_core::{Item, Rarity};
use zenith_prices::{PriceList, sparkline};

use crate::state::{estimate_text, parse_utc};

/// A list this young is not fetched again.
pub const FRESH_S: u64 = 30 * 60;
/// After a failed fetch, no other one before this long.
pub const BACKOFF_S: u64 = 5 * 60;
/// Past this age the list says so in the flag red: the fetches have been failing.
pub const OLD_S: u64 = 24 * 60 * 60;

/// The source's name, as the panel says it.
pub const SOURCE_NAME: &str = "poe.ninja";

/// Shown when a stackable is checked and no price list is on this computer.
pub const NO_PRICE_DATA: &str = "No price data yet";

/// What the price list says of one item.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Quote {
    /// The price of one, in chaos: the cheapest and the dearest of the lines of its name (more
    /// than one for a unique that has variants). `None` when the list does not know the name.
    pub each: Option<(f64, f64)>,
    /// The seven days as chaos amounts; empty when the name has several lines.
    pub trend: Vec<Option<f64>>,
    /// The price of a divine orb in chaos, when the list has it.
    pub divine: Option<f64>,
    /// When the list was fetched, in seconds since 1970; zero when that is not known.
    pub fetched: i64,
}

impl Quote {
    pub fn age_s(&self, now: i64) -> u64 {
        u64::try_from(now - self.fetched).unwrap_or(0)
    }
}

/// How the list is asked for an item: by its name line, or by its only line.
pub fn name_of(item: &Item) -> &str {
    item.name.as_deref().unwrap_or(&item.base)
}

/// Whether the item is priced from the list alone: a currency or a divination card, whatever
/// the list holds, and a plain item (a fragment, a scarab) that the list knows by name.
pub fn is_stackable(item: &Item, list: Option<&PriceList>) -> bool {
    match item.rarity {
        Rarity::Currency | Rarity::DivinationCard => true,
        Rarity::Normal | Rarity::Other(_) | Rarity::None => {
            list.is_some_and(|list| !list.find(name_of(item)).is_empty())
        }
        _ => false,
    }
}

/// The quote of an item. Without a list there is none; with one that lacks the name, the quote
/// has no price but still carries the divine rate.
pub fn quote_of(item: &Item, list: Option<&PriceList>) -> Option<Quote> {
    let list = list?;
    let found = list.find(name_of(item));
    let each = found.iter().map(|price| price.chaos).fold(None, |range: Option<(f64, f64)>, chaos| {
        Some(range.map_or((chaos, chaos), |(low, high)| (low.min(chaos), high.max(chaos))))
    });
    // Every price is in chaos, so the chaos orb itself is not a line of the overview: it is one.
    let each = each.or_else(|| (name_of(item) == "Chaos Orb").then_some((1.0, 1.0)));
    let trend = match found.as_slice() {
        [only] => sparkline(&only.trend, only.chaos),
        _ => Vec::new(),
    };
    Some(Quote { each, trend, divine: list.divine, fetched: parse_utc(&list.fetched).unwrap_or(0) })
}

/// "42 min old", and whether that is old enough for the flag red. Under a minute is "1 min".
pub fn age_text(age_s: u64) -> (String, bool) {
    let text = match age_s {
        0..3600 => format!("{} min old", (age_s / 60).max(1)),
        3600..86_400 => format!("{} h old", age_s / 3600),
        86_400..172_800 => "1 day old".to_owned(),
        _ => format!("{} days old", age_s / 86_400),
    };
    (text, age_s > OLD_S)
}

/// An amount of chaos as the panel writes it: in divine from one divine up, when the rate is
/// known, in chaos otherwise.
pub fn amount_text(chaos: f64, divine: Option<f64>) -> (String, &'static str) {
    match divine.filter(|rate| *rate > 0.0) {
        Some(rate) if chaos >= rate => (estimate_text(chaos / rate), "div"),
        _ => (estimate_text(chaos), "chaos"),
    }
}

/// The price of one as a range when the cheapest and the dearest differ: `12-30`, in divine only when even the cheapest is
/// a divine or more.
pub fn range_text(range: (f64, f64), divine: Option<f64>) -> (String, &'static str) {
    let (low, high) = range;
    let (_, unit) = amount_text(low, divine);
    let scale = if unit == "div" { divine.unwrap_or(1.0) } else { 1.0 };
    let (low_text, high_text) = (estimate_text(low / scale), estimate_text(high / scale));
    if low_text == high_text { (high_text, unit) } else { (format!("{low_text}-{high_text}"), unit) }
}

/// What a whole stack is worth, when it holds more than one.
pub fn stack_total(each: f64, stack: Option<(u32, u32)>) -> Option<f64> {
    stack.filter(|(held, _)| *held > 1).map(|(held, _)| each * f64::from(held))
}

/// "27": how many of the item one divine buys; only for an item that costs less than one divine.
pub fn divine_buys(each: f64, divine: Option<f64>) -> Option<String> {
    let rate = divine.filter(|rate| *rate > 0.0)?;
    (each > 0.0 && each < rate).then(|| {
        let count = rate / each;
        if count >= 10.0 { format!("{}", count.floor()) } else { estimate_text((count * 10.0).floor() / 10.0) }
    })
}

/// The other currency of an estimate from listings: `502 chaos` beside `3.1 div`, and the other
/// way round. Nothing for a currency that is neither, or without the divine rate.
pub fn other_currency(amount: f64, currency: &str, divine: Option<f64>) -> Option<String> {
    let rate = divine.filter(|rate| *rate > 0.0)?;
    match currency {
        "divine" => Some(format!("{} chaos", estimate_text(amount * rate))),
        "chaos" => Some(format!("{} div", estimate_text(amount / rate))),
        _ => None,
    }
}

/// "poe.ninja, 42 min old" and whether it is red.
pub fn source_line(quote: &Quote, now: i64) -> (String, bool) {
    let (age, red) = age_text(quote.age_s(now));
    (format!("{SOURCE_NAME}, {age}"), red)
}

/// The quote as lines of the console report.
pub fn report_lines(item: &Item, quote: &Quote, now: i64) -> Vec<String> {
    let mut lines = Vec::new();
    let divine = quote.divine;
    let Some(range) = quote.each else {
        return lines;
    };
    let say = |chaos: f64| {
        let (text, unit) = amount_text(chaos, divine);
        format!("{text} {unit}")
    };
    if range.0 == range.1 {
        lines.push(format!("price:    {} each", say(range.0)));
        if let Some(total) = stack_total(range.0, item.stack) {
            lines.push(format!("stack:    {} for {}", say(total), item.stack.map_or(0, |(held, _)| held)));
        }
        if let Some(count) = divine_buys(range.0, divine) {
            lines.push(format!("buys:     1 div buys {count}"));
        }
    } else {
        let (text, unit) = range_text(range, divine);
        lines.push(format!("price:    {text} {unit} (cached, the variants differ)"));
    }
    if let Some(rate) = divine {
        lines.push(format!("rate:     1 div = {} chaos", estimate_text(rate)));
    }
    lines.push(format!("source:   {}", source_line(quote, now).0));
    lines
}
