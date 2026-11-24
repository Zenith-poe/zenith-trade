//! Blocking client for the trade site's search and fetch endpoints.

use std::fmt;
use std::time::{Duration, Instant};

use serde_json::Value;
use ureq::http::{HeaderMap, Response};
use ureq::{Agent, Body};

use crate::limits::{Limiter, Rule, parse_rules};

/// Where this program can be found; sent in every `User-Agent`.
pub const CONTACT: &str = "https://github.com/Zenith-poe/zenith-trade";

/// The only site the application talks to.
pub const BASE_URL: &str = "https://www.pathofexile.com";

/// Most ids one fetch may ask for.
pub const FETCH_MAX_IDS: usize = 10;

/// Upper bound on one whole exchange, from connecting to the last byte read.
const TIMEOUT: Duration = Duration::from_secs(15);

/// Wait reported for a 429 that says nothing about how long to wait.
const REFUSAL_WAIT: Duration = Duration::from_secs(60);

/// One priced listing of a fetch result.
#[derive(Debug, Clone, PartialEq)]
pub struct Listing {
    pub id: String,
    pub amount: f64,
    pub currency: String,
    /// When the listing was indexed, as the site gives it (ISO 8601, UTC).
    pub indexed: String,
    pub seller: String,
    pub corrupted: bool,
    pub item_level: Option<u32>,
    /// A gem's level and quality, from the item's properties ("20 (Max)", "+20%").
    pub gem_level: Option<u32>,
    pub quality: Option<u32>,
    /// A Facetor's Lens's stored experience.
    pub stored_experience: Option<u64>,
    /// The item's picture on the content network, when the site gives it.
    pub icon: Option<String>,
    /// The tiers of the mods that give each stat, by trade stat id ("explicit.stat_1"): one number
    /// per mod, so a stat made of a tier and of a hybrid partner has two. Empty when the site
    /// gave no mod detail.
    pub tiers: Vec<(String, Vec<u32>)>,
    /// The whole item, to draw it as the game does. `None` when the site gave too little.
    pub item: Option<Box<ListedItem>>,
}

/// The item of a listing as the site describes it: what the game's tooltip shows, in its order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ListedItem {
    pub rarity: ListedRarity,
    /// A rare's or a unique's own name; empty otherwise.
    pub name: String,
    /// The base type line ("Lich's Circlet"); a magic item's whole name.
    pub base: String,
    /// "shaper", "elder", "crusader", "hunter", "redeemer", "warlord", as the site names them.
    pub influences: Vec<String>,
    pub fractured: bool,
    pub synthesised: bool,
    pub corrupted: bool,
    pub mirrored: bool,
    pub split: bool,
    pub identified: bool,
    pub item_level: Option<u32>,
    /// The property lines under the name: "Quality: +20%", "Energy Shield: 412".
    pub properties: Vec<ListedProperty>,
    /// "Level 68, 112 Int": the requirement line as the game writes it, or empty.
    pub requirements: String,
    /// The sockets as the game groups them: "R-G-B B", or empty.
    pub sockets: String,
    /// The mod blocks in the tooltip's order, each one drawn after a separator.
    pub blocks: Vec<Vec<ListedMod>>,
    /// The same blocks as the game draws them without Alt: each block's lines in the site's own
    /// display order, stats already merged (two "% increased Energy Shield" mods are one line),
    /// no mod boundaries. Parallel to `blocks`; empty when built by hand (then the lines of
    /// `blocks` are used).
    pub plain: Vec<Vec<String>>,
}

/// The colour of the item's name and frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListedRarity {
    #[default]
    Normal,
    Magic,
    Rare,
    Unique,
    Gem,
    Currency,
    Other,
}

/// One property line: `label` is empty for a line that is only a value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ListedProperty {
    pub label: String,
    pub value: String,
    /// The value is raised by a mod or by quality: the game draws it in the magic blue.
    pub augmented: bool,
}

/// One mod of the item, which may be several lines of text.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ListedMod {
    pub kind: ListedModKind,
    pub lines: Vec<String>,
    /// What the advanced copy says above it: prefix or suffix, the mod's name, its tier.
    pub affix: Option<Affix>,
    pub name: String,
    pub tier: Option<u32>,
    /// The mod's level (its item level requirement) as the site gives it.
    pub level: Option<u32>,
    /// The roll ranges of each line of `lines`, one entry per number of the line, in order:
    /// "(80-89)", or "(26-32)+(39-42)" when several mods add up to the line; empty for a fixed
    /// value. Parallel to `lines`; may be empty.
    pub ranges: Vec<Vec<String>>,
}

impl ListedMod {
    /// The lines as the game's advanced copy writes them: each range right after its number,
    /// "+87(80-89) to maximum Energy Shield".
    ///
    /// A line that several mods feed (the site gives only the sum, "98% increased ...") is kept
    /// once, under the mod behind most lines, and carries the ranges of every mod joined with
    /// "+" after the number: "98(26-32)+(39-42)%". A mod whose value is fixed adds nothing.
    pub fn ranged_lines(&self) -> Vec<String> {
        self.lines
            .iter()
            .enumerate()
            .map(|(n, text)| {
                let slots = self.ranges.get(n).map(Vec::as_slice).unwrap_or_default();
                with_ranges(text, slots)
            })
            .collect()
    }
}

/// `text` with `slots[j]` put right after its j-th number.
fn with_ranges(text: &str, slots: &[String]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 16);
    let (mut i, mut number) = (0, 0);
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || (chars[i] == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit))) {
                i += 1;
            }
            out.extend(&chars[start..i]);
            if let Some(slot) = slots.get(number) {
                out.push_str(slot);
            }
            number += 1;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// The range slots of one text line from the mods behind it: slot j joins the j-th magnitude of
/// each mod that rolls it within a range.
fn slots_of(mods: &[Value]) -> Vec<String> {
    let count = mods.iter().filter_map(|m| m.get("magnitudes")?.as_array()).map(Vec::len).max().unwrap_or(0);
    (0..count)
        .map(|j| {
            mods.iter()
                .filter_map(|m| {
                    let mag = m.get("magnitudes")?.as_array()?.get(j)?;
                    let text = |key: &str| match mag.get(key)? {
                        Value::String(s) => Some(s.clone()),
                        Value::Number(n) => Some(n.to_string()),
                        _ => None,
                    };
                    let (min, max) = (text("min")?, text("max")?);
                    (min != max).then(|| format!("({min}-{max})"))
                })
                .collect::<Vec<_>>()
                .join("+")
        })
        .collect()
}

/// How many numbers a text line holds, as `with_ranges` counts them.
fn numbers_in(text: &str) -> usize {
    with_ranges(text, &vec!["#".to_owned(); 64]).matches('#').count()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Affix {
    #[default]
    Prefix,
    Suffix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListedModKind {
    Enchant,
    Implicit,
    Fractured,
    #[default]
    Explicit,
    Crafted,
    /// Veiled, scourge, crucible and other lines the game draws apart.
    Other,
}

impl Listing {
    /// The best (lowest) tier among the mods that give `stat`; `None` when the listing says
    /// nothing of that stat.
    pub fn best_tier(&self, stat: &str) -> Option<u32> {
        self.tiers.iter().find(|(id, _)| id == stat).and_then(|(_, tiers)| tiers.iter().copied().min())
    }

    /// Whether the listing has the mod detail at all.
    pub fn has_tiers(&self) -> bool {
        !self.tiers.is_empty()
    }
}

/// The answer to a search: ids to fetch, cheapest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub query_id: String,
    pub ids: Vec<String>,
    pub total: u32,
}

/// How many searches may be sent right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Searches the limits allow from now on, one right after the other.
    pub left: u32,
    /// Seconds until one more is allowed, rounded up; zero unless `left` is zero.
    pub wait_s: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TradeError {
    /// Sending now would break a limit, or the site said to wait. When the
    /// limiter decided, nothing was sent.
    RateLimited { wait_s: u32 },
    /// The site answered with a browser check instead of data.
    Challenge,
    /// The site answered with an error of its own.
    Api { code: i64, message: String },
    /// Any other status that is not a success.
    Http(u16),
    /// The request did not complete, or the answer could not be read.
    Network(String),
}

impl fmt::Display for TradeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RateLimited { wait_s } => write!(f, "rate limited: wait {wait_s} s"),
            Self::Challenge => write!(f, "the trade site asks for a browser check"),
            Self::Api { code, message } => write!(f, "trade site error {code}: {message}"),
            Self::Http(status) => write!(f, "the trade site answered with status {status}"),
            Self::Network(reason) => write!(f, "network error: {reason}"),
        }
    }
}

impl std::error::Error for TradeError {}

/// The limits of one endpoint. Search and fetch are limited separately by
/// the site, so each has its own.
#[derive(Debug, Default)]
struct Gate {
    limiter: Limiter,
    /// End of the wait a 429 asked for.
    refused_until: Option<Instant>,
}

impl Gate {
    fn wait(&self, now: Instant) -> Duration {
        let refusal = self
            .refused_until
            .map_or(Duration::ZERO, |until| until.saturating_duration_since(now));
        self.limiter.wait(now).max(refusal)
    }

    /// Sends one request if the limits allow it and returns the JSON answer.
    fn run(
        &mut self,
        send: impl FnOnce() -> Result<Response<Body>, ureq::Error>,
    ) -> Result<Value, TradeError> {
        let now = Instant::now();
        let wait = self.wait(now);
        if !wait.is_zero() {
            return Err(TradeError::RateLimited {
                wait_s: whole_seconds(wait),
            });
        }
        self.limiter.note_request(now);
        let mut response = send().map_err(|error| TradeError::Network(error.to_string()))?;

        let now = Instant::now();
        self.limiter.update(rules_from(response.headers()), now);

        let status = response.status().as_u16();
        if status == 429 {
            let wait = header(response.headers(), "retry-after")
                .and_then(|text| text.trim().parse::<u64>().ok())
                .map(Duration::from_secs)
                .or_else(|| Some(self.limiter.wait(now)).filter(|wait| !wait.is_zero()))
                .unwrap_or(REFUSAL_WAIT);
            self.refused_until = Some(now + wait);
            return Err(TradeError::RateLimited {
                wait_s: whole_seconds(wait),
            });
        }
        let is_page = header(response.headers(), "content-type").is_some_and(|kind| {
            kind.trim_start()
                .to_ascii_lowercase()
                .starts_with("text/html")
        });
        if status == 403 && is_page {
            return Err(TradeError::Challenge);
        }

        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|error| TradeError::Network(error.to_string()))?;
        let json = serde_json::from_str::<Value>(&text).ok();
        if let Some(error) = json
            .as_ref()
            .and_then(|json| json.get("error"))
            .filter(|e| e.is_object())
        {
            return Err(TradeError::Api {
                code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
                message: text_at(error, "/message"),
            });
        }
        if !(200..300).contains(&status) {
            return Err(TradeError::Http(status));
        }
        json.ok_or_else(|| unexpected("not JSON"))
    }
}

pub struct Client {
    agent: Agent,
    base_url: String,
    search: Gate,
    fetch: Gate,
    exchange: Gate,
    /// The exchange ids of the site's static data, asked for once.
    ids: Option<std::collections::HashMap<String, String>>,
}

impl Client {
    /// `base_url` is [`BASE_URL`] in the application and a local server in tests.
    pub fn new(base_url: &str) -> Self {
        let agent = Agent::config_builder()
            .user_agent(user_agent())
            // Statuses are read here, not turned into errors by the library.
            .http_status_as_error(false)
            // A redirect is never followed: requests go to one host only.
            .max_redirects(0)
            .timeout_global(Some(TIMEOUT))
            .build()
            .into();
        Self {
            agent,
            base_url: base_url.trim_end_matches('/').to_owned(),
            search: Gate::default(),
            fetch: Gate::default(),
            exchange: Gate::default(),
            ids: None,
        }
    }

    /// How many searches the limits allow at `now`: what the tightest rule the site last
    /// reported leaves, with the margin of one request the client keeps on every rule, and
    /// the wait until one more is allowed when none is left. Nothing is sent.
    pub fn search_budget(&self, now: Instant) -> Budget {
        Self::budget_of(&self.search, now)
    }

    /// The same for the bulk exchange, which the site limits on its own.
    pub fn exchange_budget(&self, now: Instant) -> Budget {
        Self::budget_of(&self.exchange, now)
    }

    fn budget_of(gate: &Gate, now: Instant) -> Budget {
        let wait = gate.wait(now);
        if !wait.is_zero() {
            return Budget {
                left: 0,
                wait_s: whole_seconds(wait),
            };
        }
        Budget {
            // No wait: one request at least may go.
            left: gate.limiter.left(now).max(1),
            wait_s: 0,
        }
    }

    /// The exchange id of an item by its name (`Orb of Alchemy` is `alch`), from the site's
    /// static data, which is fetched once. `None` when the name is not offered on the exchange.
    pub fn exchange_id(&mut self, name: &str) -> Result<Option<String>, TradeError> {
        if self.ids.is_none() {
            let url = format!("{}/api/trade/data/static", self.base_url);
            let mut response = self.agent.get(&url).call().map_err(|error| TradeError::Network(error.to_string()))?;
            let status = response.status().as_u16();
            if !(200..300).contains(&status) {
                return Err(TradeError::Http(status));
            }
            let text = response.body_mut().read_to_string().map_err(|error| TradeError::Network(error.to_string()))?;
            let json = serde_json::from_str::<Value>(&text).map_err(|_| unexpected("not JSON"))?;
            self.ids = Some(crate::exchange::ids(&json));
        }
        Ok(self.ids.as_ref().and_then(|ids| ids.get(&name.to_lowercase())).cloned())
    }

    /// Posts one bulk exchange request for `league`: the cheapest online offers of `want`
    /// paid in `have`. `Ok(None)` when nobody offers it. Never retried.
    pub fn exchange(&mut self, league: &str, have: &str, want: &str) -> Result<Option<crate::exchange::Bulk>, TradeError> {
        let url = format!("{}/api/trade/exchange/{}", self.base_url, encode(league));
        let payload = crate::exchange::request(have, want).to_string();
        let agent = &self.agent;
        let json = self.exchange.run(|| agent.post(&url).content_type("application/json").send(payload.as_str()))?;
        Ok(crate::exchange::parse(&json, have, want))
    }

    /// Posts a search request (see [`crate::query`]) for `league`.
    pub fn search(&mut self, league: &str, body: &Value) -> Result<Found, TradeError> {
        let url = format!("{}/api/trade/search/{}", self.base_url, encode(league));
        let payload = body.to_string();
        let agent = &self.agent;
        let json = self.search.run(|| {
            agent
                .post(&url)
                .content_type("application/json")
                .send(payload.as_str())
        })?;

        let query_id = json
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| unexpected("no id"))?;
        let ids = json
            .get("result")
            .and_then(Value::as_array)
            .ok_or_else(|| unexpected("no result"))?
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        let total = json.get("total").and_then(Value::as_u64).unwrap_or(0);
        Ok(Found {
            query_id: query_id.to_owned(),
            ids,
            total: u32::try_from(total).unwrap_or(u32::MAX),
        })
    }

    /// Gets the listings behind the ids of a search. One call asks for the
    /// first [`FETCH_MAX_IDS`] ids at most; the caller pages through the rest.
    /// Listings without a price are skipped. No ids, no request.
    pub fn fetch(&mut self, query_id: &str, ids: &[String]) -> Result<Vec<Listing>, TradeError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let wanted: Vec<String> = ids
            .iter()
            .take(FETCH_MAX_IDS)
            .map(|id| encode(id))
            .collect();
        let url = format!(
            "{}/api/trade/fetch/{}?query={}",
            self.base_url,
            wanted.join(","),
            encode(query_id)
        );
        let agent = &self.agent;
        let json = self.fetch.run(|| agent.get(&url).call())?;

        let entries = json
            .get("result")
            .and_then(Value::as_array)
            .ok_or_else(|| unexpected("no result"))?;
        Ok(entries.iter().filter_map(listing).collect())
    }
}

pub fn user_agent() -> String {
    format!("zenith/{} (contact: {CONTACT})", env!("CARGO_PKG_VERSION"))
}

/// One entry of a fetch result; `None` when it is gone or has no price.
fn listing(entry: &Value) -> Option<Listing> {
    let price = entry.pointer("/listing/price")?;
    Some(Listing {
        id: entry.get("id")?.as_str()?.to_owned(),
        amount: price.get("amount")?.as_f64()?,
        currency: price.get("currency")?.as_str()?.to_owned(),
        indexed: text_at(entry, "/listing/indexed"),
        seller: text_at(entry, "/listing/account/name"),
        corrupted: entry
            .pointer("/item/corrupted")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        item_level: entry
            .pointer("/item/ilvl")
            .and_then(Value::as_u64)
            .and_then(|level| u32::try_from(level).ok()),
        gem_level: property_number(entry, "Level"),
        quality: property_number(entry, "Quality"),
        stored_experience: property_text(entry, "Stored Experience").and_then(|text| big_number(&text)),
        icon: entry.pointer("/item/icon").and_then(Value::as_str).map(str::to_owned),
        tiers: tiers_of(entry),
        item: item_of(entry),
    })
}

/// The whole item of a fetch entry, as the game's tooltip lists it; `None` when the entry has no
/// base type to draw.
fn item_of(entry: &Value) -> Option<Box<ListedItem>> {
    let item = entry.get("item")?;
    let base = clean(item.get("typeLine")?.as_str()?);
    if base.is_empty() {
        return None;
    }
    let flag = |key: &str| item.get(key).and_then(Value::as_bool).unwrap_or(false);
    let rarity = match item.get("frameType").and_then(Value::as_u64) {
        Some(0) => ListedRarity::Normal,
        Some(1) => ListedRarity::Magic,
        Some(2) => ListedRarity::Rare,
        // 9 is a relic, 10 a supporter foil unique: both are drawn as uniques.
        Some(3 | 9 | 10) => ListedRarity::Unique,
        Some(4) => ListedRarity::Gem,
        Some(5) => ListedRarity::Currency,
        _ => ListedRarity::Other,
    };
    let influences = item
        .get("influences")
        .and_then(Value::as_object)
        .map(|map| map.iter().filter(|(_, on)| on.as_bool() != Some(false)).map(|(name, _)| name.clone()).collect())
        .unwrap_or_default();
    let (blocks, plain) = blocks_of(item);
    Some(Box::new(ListedItem {
        rarity,
        name: item.get("name").and_then(Value::as_str).map(clean).unwrap_or_default(),
        base,
        influences,
        fractured: flag("fractured"),
        synthesised: flag("synthesised"),
        corrupted: flag("corrupted"),
        mirrored: flag("duplicated"),
        split: flag("split"),
        identified: item.get("identified").and_then(Value::as_bool).unwrap_or(true),
        item_level: item.get("ilvl").and_then(Value::as_u64).and_then(|level| u32::try_from(level).ok()),
        properties: item
            .get("properties")
            .and_then(Value::as_array)
            .map(|lines| lines.iter().filter_map(property_of).collect())
            .unwrap_or_default(),
        requirements: requirements_of(item),
        sockets: sockets_of(item),
        blocks,
        plain,
    }))
}

/// Drops the site's link markup from a text: "[Armour|Armour Rating]" is "Armour Rating" and
/// "[Cold]" is "Cold".
fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find(']') else { break };
        out.push_str(&rest[..open]);
        let inner = &rest[open + 1..open + close];
        out.push_str(inner.rsplit('|').next().unwrap_or(inner));
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

/// A property line: `{0}` in the name is replaced by the values; a property with no value is a
/// line of its own ("Two Handed Sword").
fn property_of(property: &Value) -> Option<ListedProperty> {
    let name = clean(property.get("name")?.as_str()?);
    let values: Vec<(String, bool)> = property
        .get("values")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|pair| {
                    let text = clean(pair.get(0)?.as_str()?);
                    Some((text, pair.get(1).and_then(Value::as_u64) == Some(1)))
                })
                .collect()
        })
        .unwrap_or_default();
    let augmented = values.iter().any(|(_, aug)| *aug);
    let joined = values.iter().map(|(text, _)| text.as_str()).collect::<Vec<_>>().join(", ");
    if name.contains("{0}") {
        let mut line = name;
        for (i, (text, _)) in values.iter().enumerate() {
            line = line.replace(&format!("{{{i}}}"), text);
        }
        return Some(ListedProperty { label: String::new(), value: line, augmented });
    }
    if values.is_empty() {
        return Some(ListedProperty { label: String::new(), value: name, augmented: false });
    }
    Some(ListedProperty { label: name, value: joined, augmented })
}

/// "Level 68, 112 Int".
fn requirements_of(item: &Value) -> String {
    let Some(lines) = item.get("requirements").and_then(Value::as_array) else {
        return String::new();
    };
    lines
        .iter()
        .filter_map(|line| {
            let name = clean(line.get("name")?.as_str()?);
            let value = clean(line.pointer("/values/0/0")?.as_str()?);
            Some(if name == "Level" { format!("Level {value}") } else { format!("{value} {name}") })
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// "R-G-B B": sockets of a group joined by dashes, groups by spaces.
fn sockets_of(item: &Value) -> String {
    let Some(sockets) = item.get("sockets").and_then(Value::as_array) else {
        return String::new();
    };
    let mut groups: Vec<(u64, Vec<&str>)> = Vec::new();
    for socket in sockets {
        let group = socket.get("group").and_then(Value::as_u64).unwrap_or(0);
        let colour = match (socket.get("sColour").and_then(Value::as_str), socket.get("attr").and_then(Value::as_str)) {
            (Some(colour), _) if !colour.is_empty() => colour,
            (_, Some("S")) => "R",
            (_, Some("D")) => "G",
            (_, Some("I")) => "B",
            (_, Some("G")) => "W",
            (_, Some(other)) => other,
            _ => continue,
        };
        match groups.iter_mut().find(|(id, _)| *id == group) {
            Some((_, list)) => list.push(colour),
            None => groups.push((group, vec![colour])),
        }
    }
    groups.iter().map(|(_, list)| list.join("-")).collect::<Vec<_>>().join(" ")
}

/// One text line of an item's mods before the lines are grouped into mods.
struct ModLine<'a> {
    kind: ListedModKind,
    text: String,
    /// The stat id the extended data is keyed by ("explicit.stat_1").
    hash: Option<String>,
    mods: &'a [Value],
}

/// The mod blocks in the tooltip's order: enchants, implicits, then fractured, explicit and
/// crafted together, then the lines the game draws apart.
fn blocks_of(item: &Value) -> (Vec<Vec<ListedMod>>, Vec<Vec<String>>) {
    let hashes = item.pointer("/extended/hashes");
    let mut blocks = Vec::new();
    let mut plain = Vec::new();
    let mut push = |lines: Vec<ModLine>| {
        let shown: Vec<String> = lines.iter().flat_map(|line| line.text.split('\n').map(str::to_owned)).collect();
        let block = group_mods(lines, hashes);
        if !block.is_empty() {
            blocks.push(block);
            plain.push(shown);
        }
    };
    push(mod_lines(item, "enchantMods", ListedModKind::Enchant));
    push(mod_lines(item, "implicitMods", ListedModKind::Implicit));
    let mut main = mod_lines(item, "fracturedMods", ListedModKind::Fractured);
    main.extend(mod_lines(item, "explicitMods", ListedModKind::Explicit));
    main.extend(mod_lines(item, "craftedMods", ListedModKind::Crafted));
    let rank = |kind: ListedModKind| match kind {
        ListedModKind::Fractured => 0,
        ListedModKind::Crafted => 2,
        _ => 1,
    };
    main.sort_by_key(|line| rank(line.kind));
    push(main);
    for key in ["veiledMods", "scourgedMods", "scourgeMods", "crucibleMods"] {
        push(mod_lines(item, key, ListedModKind::Other));
    }
    (blocks, plain)
}

/// The lines of one mod list. An entry is an object with a `description` (the current answer, whose
/// `domain` tells fractured and crafted lines apart) or a bare string.
fn mod_lines<'a>(item: &'a Value, key: &str, default: ListedModKind) -> Vec<ModLine<'a>> {
    let Some(entries) = item.get(key).and_then(Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            if let Some(text) = entry.as_str() {
                return Some(ModLine { kind: default, text: clean(text), hash: None, mods: &[] });
            }
            let text = clean(entry.get("description")?.as_str()?);
            let domain = entry.get("domain").and_then(Value::as_str).unwrap_or_default();
            let flag = |name: &str| entry.pointer(&format!("/flags/{name}")).and_then(Value::as_bool) == Some(true);
            let kind = if default != ListedModKind::Explicit {
                default
            } else if domain == "fractured" || flag("fractured") {
                ListedModKind::Fractured
            } else if domain == "crafted" || flag("crafted") {
                ListedModKind::Crafted
            } else {
                default
            };
            Some(ModLine {
                kind,
                text,
                hash: entry.get("hash").and_then(Value::as_str).map(|h| h.trim_start_matches("stat.").to_owned()),
                mods: entry.get("mods").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default(),
            })
        })
        .collect()
}

/// Groups the text lines that share a mod index in `extended.hashes` into one mod each (a hybrid
/// mod has two lines). Without index data every line is a mod of its own.
fn group_mods(lines: Vec<ModLine>, hashes: Option<&Value>) -> Vec<ListedMod> {
    // The mod indices of a line, from its stat id; indices are only unique within one kind.
    let indices_of = |line: &ModLine| -> Vec<(String, u64)> {
        let Some(hash) = line.hash.as_deref() else { return Vec::new() };
        let Some(kinds) = hashes.and_then(Value::as_object) else { return Vec::new() };
        let kind = hash.split('.').next().unwrap_or_default();
        kinds
            .get(kind)
            .and_then(Value::as_array)
            .and_then(|pairs| pairs.iter().find(|pair| pair.get(0).and_then(Value::as_str) == Some(hash)))
            .and_then(|pair| pair.get(1)?.as_array())
            .map(|list| list.iter().filter_map(Value::as_u64).map(|i| (kind.to_owned(), i)).collect())
            .unwrap_or_default()
    };
    struct Group {
        keys: Vec<(String, u64)>,
        lines: Vec<usize>,
    }
    let indices: Vec<Vec<(String, u64)>> = lines.iter().map(indices_of).collect();
    let mut groups: Vec<Group> = Vec::new();
    for (n, keys) in indices.iter().enumerate() {
        let hit = |group: &Group| keys.iter().any(|key| group.keys.contains(key));
        let Some(first) = groups.iter().position(hit) else {
            groups.push(Group { keys: keys.clone(), lines: vec![n] });
            continue;
        };
        // A line that links two groups joins them.
        let mut later = Vec::new();
        for at in (first + 1..groups.len()).rev() {
            if hit(&groups[at]) {
                later.push(groups.remove(at));
            }
        }
        let group = &mut groups[first];
        for other in later {
            group.keys.extend(other.keys);
            group.lines.extend(other.lines);
        }
        group.keys.extend(keys.iter().cloned());
        group.lines.push(n);
        group.lines.sort_unstable();
    }
    groups
        .into_iter()
        .map(|group| {
            // Each line's mods pair with its indices in order; the mod behind most lines is the
            // hybrid's own (the other one merely adds to a shared stat line).
            let mut seen: Vec<((String, u64), usize)> = Vec::new();
            let mut best: Option<(usize, &Value)> = None;
            for &n in &group.lines {
                let line = &lines[n];
                if line.mods.len() != indices[n].len() {
                    continue;
                }
                for (key, m) in indices[n].iter().zip(line.mods) {
                    let count = match seen.iter_mut().find(|(k, _)| k == key) {
                        Some((_, count)) => {
                            *count += 1;
                            *count
                        }
                        None => {
                            seen.push((key.clone(), 1));
                            1
                        }
                    };
                    if m.get("name").and_then(Value::as_str).is_some() && best.is_none_or(|(c, _)| count > c) {
                        best = Some((count, m));
                    }
                }
            }
            let (name, tier_text, level) = best
                .map(|(_, m)| {
                    (
                        clean(m.get("name").and_then(Value::as_str).unwrap_or_default()),
                        m.get("tier").and_then(Value::as_str).unwrap_or_default().to_owned(),
                        m.get("level").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()),
                    )
                })
                .unwrap_or_default();
            ListedMod {
                kind: lines[group.lines[0]].kind,
                lines: group
                    .lines
                    .iter()
                    .flat_map(|&n| lines[n].text.split('\n').map(str::to_owned).collect::<Vec<_>>())
                    .collect(),
                affix: match tier_text.chars().next() {
                    Some('P') => Some(Affix::Prefix),
                    Some('S') => Some(Affix::Suffix),
                    _ => None,
                },
                name,
                tier: tier_text.get(1..).and_then(|n| n.parse().ok()).filter(|&n| n > 0),
                level,
                ranges: group
                    .lines
                    .iter()
                    .flat_map(|&n| {
                        // A text of several lines shares its slots in order.
                        let mut slots = slots_of(lines[n].mods).into_iter();
                        lines[n].text.split('\n').map(|part| slots.by_ref().take(numbers_in(part)).collect::<Vec<_>>()).collect::<Vec<_>>()
                    })
                    .collect(),
            }
        })
        .collect()
}

/// The tier of each mod behind each stat of the item, from its detailed mod lines: a line has a
/// `hash` ("stat.explicit.stat_1") and the `mods` that make it, each with a `tier` ("P1", "S3").
fn tiers_of(entry: &Value) -> Vec<(String, Vec<u32>)> {
    let Some(lines) = entry.pointer("/item/explicitMods").and_then(Value::as_array) else {
        return Vec::new();
    };
    lines
        .iter()
        .filter_map(|line| {
            let id = line.get("hash")?.as_str()?.trim_start_matches("stat.").to_owned();
            let tiers = line
                .get("mods")?
                .as_array()?
                .iter()
                .filter_map(|m| {
                    let tier = m.get("tier")?.as_str()?;
                    tier.get(1..)?.parse().ok()
                })
                .collect();
            Some((id, tiers))
        })
        .collect()
}

/// The number a property of the item starts with: `Level` "20 (Max)" is 20, `Quality` "+20%" is 20.
fn property_text(entry: &Value, name: &str) -> Option<String> {
    let properties = entry.pointer("/item/properties")?.as_array()?;
    // Some names hold their value's place: `Stored Experience: {0}`.
    let named = |property: &&Value| {
        property.get("name").and_then(Value::as_str).is_some_and(|text| text == name || text.strip_prefix(name) == Some(": {0}"))
    };
    let property = properties.iter().find(named)?;
    Some(property.pointer("/values/0/0")?.as_str()?.to_owned())
}

/// A number written with separators between the thousands (`350,000,000`, `350.000.000`, `350 000 000`).
fn big_number(text: &str) -> Option<u64> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn property_number(entry: &Value, name: &str) -> Option<u32> {
    let text = property_text(entry, name)?;
    let text = text.as_str();
    let digits: String = text.trim_start_matches('+').chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn text_at(json: &Value, pointer: &str) -> String {
    json.pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn unexpected(what: &str) -> TradeError {
    TradeError::Network(format!("unexpected answer: {what}"))
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}

/// The rules of every set named in `X-Rate-Limit-Rules`, with their state.
fn rules_from(headers: &HeaderMap) -> Vec<Rule> {
    let Some(names) = header(headers, "x-rate-limit-rules") else {
        return Vec::new();
    };
    names
        .split(',')
        .map(|name| name.trim().to_ascii_lowercase())
        .filter(|name| !name.is_empty())
        .flat_map(|name| {
            let limit = header(headers, &format!("x-rate-limit-{name}"));
            let state = header(headers, &format!("x-rate-limit-{name}-state"));
            match (limit, state) {
                (Some(limit), Some(state)) => parse_rules(limit, state),
                _ => Vec::new(),
            }
        })
        .collect()
}

/// Seconds to show for a wait: rounded up, never zero.
fn whole_seconds(wait: Duration) -> u32 {
    let seconds = wait.as_secs() + u64::from(wait.subsec_nanos() > 0);
    u32::try_from(seconds).unwrap_or(u32::MAX).max(1)
}

/// Percent-encodes everything but unreserved characters, for one URL segment.
fn encode(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}
