//! The bulk exchange: what sellers ask, in one currency, for one stackable.

use std::collections::HashMap;

use serde_json::{Value, json};

/// Offers kept and shown.
pub const SHOWN: usize = 20;

/// One seller's offer: `give` of the currency we pay for `get` of the item.
#[derive(Debug, Clone, PartialEq)]
pub struct Offer {
    pub seller: String,
    pub give: f64,
    pub get: f64,
    pub stock: u64,
}

impl Offer {
    /// What one item costs in the paying currency.
    pub fn each(&self) -> f64 {
        self.give / self.get
    }
}

/// The offers for one item against one currency, cheapest first, with the median of their
/// prices.
#[derive(Debug, Clone, PartialEq)]
pub struct Bulk {
    /// The currency paid, by its exchange id (`chaos`, `divine`).
    pub have: String,
    pub offers: Vec<Offer>,
    pub median: Option<f64>,
}

/// One row of the list shown: a seller's offer, or the price list's own quote.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Row<'a> {
    Offer(&'a Offer),
    /// The market price of one item in the paying currency, from the price list.
    Market(f64),
}

impl Bulk {
    /// The offers in price order with the market price (when known) merged in at its place; it
    /// goes before the offers that ask the same.
    pub fn rows(&self, market: Option<f64>) -> Vec<Row<'_>> {
        let mut rows: Vec<Row<'_>> = self.offers.iter().map(Row::Offer).collect();
        if let Some(price) = market.filter(|price| price.is_finite() && *price > 0.0) {
            let at = self.offers.iter().position(|offer| offer.each() >= price).unwrap_or(rows.len());
            rows.insert(at, Row::Market(price));
        }
        rows
    }
}

/// The body of a bulk exchange request for online sellers of `want` against `have`.
pub fn request(have: &str, want: &str) -> Value {
    json!({
        "query": { "status": { "option": "online" }, "have": [have], "want": [want] },
        "sort": { "have": "asc" },
        "engine": "new"
    })
}

/// The offer of every single-offer listing of an answer that sells `want` for `have`, cheapest first,
/// at most [`SHOWN`], with their median. `None` when nothing qualifies.
pub fn parse(json: &Value, have: &str, want: &str) -> Option<Bulk> {
    let mut offers: Vec<Offer> = json
        .get("result")?
        .as_object()?
        .values()
        .filter_map(|entry| offer(entry, have, want))
        .collect();
    offers.sort_by(|a, b| a.each().total_cmp(&b.each()));
    if offers.is_empty() {
        return None;
    }
    // The market price weighs every offer by the stock behind it, not only the ones shown.
    let median = weighted_median(&offers);
    offers.truncate(SHOWN);
    Some(Bulk { have: have.to_owned(), offers, median })
}

fn offer(entry: &Value, have: &str, want: &str) -> Option<Offer> {
    let listing = entry.get("listing")?;
    // A listing that trades several things at once has no price of its own for one of them.
    let offers = listing.get("offers")?.as_array()?;
    if offers.len() != 1 {
        return None;
    }
    let found = offers.iter().find_map(|offer| {
        let (pay, item) = (offer.get("exchange")?, offer.get("item")?);
        if pay.get("currency")?.as_str()? != have || item.get("currency")?.as_str()? != want {
            return None;
        }
        let (give, get) = (pay.get("amount")?.as_f64()?, item.get("amount")?.as_f64()?);
        (give > 0.0 && get > 0.0).then(|| (give, get, item.get("stock").and_then(Value::as_u64).unwrap_or(0)))
    })?;
    Some(Offer {
        seller: listing.pointer("/account/name").and_then(Value::as_str).unwrap_or_default().to_owned(),
        give: found.0,
        get: found.1,
        stock: found.2,
    })
}

/// The middle of the values; the mean of the two middle ones when there is an even number.
pub fn median(values: &[f64]) -> Option<f64> {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    match sorted.len() {
        0 => None,
        n if n % 2 == 1 => Some(sorted[middle]),
        _ => Some((sorted[middle - 1] + sorted[middle]) / 2.0),
    }
}

/// The price at which half of the stock on offer is cheaper, over offers sorted cheapest first.
/// An offer with no stock counts as one.
pub fn weighted_median(offers: &[Offer]) -> Option<f64> {
    let weight = |offer: &Offer| offer.stock.max(1) as f64;
    let total: f64 = offers.iter().map(weight).sum();
    let mut seen = 0.0;
    for offer in offers {
        seen += weight(offer);
        if seen * 2.0 >= total {
            return Some(offer.each());
        }
    }
    None
}

/// The exchange id of every name of the site's static data (`Orb of Alchemy` to `alch`), keyed by
/// the lowercased name.
pub fn ids(json: &Value) -> HashMap<String, String> {
    let groups = json.get("result").and_then(Value::as_array).into_iter().flatten();
    let entries = groups
        .flat_map(|group| group.get("entries").and_then(Value::as_array).into_iter().flatten())
        .filter_map(|entry| Some((entry.get("text")?.as_str()?.to_lowercase(), entry.get("id")?.as_str()?.to_owned())))
        .filter(|(_, id)| id != "sep");
    let mut ids = HashMap::new();
    for (text, id) in entries {
        // Several maps share a text (`Map (Tier 16)` is a plain map, a blighted one, a
        // memory-influenced one...): the blighted kinds are named as the game names them, and the
        // plain text stays the first entry's, the plain map's.
        for (prefix, name) in [("blighted-map-", "blighted "), ("blight-ravaged-map-", "blight-ravaged ")] {
            if id.starts_with(prefix) {
                ids.entry(format!("{name}{text}")).or_insert_with(|| id.clone());
            }
        }
        ids.entry(text).or_insert(id);
    }
    ids
}

/// Where the game's content network serves the images the static data names.
pub const IMAGE_HOST: &str = "https://web.poecdn.com";

/// The address of an image of the static data (`/gen/image/...png?...`, relative to the host).
pub fn image_url(path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_owned();
    }
    format!("{IMAGE_HOST}/{}", path.trim_start_matches('/'))
}

/// The image of every entry of the static data, as `(exchange id, image path)`, keyed by the id
/// and by the lowercased name alike. Entries with no image are left out.
pub fn images(json: &Value) -> HashMap<String, (String, String)> {
    let groups = json.get("result").and_then(Value::as_array).into_iter().flatten();
    let mut found = HashMap::new();
    for entry in groups.flat_map(|group| group.get("entries").and_then(Value::as_array).into_iter().flatten()) {
        let (Some(id), Some(image)) = (entry.get("id").and_then(Value::as_str), entry.get("image").and_then(Value::as_str)) else {
            continue;
        };
        if id == "sep" {
            continue;
        }
        if let Some(text) = entry.get("text").and_then(Value::as_str) {
            found.entry(text.to_lowercase()).or_insert_with(|| (id.to_owned(), image.to_owned()));
        }
        found.insert(id.to_lowercase(), (id.to_owned(), image.to_owned()));
    }
    found
}
