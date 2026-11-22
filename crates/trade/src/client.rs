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
    /// The item's picture on the content network, when the site gives it.
    pub icon: Option<String>,
    /// The tiers of the mods that give each stat, by trade stat id ("explicit.stat_1"): one number
    /// per mod, so a stat made of a tier and of a hybrid partner has two. Empty when the site
    /// gave no mod detail.
    pub tiers: Vec<(String, Vec<u32>)>,
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
        icon: entry.pointer("/item/icon").and_then(Value::as_str).map(str::to_owned),
        tiers: tiers_of(entry),
    })
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
fn property_number(entry: &Value, name: &str) -> Option<u32> {
    let properties = entry.pointer("/item/properties")?.as_array()?;
    let property = properties.iter().find(|property| property.get("name").and_then(Value::as_str) == Some(name))?;
    let text = property.pointer("/values/0/0")?.as_str()?;
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
