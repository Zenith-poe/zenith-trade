//! The price check itself: the league, one search, then the listings in two fetches. It runs on
//! a thread of its own, since every request blocks, and reports each step: the item as it is
//! read, then where the search stands.
//!
//! The same `Checker` serves the hotkey and "Search again" (through `spawn`) and `--price`
//! (through `price_file`). Nothing here is retried by itself: a refusal ends the check and is
//! shown.
//!
//! The data pack is read here too, on this thread, and built here when a piece of gear is
//! checked and there is none; `--update-data` (through `update_data`) builds it on demand.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering::SeqCst};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui;
use zenith_data::{Pack, PackError};
use zenith_prices::{FileError, PriceList};
use zenith_trade::client::{BASE_URL, Budget, CONTACT, FETCH_MAX_IDS};
use zenith_trade::{Bulk, Client, Listing};

use crate::check::{self, Outcome, Way};
use crate::config::Config;
use crate::panel::icons;
use crate::quote::{self, Quote};
use crate::state::{self, Plan, Step, View};

/// Listings are fetched in this many requests of `FETCH_MAX_IDS` ids: 20 listings at most.
const FETCHES: usize = 2;

/// Upper bound on the whole exchange for the league list.
const LEAGUES_TIMEOUT: Duration = Duration::from_secs(15);

/// Names the league to search in, whatever the league list says.
const LEAGUE_VARIABLE: &str = "ZENITH_LEAGUE";

/// Names the file of the data pack, in place of `Pack::default_path()`.
const PACK_VARIABLE: &str = "ZENITH_PACK";

pub enum Job {
    /// Price the item of this copied text, starting from these pinned settings. `seq` is the
    /// number of the check it belongs to.
    Check { seq: u64, text: String, pins: Config },
    /// Search again for check number `seq`: send this request, as `check::request` made it.
    Search { seq: u64, request: serde_json::Value },
    /// The league was changed in Settings: `None` goes back to the current trade league.
    League(Option<String>),
}

/// A step of check number `seq`, for the view on screen to take.
pub struct Update {
    pub seq: u64,
    pub step: Step,
}

/// How many searches the limits allow, now and as time passes without another request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forecast {
    at: Instant,
    /// Seconds after `at`, and the searches left from then on. The first entry is at zero.
    steps: Vec<(u32, u32)>,
}

/// The panel shows three pips: what is left beyond that is not looked for.
pub const PIPS: u32 = 3;

/// How far ahead a forecast looks, in seconds: past the longest window the site has had.
const FORECAST_HORIZON: u32 = 1800;

impl Forecast {
    /// What a client allows before any answer told it the site's rules: one search.
    pub fn fresh(at: Instant) -> Self {
        Self { at, steps: vec![(0, 1)] }
    }

    /// The budget at `at` and at each later second where it changes, until `PIPS` searches are
    /// left or the horizon is reached.
    pub fn of(client: &Client, at: Instant) -> Self {
        let mut steps: Vec<(u32, u32)> = Vec::new();
        let mut second = 0;
        loop {
            let budget = client.search_budget(at + Duration::from_secs(second.into()));
            let left = budget.left.min(PIPS);
            if steps.last().is_none_or(|(_, before)| *before != left) {
                steps.push((second, left));
            }
            // Past the horizon only to where a wait ends: a forecast never ends on "none left".
            if left >= PIPS || second > FORECAST_HORIZON {
                break;
            }
            // With none left the client says when the next one is; otherwise look second by second.
            second += budget.wait_s.max(1);
        }
        Self { at, steps }
    }

    /// The budget at `now`: what is left, and when none is, the seconds until one is.
    pub fn budget(&self, now: Instant) -> Budget {
        let elapsed = now.saturating_duration_since(self.at).as_secs_f64();
        let reached = self.steps.iter().rposition(|(second, _)| f64::from(*second) <= elapsed).unwrap_or(0);
        let left = self.steps.get(reached).map_or(1, |(_, left)| *left);
        if left > 0 {
            return Budget { left, wait_s: 0 };
        }
        // Nothing ahead says when: past the horizon. A second at a time, then.
        let until = self.steps.get(reached + 1).map_or(elapsed + 1.0, |(second, _)| f64::from(*second));
        Budget { left: 0, wait_s: ((until - elapsed).ceil() as u32).max(1) }
    }

    /// How long until `budget` gives something else; `None` when it no longer changes.
    pub fn next_change(&self, now: Instant) -> Option<Duration> {
        let elapsed = now.saturating_duration_since(self.at).as_secs_f64();
        let next = self.steps.iter().map(|(second, _)| f64::from(*second)).find(|second| *second > elapsed)?;
        // The wait is shown in whole seconds: while there is one, a second at a time.
        let whole = if self.budget(now).left == 0 { 1.0 } else { f64::INFINITY };
        Some(Duration::from_secs_f64((next - elapsed).min(whole).max(0.05)))
    }
}

/// The forecast the worker last made, for whoever draws the budget.
pub type BudgetSlot = Arc<Mutex<Forecast>>;

/// The data pack as the worker last read or built it, for whoever shows an item before the
/// worker gets to it. Empty until the worker has read the file, and for as long as there is none.
pub type PackSlot = Arc<RwLock<Option<Arc<Pack>>>>;

/// The pack in the slot, if any.
pub fn pack_in(slot: &PackSlot) -> Option<Arc<Pack>> {
    slot.read().ok()?.clone()
}

pub fn league_override() -> Option<String> {
    std::env::var(LEAGUE_VARIABLE)
        .ok()
        .map(|league| league.trim().to_owned())
        .filter(|league| !league.is_empty())
        // The league of the Settings window, when the variable names none.
        .or_else(|| crate::CONFIG.lock().ok().and_then(|config| config.league).map(|league| league.as_str().to_owned()))
}

/// Where the data pack is kept: the file `ZENITH_PACK` names, or the one in the data folder.
pub fn pack_path() -> PathBuf {
    std::env::var_os(PACK_VARIABLE).filter(|path| !path.is_empty()).map_or_else(Pack::default_path, PathBuf::from)
}

/// The same identity as the trade client's requests.
fn user_agent() -> String {
    format!("zenith/{} (contact: {CONTACT})", env!("CARGO_PKG_VERSION"))
}

/// Asks the site for its leagues and returns the current trade league.
fn fetch_league(base_url: &str) -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .user_agent(user_agent())
        .http_status_as_error(false)
        .max_redirects(0)
        .timeout_global(Some(LEAGUES_TIMEOUT))
        .build()
        .into();
    let url = format!("{}/api/leagues?type=main&realm=pc", base_url.trim_end_matches('/'));
    let mut response = agent.get(&url).call().map_err(|error| error.to_string())?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(format!("status {status}"));
    }
    let body = response.body_mut().read_to_string().map_err(|error| error.to_string())?;
    state::pick_league(&body).ok_or_else(|| "no current league in the answer".to_owned())
}

/// Gets the three lists of the site, builds a pack stamped with the time `now` and saves it at
/// `path`: three requests, two seconds apart. On any failure the file is left as it was.
fn build_pack(base_url: &str, path: &Path, now: i64) -> Result<Pack, String> {
    let (stats, items, filters) = zenith_data::fetch(base_url, &user_agent()).map_err(|error| error.to_string())?;
    let pack = zenith_data::build(&stats, &items, &filters, &state::utc_text(now)).map_err(|error| error.to_string())?;
    pack.save(path).map_err(|error| error.to_string())?;
    Ok(pack)
}

/// When the file was last written; `None` when there is no file.
fn written(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|file| file.modified()).ok()
}

/// The price list of the league, as the worker holds it.
#[derive(Default)]
struct Prices {
    /// Where lists are fetched from. `None` for a checker that never asks: the tests of the trade
    /// requests, and anything that must stay off the network.
    source: Option<String>,
    /// The file of the list, in place of `PriceList::path_for(league)`.
    file: Option<PathBuf>,
    /// The league `list` is of.
    league: Option<String>,
    list: Option<PriceList>,
    /// When `list`'s file was written, as of the last time it was read or saved.
    written: Option<SystemTime>,
    /// When the last fetch failed: no other before `quote::BACKOFF_S` has passed.
    failed: Option<Instant>,
}

/// Fetches the list of `league` from `source`, parses it and saves it in `path`. A list that
/// cannot be saved is still returned: it is good for this run.
fn fetch_prices(source: &str, league: &str, path: &Path) -> Result<PriceList, String> {
    let body = zenith_prices::fetch(source, league, &user_agent()).map_err(|error| error.to_string())?;
    let list = zenith_prices::parse(&body, league, &state::utc_text(state::now_unix())).map_err(|error| error.to_string())?;
    if let Err(error) = list.save(path) {
        log!("prices: could not save {}: {error}", path.display());
    }
    Ok(list)
}

/// Holds what lasts from one check to the next: the client, whose limits must be remembered, the
/// league and the data pack.
pub struct Checker {
    client: Client,
    base_url: String,
    league: Option<String>,
    pack: PackSlot,
    /// Where the pack is kept. Without a file no pack is ever read or built.
    pack_file: Option<PathBuf>,
    /// When the pack file was written, as of the last time it was read or saved.
    pack_written: Option<SystemTime>,
    budget: BudgetSlot,
    prices: Prices,
    /// What the price list says of the item being checked, for the estimates of its search.
    quote: Option<Quote>,
}

impl Checker {
    /// `league`, when given, is used as it is and the league list is never asked for.
    pub fn new(base_url: &str, league: Option<String>) -> Self {
        Self {
            client: Client::new(base_url),
            base_url: base_url.to_owned(),
            league,
            pack: PackSlot::default(),
            pack_file: None,
            pack_written: None,
            budget: Arc::new(Mutex::new(Forecast::fresh(Instant::now()))),
            prices: Prices::default(),
            quote: None,
        }
    }

    /// Prices stackables and uniques from the cached price list, fetched from `source` when it is
    /// missing or old.
    pub fn with_prices(mut self, source: &str) -> Self {
        self.prices.source = Some(source.to_owned());
        self
    }

    /// The price list is kept in this file, whatever the league.
    #[cfg(test)]
    fn with_price_file(mut self, path: PathBuf) -> Self {
        self.prices.file = Some(path);
        self
    }

    fn price_path(&self, league: &str) -> PathBuf {
        self.prices.file.clone().unwrap_or_else(|| PriceList::path_for(league))
    }

    /// Reads the list of the league from its file, unless it is as it was the last time. Another
    /// league has its own list: the previous league's prices are never used. No request.
    pub fn load_prices(&mut self) {
        let Some(league) = self.league.clone() else {
            return;
        };
        if self.prices.league.as_deref() != Some(&league) {
            self.prices = Prices {
                source: self.prices.source.take(),
                file: self.prices.file.take(),
                league: Some(league.clone()),
                ..Prices::default()
            };
        }
        let path = self.price_path(&league);
        let now = written(&path);
        if now.is_none() || now == self.prices.written {
            return;
        }
        self.prices.written = now;
        match PriceList::load(&path) {
            Ok(list) if list.league == league => self.prices.list = Some(list),
            Ok(_) => log!("prices: {} is of another league", path.display()),
            Err(FileError::Missing) => {}
            Err(error) => log!("prices: {} could not be used: {error}", path.display()),
        }
    }

    /// Fetches the list when there is none or it is older than 30 minutes, and not within five
    /// minutes of a failed attempt; `force` skips both. One request at a time, since the worker
    /// is one thread. Says whether there is a newer list.
    fn refresh_prices(&mut self, force: bool) -> bool {
        let Some(source) = self.prices.source.clone() else {
            return false;
        };
        self.load_league();
        self.load_prices();
        let Some(league) = self.league.clone() else {
            return false;
        };
        if !force {
            let fresh = self.prices.list.as_ref().is_some_and(|list| list.age_s(SystemTime::now()) < quote::FRESH_S);
            let waiting = self.prices.failed.is_some_and(|at| at.elapsed().as_secs() < quote::BACKOFF_S);
            if fresh || waiting {
                return false;
            }
        }
        let path = self.price_path(&league);
        match fetch_prices(&source, &league, &path) {
            Ok(list) => {
                log!("prices: {} prices for {league}", list.len());
                self.prices.written = written(&path);
                self.prices.list = Some(list);
                self.prices.failed = None;
                true
            }
            Err(reason) => {
                log!("prices: could not be fetched: {reason}");
                self.prices.failed = Some(Instant::now());
                false
            }
        }
    }

    /// The live bulk offers for a priced stackable: one request against chaos, and one against
    /// divine when the item is worth a divine or more. A request that the limits do not allow is
    /// not sent; nothing is retried and a failure only leaves the offers out.
    fn bulk_offers(&mut self, check: &crate::check::Check) -> Vec<Bulk> {
        let Some(quote) = check.result.quote.as_ref() else {
            return Vec::new();
        };
        let Some((each, _)) = quote.each else {
            return Vec::new();
        };
        let dear = quote.divine.is_some_and(|rate| rate > 0.0 && each >= rate);
        self.bulk_of(quote::name_of(&check.item), dear)
    }

    /// The live bulk offers for `name`, against chaos and, when it is `dear` (worth a divine or
    /// more), against divine too.
    fn bulk_of(&mut self, name: &str, dear: bool) -> Vec<Bulk> {
        let Some(league) = self.league.clone() else {
            return Vec::new();
        };
        let mut haves = vec!["chaos"];
        if dear {
            haves.push("divine");
        }
        let id = match self.client.exchange_id(name) {
            Ok(Some(id)) => id,
            Ok(None) => return Vec::new(),
            Err(error) => {
                log!("bulk: no exchange id: {error}");
                return Vec::new();
            }
        };
        let mut sets = Vec::new();
        for have in haves {
            if self.client.exchange_budget(Instant::now()).left == 0 {
                log!("bulk: no request left in the budget");
                break;
            }
            match self.client.exchange(&league, have, &id) {
                Ok(Some(bulk)) => sets.push(bulk),
                Ok(None) => {}
                Err(error) => {
                    log!("bulk: {error}");
                    break;
                }
            }
        }
        sets
    }

    /// A check priced on the bulk exchange (a map, see `check::bulk_name`): its outcome, whole.
    fn bulk_outcome(&mut self, name: &str) -> crate::check::Outcome {
        self.load_league();
        if self.league.is_none() {
            return crate::check::Outcome { problem: Some(state::NO_LEAGUE_LIST.into()), quote: self.quote.clone(), ..crate::check::Outcome::default() };
        }
        let bulk = self.bulk_of(name, false);
        let problem = bulk.is_empty().then(|| format!("No bulk offers for {name}"));
        crate::check::Outcome { league: self.league.clone(), quote: self.quote.clone(), bulk, problem, ..crate::check::Outcome::default() }
    }

    /// The view of a copied text with the price list's say in it.
    fn read_view(&self, text: &str, pins: &Config, way: Option<Way>) -> View {
        let view = state::priced_as(state::read_pinned(text, self.pack().as_deref(), pins), way);
        state::quoted(view, self.prices.list.as_ref(), self.league.as_deref())
    }

    /// Where this checker says how many searches are left: renewed after every search.
    pub fn budget_slot(&self) -> BudgetSlot {
        self.budget.clone()
    }

    fn forecast(&self) {
        let forecast = Forecast::of(&self.client, Instant::now());
        if let Ok(mut slot) = self.budget.lock() {
            *slot = forecast;
        }
    }

    /// The data pack is kept in this file: read from it, and built into it when missing.
    pub fn with_pack_file(mut self, path: PathBuf) -> Self {
        self.pack_file = Some(path);
        self
    }

    /// Where this checker puts its pack.
    pub fn pack_slot(&self) -> PackSlot {
        self.pack.clone()
    }

    pub fn pack(&self) -> Option<Arc<Pack>> {
        pack_in(&self.pack)
    }

    fn set_pack(&mut self, pack: Pack) {
        if let Ok(mut slot) = self.pack.write() {
            *slot = Some(Arc::new(pack));
        }
    }

    /// Switches to another league (`None`: the current trade league) and loads its prices.
    pub fn set_league(&mut self, league: Option<String>) {
        self.league = league;
        self.load_league();
        self.load_prices();
        self.refresh_prices(false);
    }

    /// One request for the league list, unless the league is known already.
    pub fn load_league(&mut self) {
        if self.league.is_some() {
            return;
        }
        match fetch_league(&self.base_url) {
            Ok(league) => {
                log!("league: {league}");
                self.league = Some(league);
            }
            Err(reason) => log!("could not load the league list: {reason}"),
        }
    }

    /// Reads the pack file, unless it is as it was the last time: at start, and again before each
    /// check, so that a pack `--update-data` wrote meanwhile is the one used. No request.
    pub fn load_pack(&mut self) {
        let Some(path) = &self.pack_file else {
            return;
        };
        let now = written(path);
        if now.is_none() || now == self.pack_written {
            return;
        }
        // Whatever the file holds, it is not read again until it changes.
        self.pack_written = now;
        match Pack::load(path) {
            Ok(pack) => {
                log!("game data: built {}, from {}", pack.built, path.display());
                self.set_pack(pack);
            }
            Err(PackError::Missing) => {}
            Err(error) => log!("game data: {} could not be used: {error}", path.display()),
        }
    }

    /// Builds the pack from the site's lists and saves it. Says whether there is one now.
    fn update_pack(&mut self) -> bool {
        let Some(path) = self.pack_file.clone() else {
            return false;
        };
        match build_pack(&self.base_url, &path, state::now_unix()) {
            Ok(pack) => {
                log!("game data: built {}, saved in {}", pack.built, path.display());
                self.pack_written = written(&path);
                self.set_pack(pack);
                true
            }
            Err(reason) => {
                log!("game data: could not be built: {reason}");
                false
            }
        }
    }

    /// Prices the item of `text`, handing every state of the check to `emit` as a whole view, the
    /// first one before any request. `alive` is asked before each request: once it says no, the
    /// check is abandoned without a word. `way` is as for `run`.
    pub fn check(&mut self, text: &str, way: Option<Way>, alive: &dyn Fn() -> bool, emit: &mut dyn FnMut(View)) {
        let mut view: Option<View> = None;
        self.run(text, &Config::default(), way, alive, &mut |step| {
            let shown = match (view.take(), step) {
                (None, Step::Read(read)) => read,
                (None, Step::Found(_) | Step::Relaxed(_) | Step::Bulk(_)) => return,
                (Some(mut shown), step) => {
                    shown.take(step);
                    shown
                }
            };
            emit(shown.clone());
            view = Some(shown);
        });
    }

    /// The same check as its steps: the item as it is read, the first time before any request,
    /// then where the search stands. The check starts from `pins`, the settings the player
    /// pinned: its first search asks for them. It prices a piece of gear the way the item opens
    /// on, or the way `way` names.
    pub fn run(&mut self, text: &str, pins: &Config, way: Option<Way>, alive: &dyn Fn() -> bool, emit: &mut dyn FnMut(Step)) {
        self.load_pack();
        self.load_prices();
        let mut view = self.read_view(text, pins, way);
        self.quote = view.check().and_then(|check| check.result.quote.clone());
        emit(Step::Read(view.clone()));

        // An unidentified unique waits for its pick: get the pictures of its candidates, then show
        // the picker again with them.
        if let View::Item(check) = &mut view
            && check.picking()
        {
            let Some(kind) = icons::unique_type(&check.item.class).filter(|_| alive()) else { return };
            let dir = self.price_path(self.league.as_deref().unwrap_or("Standard")).parent().map(Path::to_path_buf).unwrap_or_default();
            let league = self.league.as_deref().unwrap_or("Standard");
            let mut found = icons::unique_icons(&dir, league, kind, &user_agent());
            // A league lists only the uniques sold in it: Standard has nearly all of them.
            if league != "Standard" && check.candidates.iter().any(|name| !found.contains_key(name)) && alive() {
                for (name, icon) in icons::unique_icons(&dir, "Standard", kind, &user_agent()) {
                    found.entry(name).or_insert(icon);
                }
            }
            if !found.is_empty() && alive() {
                check.set_candidate_icons(&found);
                emit(Step::Read(View::Item(check.clone())));
            }
            return;
        }

        // A stackable is only its price: when the list is missing or old, get it before saying
        // more, and show the item again with the new one.
        if view.check().is_some_and(|check| check.plan == Plan::Stack) {
            if alive() && self.refresh_prices(false) {
                view = self.read_view(text, pins, way);
                emit(Step::Read(view.clone()));
            }
            if alive()
                && let Some(bulk) = view.check().map(|check| self.bulk_offers(check)).filter(|bulk| !bulk.is_empty())
            {
                emit(Step::Bulk(bulk));
            }
            return;
        }

        if view.check().is_some_and(|check| check.plan == Plan::NeedsData) {
            // A piece of gear and no data pack: build one first, as `--update-data` does.
            if !alive() {
                return;
            }
            let mut waiting = view.clone();
            if let View::Item(check) = &mut waiting {
                check.result.note = Some(state::LOADING_DATA.into());
            }
            emit(Step::Read(waiting));
            if !self.update_pack() {
                emit(Step::Read(state::without_data(view)));
                return;
            }
            view = self.read_view(text, pins, way);
            emit(Step::Read(view.clone()));
        }
        let Some(request) = view.check().and_then(check::request) else {
            return;
        };
        if let Some(name) = check::bulk_name(&request) {
            if alive() {
                emit(Step::Found(self.bulk_outcome(name)));
            }
            return;
        }
        let mut last = None;
        self.search(&request, alive, &mut |outcome| {
            last = Some(outcome.clone());
            emit(Step::Found(outcome));
        });
        // Nothing found: one more search with the stats on relaxed to "any (n-1) of n".
        let empty = last.is_some_and(|outcome| outcome.total == Some(0) && outcome.listings.is_empty());
        if empty && alive() {
            let left = self.client.search_budget(Instant::now()).left;
            let again = view.check().and_then(|check| check::broaden(check, 0, left).map(|n| (n, check::relaxed_request(check, n))));
            if let Some((n, Some(request))) = again {
                emit(Step::Relaxed(n));
                self.search(&request, alive, &mut |outcome| emit(Step::Found(outcome)));
            }
        }
        // The next check starts from a fresh list.
        if alive() {
            self.refresh_prices(false);
        }
    }

    /// Sends one search and fetches its first listings, handing `emit` where it stands after each
    /// answer. `alive` is asked before each request.
    pub fn search(&mut self, request: &serde_json::Value, alive: &dyn Fn() -> bool, emit: &mut dyn FnMut(Outcome)) {
        let mut draft = Draft::default();
        if !alive() {
            return;
        }
        // The list could not be loaded at startup, or this is `--price`: ask now.
        self.load_league();
        let Some(league) = self.league.clone() else {
            emit(draft.stopped(state::NO_LEAGUE_LIST.into()));
            return;
        };
        draft.league = Some(league.clone());
        draft.quote = self.quote.clone();

        if !alive() {
            return;
        }
        let found = self.client.search(&league, request);
        // Sent or refused, the limits have moved.
        self.forecast();
        let found = match found {
            Ok(found) => found,
            Err(error) => {
                log!("search: {error}");
                emit(draft.stopped(state::problem_for(&error)));
                return;
            }
        };
        log!("search: {} listings, {} ids", found.total, found.ids.len());
        draft.total = Some(found.total);
        draft.query_id = Some(found.query_id.clone());
        if found.ids.is_empty() {
            emit(draft.stopped(state::NO_LISTINGS.into()));
            return;
        }
        emit(draft.outcome(true));

        let pages: Vec<&[String]> = found.ids.chunks(FETCH_MAX_IDS).take(FETCHES).collect();
        for (index, page) in pages.iter().enumerate() {
            if !alive() {
                return;
            }
            match self.client.fetch(&found.query_id, page) {
                Ok(listings) => draft.listings.extend(listings),
                Err(error) => {
                    log!("fetch: {error}");
                    emit(draft.stopped(state::problem_for(&error)));
                    return;
                }
            }
            let last = index + 1 == pages.len();
            if last && draft.listings.is_empty() {
                // Every listing fetched was without a price.
                emit(draft.stopped(state::NO_LISTINGS.into()));
            } else {
                emit(draft.outcome(!last));
            }
        }
    }
}

/// The search in progress.
#[derive(Default)]
struct Draft {
    league: Option<String>,
    listings: Vec<Listing>,
    total: Option<u32>,
    query_id: Option<String>,
    quote: Option<Quote>,
}

impl Draft {
    fn outcome(&self, loading: bool) -> Outcome {
        Outcome {
            league: self.league.clone(),
            listings: self.listings.clone(),
            total: self.total,
            loading,
            estimate: state::estimate_of(&self.listings),
            problem: None,
            note: None,
            bulk: Vec::new(),
            query_id: self.query_id.clone(),
            quote: self.quote.clone(),
        }
    }

    /// The search ends here: what was received stays, with the reason under it.
    fn stopped(&self, problem: String) -> Outcome {
        Outcome { problem: Some(problem), ..self.outcome(false) }
    }
}

/// What `spawn` hands back: where to send jobs, where their steps come from, and where the worker
/// puts its pack and its forecast of the request budget.
pub struct Worker {
    pub jobs: Sender<Job>,
    pub updates: Receiver<Update>,
    pub pack: PackSlot,
    pub budget: BudgetSlot,
}

/// Starts the worker thread. It reads the data pack and asks for the league list once, then
/// waits for jobs. `current` holds the number of the check the panel is showing: a job or an
/// update of any other number is dropped, and no further request is made for it. After each
/// update sent, the panel is asked to repaint.
pub fn spawn(ctx: egui::Context, current: &'static AtomicU64) -> std::io::Result<Worker> {
    let (job_sender, jobs) = channel::<Job>();
    let (updates, update_receiver) = channel::<Update>();
    let mut checker = Checker::new(BASE_URL, league_override()).with_pack_file(pack_path()).with_prices(zenith_prices::SOURCE);
    let (pack, budget) = (checker.pack_slot(), checker.budget_slot());
    std::thread::Builder::new().name("price-check".into()).spawn(move || {
        checker.load_pack();
        if checker.pack().is_none() {
            log!("game data: none yet; the first piece of gear checked builds it, or run zenith --update-data");
        }
        checker.load_league();
        checker.load_prices();
        checker.refresh_prices(false);
        log!("data and prices loaded: {}", crate::memory());
        for job in jobs {
            let seq = match &job {
                Job::Check { seq, .. } | Job::Search { seq, .. } => *seq,
                Job::League(league) => {
                    checker.set_league(league.clone());
                    continue;
                }
            };
            let alive = || current.load(SeqCst) == seq;
            if !alive() {
                continue;
            }
            let mut send = |step: Step| {
                if alive() && updates.send(Update { seq, step }).is_ok() {
                    ctx.request_repaint();
                }
            };
            match job {
                Job::Check { text, pins, .. } => checker.run(&text, &pins, None, &alive, &mut send),
                Job::Search { request, .. } => match check::bulk_name(&request) {
                    Some(name) => send(Step::Found(checker.bulk_outcome(name))),
                    None => checker.search(&request, &alive, &mut |outcome| send(Step::Found(outcome))),
                },
                Job::League(_) => {}
            }
        }
    })?;
    Ok(Worker { jobs: job_sender, updates: update_receiver, pack, budget })
}

/// `--update-prices [league]`: fetches the price list of the league (the current trade league
/// when none is named), saves it, and says what it holds. Returns the exit code: 0 when the
/// list was saved, 1 otherwise.
pub fn update_prices(league: Option<String>) -> i32 {
    let league = league.or_else(league_override).or_else(|| match fetch_league(BASE_URL) {
        Ok(league) => Some(league),
        Err(reason) => {
            println!("could not load the league list: {reason}");
            None
        }
    });
    let Some(league) = league else {
        return 1;
    };
    println!("Downloading the prices of {league} from {}...", quote::SOURCE_NAME);
    let (report, code) = prices_report(zenith_prices::SOURCE, &league, &PriceList::path_for(&league));
    print!("{report}");
    code
}

/// What `update_prices` prints after its first line, with the exit code.
fn prices_report(source: &str, league: &str, path: &Path) -> (String, i32) {
    match fetch_prices(source, league, path) {
        Ok(list) => {
            let rate = list.divine.map_or_else(|| "no divine rate".to_owned(), |rate| format!("1 div = {} chaos", state::estimate_text(rate)));
            (format!("{} prices for {league}, {rate}.\nSaved in {}.\n", list.len(), path.display()), 0)
        }
        Err(reason) => (format!("The prices could not be updated: {reason}.\nThe list already on this computer, if any, is still there.\n"), 1),
    }
}

/// `--update-data`: builds the data pack from the site's lists, saves it, and says on the console
/// what it holds and what changed. Returns the exit code: 0 when the pack was saved, 1 otherwise.
pub fn update_data() -> i32 {
    println!("Downloading the game data from the trade site: three lists, a few seconds...");
    let (report, code) = update_report(BASE_URL, &pack_path(), state::now_unix());
    print!("{report}");
    code
}

/// What `update_data` prints after its first line, with the exit code.
fn update_report(base_url: &str, path: &Path, now: i64) -> (String, i32) {
    let mut report = String::new();
    let old = match Pack::load(path) {
        Ok(pack) => Some(pack),
        Err(PackError::Missing) => None,
        Err(error) => {
            report.push_str(&format!("The previous game data could not be read ({error}): it is replaced.\n"));
            None
        }
    };
    match build_pack(base_url, path, now) {
        Ok(new) => {
            report.push_str(&state::update_summary(old.as_ref(), &new, &path.display().to_string()));
            (report, 0)
        }
        Err(reason) => (
            format!(
                "The game data could not be updated: {reason}.\nNothing was changed: the game data already on this computer, if any, is still there.\n"
            ),
            1,
        ),
    }
}

/// `--price <file>`: the check of the hotkey on the text of a file, written to the console.
/// With `search` off, the item is only read: its mods and the filters a search would use, without
/// any request. `way` is what `--way` asks a piece of gear to be priced as; without it, the way
/// the item opens on. Returns the exit code: 0 when the item was priced (or read), 1 when
/// something stood in the way, 2 when the file could not be read.
pub fn price_file(path: &str, search: bool, way: Option<Way>) -> i32 {
    let text = match std::fs::read(path).map(|bytes| decode(&bytes)) {
        Ok(Some(text)) => text,
        Ok(None) => {
            println!("could not read {path}: neither UTF-8 nor UTF-16 text");
            return 2;
        }
        Err(error) => {
            println!("could not read {path}: {error}");
            return 2;
        }
    };
    let mut checker = Checker::new(BASE_URL, league_override()).with_pack_file(pack_path());
    if search {
        // A stackable is priced from the list, fetched when there is none: it needs the league.
        checker = checker.with_prices(zenith_prices::SOURCE);
        checker.load_league();
    }
    let (report, code) = if search {
        price_text(&mut checker, &text, state::now_unix(), way)
    } else {
        read_text(&mut checker, &text, state::now_unix(), way)
    };
    print!("{report}");
    code
}

/// The panel of the item a file holds, as the program would show it once its search is done:
/// with the data pack and the price list on disk, and a real search. For `--render --view
/// file:<path>`.
#[cfg(feature = "dev")]
pub fn checked_view(path: &str) -> Option<View> {
    let text = std::fs::read(path).ok().and_then(|bytes| decode(&bytes))?;
    let mut checker = Checker::new(BASE_URL, league_override()).with_pack_file(pack_path()).with_prices(zenith_prices::SOURCE);
    checker.load_league();
    let mut last = None;
    checker.check(&text, None, &|| true, &mut |view| last = Some(view));
    last
}

/// The text of a file saved as UTF-8, or as UTF-16 with its byte order mark (what a redirection
/// in Windows PowerShell writes). The mark is dropped.
fn decode(bytes: &[u8]) -> Option<String> {
    let utf16 = |unit: fn([u8; 2]) -> u16| {
        let units: Vec<u16> = bytes[2..].as_chunks::<2>().0.iter().map(|pair| unit(*pair)).collect();
        String::from_utf16(&units).ok()
    };
    match bytes {
        [0xFF, 0xFE, ..] => utf16(u16::from_le_bytes),
        [0xFE, 0xFF, ..] => utf16(u16::from_be_bytes),
        _ => {
            let text = std::str::from_utf8(bytes).ok()?;
            Some(text.strip_prefix('\u{feff}').unwrap_or(text).to_owned())
        }
    }
}

/// The report of a view, which says what a piece of gear is priced as. A way that was asked for
/// an item that has none to choose is said to be left aside.
fn report_of(view: &View, now: i64, way: Option<Way>) -> String {
    let mut report = state::report(view, now);
    if way.is_some() && view.check().is_some_and(|check| !check.has_ways()) {
        report.push_str("way:      none to choose for this item (--way left aside)\n");
    }
    report
}

/// Runs one whole check and returns its last state as text, with the exit code.
fn price_text(checker: &mut Checker, text: &str, now: i64, way: Option<Way>) -> (String, i32) {
    let mut last = None;
    checker.check(text, way, &|| true, &mut |view| last = Some(view));
    match last {
        Some(view) => (report_of(&view, now, way), if view.problem().is_some() { 1 } else { 0 }),
        None => ("problem: nothing was checked\n".to_owned(), 1),
    }
}

/// Reads the item with the pack on disk and returns it as text, with the exit code. No request:
/// a pack that is missing is not built.
fn read_text(checker: &mut Checker, text: &str, now: i64, way: Option<Way>) -> (String, i32) {
    checker.load_pack();
    let view = state::priced_as(state::read(text, checker.pack().as_deref()), way);
    let view = match view.check().map(|check| &check.plan) {
        Some(Plan::NeedsData) => state::without_data(view),
        _ => view,
    };
    let mut report = report_of(&view, now, way);
    let code = if view.problem().is_some() {
        1
    } else {
        report.push_str("search:   not made (--no-search)\n");
        0
    };
    (report, code)
}
