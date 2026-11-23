//! The check the panel is showing, as the player edits it: the item, its rows, the filters and
//! options of the search, and what the last search brought. The panel reports what the player
//! does as `Action`s; `apply` is the whole behaviour, pure, and `request` is what a search sends.
//!
//! A piece of gear is priced in one of two ways, each a set of filters and options to start
//! from: as the item it is, or as the base a crafter would buy. See `Way` and `way_set`.
//!
//! A unique is searched by its name. Its mods are filters too, all off to start with: one that
//! is switched on is asked from the value the item has. For any mod whose range is known,
//! `rolls` gives the values its minimum is set to in one gesture.

use crate::quote::Quote;
use zenith_core::matching::{MAP_BONUS, MAP_MODIFIERS, MAP_PACK_SIZE, MAP_QUANTITY, MAP_RARITY, map_pseudo};
use zenith_core::matching::{ARMOUR, BASE_PERCENTILE, ATTACKS_PER_SECOND, BLOCK, CRITICAL_CHANCE, ELEMENTAL_DPS, ENERGY_SHIELD, EVASION, PHYSICAL_DPS, TOTAL_DPS};
use zenith_core::{Estimate, Filter, Influence, Item, Rarity, gem_level, open_affixes, parse_mods, parse_properties};
use zenith_trade::client::{BASE_URL, Budget};
use zenith_trade::{Currency, GearSearch, Listed, Listing, SearchOptions, Status, gear_request, gem_request, unique_request, add_armour_filters, add_weapon_filters};

/// How a row stands in the search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// The search asks for it.
    Searched,
    /// It could be asked for and is not, or nothing is known of it yet.
    Quiet,
    /// The data pack has no stat for it: it cannot be asked for.
    Unmatched,
}

/// A property of the item, or one of its mods: what the panel lists under the estimate.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// What a mod is, as the report names it: `Prefix`, `Crafted Suffix`. `None` for a property.
    pub kind: Option<String>,
    pub tier: Option<u32>,
    /// Where the mod's values sit in the ranges they were rolled in, from 0 to 1. `None` when no
    /// line gives a range.
    pub roll: Option<f32>,
    /// The text, without the roll ranges: one entry per line of the mod. For a property, its
    /// name alone.
    pub lines: Vec<String>,
    /// The value of a property, written after its name.
    pub figure: Option<String>,
    /// That value is the one the item would have at 20% quality, which is how the site compares
    /// it, and not the one it has now: its quality is lower.
    pub q20: bool,
    /// The trade stat of a mod, or the site's name for a property filter.
    pub id: Option<String>,
    /// The filter of `Check::filters` the row switches on and off. `None`: the row cannot be
    /// searched, and a click on it does nothing.
    pub filter: Option<usize>,
    /// The data pack was asked and has no stat for it.
    pub unmatched: bool,
    /// A mod the item can no longer lose.
    pub fractured: bool,
    /// A mod put on the item at the crafting bench.
    pub crafted: bool,
    /// A mod the Foulborn mechanic changed.
    pub foulborn: bool,
    /// What makes the mod special besides the marks above: an influence, an Eldritch implicit, an essence...
    pub origin: Option<zenith_core::Origin>,
    /// An implicit the item got by being corrupted: it decides the price.
    pub corruption: bool,
    /// A display rule keeps the row off the panel (a fixed mod of a unique); the panel's Hidden
    /// switch shows it again.
    pub hidden: bool,
    /// A mod the base, or what was done to it, gives the item: the game writes those apart from
    /// the others.
    pub implicit: bool,
    /// A mod of the pool a unique draws from (Zana's Ingenuity): never hidden.
    pub variant: bool,
    /// The value of a mod as its stat counts it, and the least and the most it could have been
    /// when the item gives the range it was rolled in: see `zenith_core::Matched`.
    pub value: Option<f64>,
    pub range: Option<(f64, f64)>,
}

/// What a search asks for besides the filters of the rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// This base only.
    pub base: bool,
    /// The least item level, quality and number of linked sockets.
    pub item_level: Option<u32>,
    pub quality: Option<u32>,
    pub links: Option<u32>,
    /// Corrupted listings only, or only those that are not; `None` for both.
    pub corrupted: Option<bool>,
    /// The same for fractured listings. No chip shows it: it is part of a way of pricing.
    pub fractured: Option<bool>,
    /// The influences and the synthesis the listings must have: bits of `Influence::bit`.
    pub influences: u16,
    /// Mirrored, split and identified listings only, or only those that are not; `None` for both.
    pub mirrored: Option<bool>,
    pub split: Option<bool>,
    pub identified: Option<bool>,
    /// Foulborn listings only (a Foulborn unique asks for them), or only those that are not.
    pub foulborn: Option<bool>,
    /// Gear with a free prefix, or a free suffix, only.
    pub open_prefix: bool,
    pub open_suffix: bool,
    /// Magic listings only (a magic item starts with it); off, any rarity but unique.
    pub magic: bool,
    /// A map asks the bulk exchange for offers in place of searching listings.
    pub bulk: bool,
    pub status: Status,
    /// `None`: any currency.
    pub currency: Option<Currency>,
    /// `None`: any age.
    pub listed: Option<Listed>,
}

impl Options {
    /// What a check starts from: instant buyout, chaos and divine, three days, corrupted or not
    /// like the item, and nothing else.
    pub fn of(item: &Item) -> Self {
        Self {
            // A map asks for its base, `Map (Tier 16)`, and a cluster jewel for its size.
            base: (item.class == "Maps" && item.rarity != Rarity::Unique) || is_cluster(item),
            // A gem asks for the level and the quality it has.
            item_level: if item.rarity == Rarity::Gem { gem_level(item) } else { None },
            // A gem and a unique ask for the quality they have.
            quality: if matches!(item.rarity, Rarity::Gem | Rarity::Unique) { item.quality.filter(|quality| *quality > 0) } else { None },
            links: None,
            corrupted: Some(item.corrupted),
            fractured: None,
            influences: item.influences.iter().fold(0, |bits, influence| bits | influence.bit()),
            mirrored: item.mirrored.then_some(true),
            split: item.split.then_some(true),
            identified: (!item.identified).then_some(false),
            foulborn: is_foulborn(item).then_some(true),
            open_prefix: false,
            open_suffix: false,
            // An unidentified item shows no mods: it is worth its base, whatever its rarity.
            magic: item.rarity == Rarity::Magic && item.identified,
            bulk: starts_in_bulk(item),
            status: Status::Securable,
            currency: Some(Currency::ChaosDivine),
            listed: Some(Listed::Days3),
        }
    }
}

/// What a piece of gear is priced as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Way {
    /// The item it is: its defences and its best mods.
    Item,
    /// What it is worth to a crafter: its base, its item level and the mods that are fractured,
    /// whatever its other mods are.
    Base,
}

/// The ways in the order the panel offers them.
pub const WAYS: [Way; 2] = [Way::Item, Way::Base];

/// The highest item level a base is asked for: from there on every mod can be rolled.
const BASE_LEVEL: u32 = 86;

/// How the item is searched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// It is not: the outcome says why.
    Nothing,
    /// By its name.
    Unique,
    /// By its category, with the filters that are on. `base` is the base type as the site names
    /// it, when it is known: what the base option asks for.
    Gear { category: String, base: Option<String> },
    /// By its category and its very base, whatever the rarity: an item that has no rarity, as a
    /// viscera. One such item differs from another of the same base by its rolls alone, so its
    /// check starts with the base asked for and every mod asked from the value the item has;
    /// there are no two ways of pricing it.
    Exact { category: String, base: Option<String> },
    /// A piece of gear and no data pack to match its mods with: the pack first, then read again.
    NeedsData,
    /// A stackable, priced from the cached price list: no search is made.
    Stack,
    /// A skill or support gem, by its name: `name` is the gem as the site lists it, and `disc`
    /// tells a transfigured gem from it.
    Gem { name: String, disc: Option<String> },
}

/// What a search has brought so far.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Outcome {
    /// The league searched, once it is known.
    pub league: Option<String>,
    /// Cheapest first, as the site sorts them.
    pub listings: Vec<Listing>,
    /// How many listings the search found, fetched or not.
    pub total: Option<u32>,
    /// More is on its way.
    pub loading: bool,
    pub estimate: Option<Estimate>,
    /// Why there is no price, or why the list stops short.
    pub problem: Option<String>,
    /// What is being waited for before the search, in place of the count of listings.
    pub note: Option<String>,
    /// The site's id of the search, once it has answered: what its page is opened with.
    pub query_id: Option<String>,
    /// What the cached price list says of the item: its price before the listings arrive, the
    /// other currency of the estimate, and a stackable's whole answer.
    pub quote: Option<Quote>,
    /// The live bulk exchange offers of a stackable, one set per currency paid.
    pub bulk: Vec<zenith_trade::Bulk>,
}

/// One item being priced.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub item: Item,
    /// The properties that can be searched, then every mod.
    pub rows: Vec<Row>,
    /// The filters of the search, on or off. A row points at its own.
    pub filters: Vec<Filter>,
    pub options: Options,
    /// What the item is priced as. It is `Way::Item` for anything that has no way to choose.
    pub way: Way,
    /// The filters or the options changed since the search the outcome comes from.
    pub stale: bool,
    /// The filters, by index, whose mod the listings must not have: the request's `not` group.
    /// Such a filter is off.
    pub not: Vec<usize>,
    /// The filters, by index, searched as a pure tier: the hybrid partners of their stat are in the
    /// request's `not` group, and the listings whose mod is of a lower tier step aside.
    pub pure: Vec<usize>,
    /// "Any N": the filters that are on, as a count group asking for this many of them, when
    /// there are at least two. `None`: all of them are asked for.
    pub any: Option<u32>,
    pub plan: Plan,
    pub result: Outcome,
    /// The size of the item's largest group of linked sockets; `None` when it has no socket.
    pub links: Option<u32>,
    /// An unidentified unique: the uniques the data pack knows on its base, to choose from, and
    /// the one chosen. Once one is chosen the search is the one of that unique by its name.
    pub candidates: Vec<String>,
    /// The picture address of each candidate, in the same order; filled where the price list meets the check.
    pub candidate_icons: Vec<Option<String>>,
    pub unique: Option<String>,
    /// The Hidden switch is on: the rows a display rule hides show again, dimmed. Not saved.
    pub reveal: bool,
    /// The menu of the candidates is shown open whatever the panel's own state says: the
    /// `--view` pictures of it.
    pub candidates_open: bool,
    /// The settings let a map be priced on the bulk exchange: without it there is no Bulk chip.
    pub bulk_allowed: bool,
    /// The filters as the item was read: what pricing it as an item starts from.
    defaults: Vec<Filter>,
}

impl Check {
    /// A check as the item is first read, with the filters of `filters`: the options of
    /// `Options::of`, nothing changed, and an outcome that is loading exactly when something is
    /// to follow. A fractured piece of gear is first of all a base: its check opens on the set
    /// of `Way::Base`. An item searched by its very base (`Plan::Exact`) starts with the base
    /// asked for, and with nothing asked of its corruption unless it is corrupted.
    pub fn new(item: Item, rows: Vec<Row>, filters: Vec<Filter>, plan: Plan) -> Self {
        let mut check = Self {
            options: Options::of(&item),
            way: Way::Item,
            defaults: filters.clone(),
            result: Outcome { loading: !matches!(plan, Plan::Nothing | Plan::Stack), ..Outcome::default() },
            links: parse_properties(&item).links,
            candidates: Vec::new(),
            candidate_icons: Vec::new(),
            unique: None,
            reveal: false,
            candidates_open: false,
            bulk_allowed: false,
            item,
            rows,
            filters,
            stale: false,
            not: Vec::new(),
            pure: Vec::new(),
            any: None,
            plan,
        };
        if check.has_ways() && check.item.fractured {
            (check.filters, check.options) = way_set(&check, Way::Base);
            check.way = Way::Base;
        }
        if let Plan::Exact { base, .. } = &check.plan {
            check.options.base = base.is_some();
            check.options.corrupted = check.item.corrupted.then_some(true);
        }
        // The influences say which mods can be on a piece of gear: a unique has them by name.
        if check.plan == Plan::Unique {
            check.options.influences = 0;
        }
        // An unidentified item has no mod to ask for: its base and its level tell it.
        if !check.item.identified {
            if let Plan::Gear { base: Some(_), .. } = &check.plan {
                check.options.base = true;
            }
            // A cluster jewel's level matters only by its steps: offered, not asked.
            if matches!(check.plan, Plan::Gear { .. }) && !is_cluster(&check.item) {
                check.options.item_level = check.item.item_level;
            }
        }
        check.loosen(slack());
        check
    }

    /// Offers these uniques for an unidentified unique, when it is one and there is any: with a
    /// single one, it is the one chosen.
    pub fn offer_uniques(&mut self, names: Vec<String>) {
        if self.item.rarity != Rarity::Unique || self.item.identified || names.is_empty() {
            return;
        }
        self.unique = (names.len() == 1).then(|| names[0].clone());
        // Several: nothing is searched before the player picks one, from the list open at once.
        if names.len() > 1 {
            self.candidates_open = true;
            self.result.loading = false;
        }
        self.candidates = names;
    }

    /// Gives each candidate its picture, by name; one with none keeps a placeholder.
    pub fn set_candidate_icons(&mut self, icons: &std::collections::HashMap<String, String>) {
        self.candidate_icons = self.candidates.iter().map(|name| icons.get(name).cloned()).collect();
    }

    /// Whether the picker is up: an unidentified unique with several candidates and no pick yet.
    pub fn picking(&self) -> bool {
        self.candidates_open && self.unique.is_none() && !self.candidates.is_empty()
    }

    /// Whether the mod behind a filter has decimals: its roll range or its value has a fractional
    /// part. The slider and the wheel go in tenths for it, and in whole numbers for any other.
    /// Whether a display rule keeps the row off the panel: a hidden row, or any row but a
    /// fractured mod on the base view.
    pub fn hides(&self, row: &Row) -> bool {
        let id = row.filter.and_then(|index| self.filters.get(index)).map_or("", |filter| filter.id.as_str());
        // A map shows its very boss on the item view, and any of its family on the base view.
        row.hidden
            || (self.way == Way::Item && is_any_boss(id))
            || (self.way == Way::Base && !kept_on_base(id) && !(self.item.fractured && row.fractured))
    }

    /// How many rows the display rules hide.
    pub fn hidden_count(&self) -> usize {
        self.rows.iter().filter(|row| self.hides(row)).count()
    }

    pub fn fine(&self, filter: usize) -> bool {
        self.rows.iter().any(|row| row.filter == Some(filter) && has_decimals(row.range, row.value))
    }

    /// The minimums of the filters of a mod or a property start `slack` under the value the item
    /// has, as the player chose: not at all when exact. Only for an item searched by its filters:
    /// a unique and an item searched by its base ask for what they have.
    pub fn loosen(&mut self, slack: zenith_core::matching::Slack) {
        if slack == zenith_core::matching::Slack::Exact || !matches!(self.plan, Plan::Gear { .. }) {
            return;
        }
        for filter in self.filters.iter_mut().chain(self.defaults.iter_mut()) {
            filter.min = filter.min.map(|min| slack.lower(min));
        }
    }

    /// The same item read with the same data: the same rows, and the same filters to start from.
    pub fn same_reading(&self, other: &Check) -> bool {
        self.item == other.item && self.rows == other.rows && self.plan == other.plan && self.defaults == other.defaults
    }

    /// There are two ways of pricing it: a piece of gear searched on its filters. A unique is
    /// searched by its name, and an item that has no rarity by its base already.
    pub fn has_ways(&self) -> bool {
        matches!(self.plan, Plan::Gear { .. }) && self.item.identified
    }

    /// What the panel calls a way of pricing this item.
    pub fn way_label(&self, way: Way) -> &'static str {
        match way {
            Way::Item => "Item",
            Way::Base if self.item.fractured => "Fractured base",
            Way::Base => "Base",
        }
    }

    /// The minimum a filter started with in the way the item is priced: what switching it back
    /// on gives it when it has none.
    fn start_min(&self, index: usize) -> Option<f64> {
        way_set(self, self.way).0.get(index).and_then(|filter| filter.min)
    }

    /// Whether the row is the first of those that share its filter, or has the filter alone: the
    /// fields and the slider of a filter are shown once, at that row. A row without a filter leads
    /// nothing.
    pub fn leads(&self, row: &Row) -> bool {
        let Some(filter) = row.filter else { return false };
        self.rows.iter().find(|other| other.filter == Some(filter)).is_some_and(|first| std::ptr::eq(first, row))
    }

    /// The stat filters that are on: what the "and" group, or the count group, holds.
    pub fn stats_on(&self) -> usize {
        self.filters.iter().filter(|filter| filter.on && !is_property(&filter.id)).count()
    }

    /// The least of the stats on that a listing must have, when "Any N" is on and there are
    /// at least two stats to choose among; always from 1 to the number of stats on.
    pub fn any_count(&self) -> Option<u32> {
        let on = self.stats_on() as u32;
        self.any.filter(|_| on >= 2).map(|n| n.clamp(1, on))
    }

    /// Whether the filter at `index` is excluded.
    pub fn excluded(&self, index: usize) -> bool {
        self.not.contains(&index)
    }

    /// Whether the filter at `index` is searched as a pure tier: it is on, and its row has a tier.
    pub fn is_pure(&self, index: usize) -> bool {
        self.pure.contains(&index) && self.filters.get(index).is_some_and(|filter| filter.on && filter.tier.is_some())
    }

    /// Whether the filter at `index` can be searched as a pure tier: a stat of the item with a tier.
    pub fn can_pure(&self, index: usize) -> bool {
        self.can_exclude(index) && !self.excluded(index) && self.filters.get(index).is_some_and(|filter| filter.tier.is_some())
    }

    /// Whether a listing is of a lower tier than a pure filter wants: it has the mod detail, and
    /// its mod for that stat is of a higher tier number than the item's.
    fn other_tier(&self, listing: &Listing) -> bool {
        listing.has_tiers()
            && self.pure.iter().filter(|index| self.is_pure(**index)).any(|index| {
                let filter = &self.filters[*index];
                matches!((listing.best_tier(&filter.id), filter.tier), (Some(found), Some(wanted)) if found > wanted)
            })
    }

    /// The listings that stay once the pure tiers have set aside the others.
    pub fn kept_listings(&self) -> Vec<Listing> {
        self.result.listings.iter().filter(|listing| !self.other_tier(listing)).cloned().collect()
    }

    /// How many fetched listings the pure tiers set aside.
    pub fn other_tier_count(&self) -> usize {
        self.result.listings.iter().filter(|listing| self.other_tier(listing)).count()
    }

    /// Whether the filter at `index` can be excluded: it is a stat that a row of the panel shows.
    pub fn can_exclude(&self, index: usize) -> bool {
        self.filters.get(index).is_some_and(|filter| !is_property(&filter.id)) && self.rows.iter().any(|row| row.filter == Some(index))
    }

    /// The filter a row switches.
    pub fn filter_of(&self, row: &Row) -> Option<&Filter> {
        self.filters.get(row.filter?)
    }

    pub fn mark(&self, row: &Row) -> Mark {
        match self.filter_of(row) {
            Some(filter) if filter.on => Mark::Searched,
            _ if row.unmatched => Mark::Unmatched,
            _ => Mark::Quiet,
        }
    }

    /// The least the search asks of a row, when the row is searched.
    pub fn min_of(&self, row: &Row) -> Option<f64> {
        self.filter_of(row).filter(|filter| filter.on).and_then(|filter| filter.min)
    }

    /// Whether a filter started from its most, not its least: a stat whose less is the better,
    /// as more damage taken.
    pub fn upper(&self, index: usize) -> bool {
        self.defaults.get(index).is_some_and(|filter| filter.min.is_none() && filter.max.is_some())
    }

    /// Whether a filter can be asked for a negative value: it started with one, or has one now.
    /// Everything else is only ever asked for from zero up.
    pub fn signed(&self, index: usize) -> bool {
        let negative = |bound: Option<f64>| bound.is_some_and(|bound| bound < 0.0);
        negative(self.defaults.get(index).and_then(|filter| filter.min.or(filter.max)))
            || self.filters.get(index).is_some_and(|filter| negative(filter.min) || negative(filter.max))
    }

    /// A search can be sent for it.
    /// A map searched on the site, by its properties or on the bulk exchange.
    pub fn is_map(&self) -> bool {
        matches!(&self.plan, Plan::Gear { category, .. } if category == MAP_CATEGORY)
    }

    /// Priced on the bulk exchange: offers, not listings.
    pub fn in_bulk(&self) -> bool {
        self.is_map() && self.options.bulk
    }

    pub fn searchable(&self) -> bool {
        matches!(self.plan, Plan::Unique | Plan::Gear { .. } | Plan::Exact { .. } | Plan::Gem { .. })
    }

    /// No filter that is on asks for the impossible: a number that is none, or a minimum above
    /// its maximum.
    pub fn valid(&self) -> bool {
        self.filters.iter().filter(|filter| filter.on).all(|filter| {
            let finite = |bound: Option<f64>| bound.is_none_or(f64::is_finite);
            let ordered = !matches!((filter.min, filter.max), (Some(min), Some(max)) if min > max);
            finite(filter.min) && finite(filter.max) && ordered
        })
    }

    /// The chips of the item's own options, in the order they are shown: the base, the item
    /// level, the quality, the linked sockets, corrupted or not. Each is there only when the item
    /// has what it asks for. A unique is searched by its name: only its links and its corruption
    /// can be asked for. An item that is not searched has none. An item searched by its very
    /// base has the chip of its corruption only when it is corrupted, or when the search asks
    /// something of it.
    pub fn option_chips(&self) -> Vec<OptionChip> {
        let chosen = &self.options;
        let mut chips = Vec::new();
        if !self.searchable() {
            return chips;
        }
        if self.plan == Plan::Unique
            && let Some(has) = self.item.quality.filter(|quality| *quality > 0)
        {
            let asked = chosen.quality;
            let toggled = if asked.is_some() { None } else { Some(has) };
            chips.push(OptionChip { option: ItemOption::Quality, label: "Quality".into(), number: Some(asked.unwrap_or(has)), on: asked.is_some(), click: Action::SetQuality(toggled) });
        }
        if let Plan::Gear { base, .. } | Plan::Exact { base, .. } = &self.plan {
            if let Some(base) = base {
                chips.push(OptionChip {
                    option: ItemOption::Base,
                    label: base.clone(),
                    number: None,
                    on: chosen.base,
                    click: Action::ToggleBase,
                });
            }
            let numbered = [
                (ItemOption::ItemLevel, "ilvl", self.item.item_level.map(|level| if is_cluster(&self.item) { cluster_level(level) } else { level }), chosen.item_level),
                (ItemOption::Quality, "Quality", self.item.quality, chosen.quality),
            ];
            for (option, label, has, asked) in numbered {
                let Some(has) = has else {
                    continue;
                };
                // Switched on, it asks for what the item has.
                let toggled = if asked.is_some() { None } else { Some(has) };
                let click = if option == ItemOption::ItemLevel { Action::SetItemLevel(toggled) } else { Action::SetQuality(toggled) };
                chips.push(OptionChip { option, label: label.into(), number: Some(asked.unwrap_or(has)), on: asked.is_some(), click });
            }
        }
        if matches!(self.plan, Plan::Gem { .. }) {
            let numbered = [
                (ItemOption::ItemLevel, "Level", gem_level(&self.item), chosen.item_level),
                (ItemOption::Quality, "Quality", self.item.quality.filter(|quality| *quality > 0), chosen.quality),
            ];
            for (option, label, has, asked) in numbered {
                let Some(has) = has else {
                    continue;
                };
                let toggled = if asked.is_some() { None } else { Some(has) };
                let click = if option == ItemOption::ItemLevel { Action::SetItemLevel(toggled) } else { Action::SetQuality(toggled) };
                chips.push(OptionChip { option, label: label.into(), number: Some(asked.unwrap_or(has)), on: asked.is_some(), click });
            }
        }
        if let Some(links) = self.links.filter(|links| *links >= 2) {
            let asked = chosen.links;
            chips.push(OptionChip {
                option: ItemOption::Links,
                label: format!("{} links", asked.unwrap_or(links)),
                number: None,
                on: asked.is_some(),
                click: Action::SetLinks(if asked.is_some() { None } else { Some(links) }),
            });
        }
        let exact = matches!(self.plan, Plan::Exact { .. });
        if !exact || self.item.corrupted || chosen.corrupted.is_some() {
            chips.push(OptionChip {
                option: ItemOption::Corrupted,
                label: corrupted_label(chosen.corrupted).into(),
                number: None,
                on: chosen.corrupted.is_some(),
                click: Action::CycleCorrupted,
            });
        }
        if self.item.rarity == Rarity::Magic && matches!(self.plan, Plan::Gear { .. }) {
            chips.push(OptionChip { option: ItemOption::Magic, label: "Magic only".into(), number: None, on: chosen.magic, click: Action::ToggleMagic });
        }
        if self.is_map() && self.bulk_allowed {
            chips.push(OptionChip { option: ItemOption::Bulk, label: "Bulk".into(), number: None, on: chosen.bulk, click: Action::ToggleBulk });
        }
        if is_foulborn(&self.item) && self.plan == Plan::Unique {
            chips.push(OptionChip { option: ItemOption::Foulborn, label: "Foulborn".into(), number: None, on: chosen.foulborn.is_some(), click: Action::ToggleFoulborn });
        }
        self.mark_chips(&mut chips);
        let on = self.stats_on();
        if on >= 2 {
            let label = self.any_count().map_or_else(|| "Any N".to_owned(), |n| format!("Any {n} of {on}"));
            chips.push(OptionChip { option: ItemOption::Any, label, number: None, on: self.any_count().is_some(), click: Action::ToggleAny });
        }
        // A rare with a free slot can be asked to have it still: off, as it asks for more than the item is.
        if matches!(&self.plan, Plan::Gear { category, .. } if category != MAP_CATEGORY)
            && let Some((prefixes, suffixes)) = open_affixes(&self.item, &parse_mods(&self.item))
        {
            if prefixes > 0 {
                chips.push(OptionChip { option: ItemOption::OpenPrefix, label: "Open prefix".into(), number: None, on: chosen.open_prefix, click: Action::ToggleOpenPrefix });
            }
            if suffixes > 0 {
                chips.push(OptionChip { option: ItemOption::OpenSuffix, label: "Open suffix".into(), number: None, on: chosen.open_suffix, click: Action::ToggleOpenSuffix });
            }
        }
        chips
    }

    /// The chips of the influences, the synthesis, and of what the item says of itself: mirrored,
    /// split, unidentified. Each only when the item has it; the influences only for gear.
    fn mark_chips(&self, chips: &mut Vec<OptionChip>) {
        let chosen = &self.options;
        let gear = matches!(self.plan, Plan::Gear { .. } | Plan::Exact { .. });
        let mut chip = |option, label: &str, on, click| chips.push(OptionChip { option, label: label.into(), number: None, on, click });
        if gear {
            for influence in &self.item.influences {
                let on = chosen.influences & influence.bit() != 0;
                chip(ItemOption::Influence, influence.label(), on, Action::ToggleInfluence(*influence));
            }
        }
        if self.item.mirrored {
            chip(ItemOption::Mirrored, "Mirrored", chosen.mirrored.is_some(), Action::ToggleMirrored);
        }
        if self.item.split {
            chip(ItemOption::Split, "Split", chosen.split.is_some(), Action::ToggleSplit);
        }
        if !self.candidates.is_empty() {
            let name = self.unique.as_deref().unwrap_or("Which unique?");
            chip(ItemOption::Unique, name, self.unique.is_some(), Action::PickUnique(None));
        }
        if !self.item.identified {
            chip(ItemOption::Identified, "Unidentified", chosen.identified.is_some(), Action::ToggleIdentified);
        }
    }

    /// What the chip of a setting says: its value now.
    pub fn setting_label(&self, setting: Setting) -> &'static str {
        match setting {
            Setting::Status => status_label(self.options.status),
            Setting::Currency => currency_label(self.options.currency),
            Setting::Listed => listed_label(self.options.listed),
        }
    }

    /// The page of the last search on the trade site, once the site has answered one.
    pub fn trade_url(&self) -> Option<String> {
        let league = self.result.league.as_deref()?;
        let query = self.result.query_id.as_deref()?;
        Some(format!("{}{}/{}", trade_page(), encode(league), encode(query)))
    }
}

/// The filters and the options a way of pricing starts from. The settings of every search (its
/// status, its currency, the age of the listings) are no part of a way: they stay as the check
/// has them.
///
/// As an item: the filters as the item was read, and no option but its corruption.
///
/// As a base: the base when the site knows it, the item level as far as `BASE_LEVEL`, and no
/// filter but those of the fractured mods, each asked from the low end of the range it was
/// rolled in when the item gives it and from its usual minimum otherwise. When the item is
/// fractured the listings must be too.
pub fn way_set(check: &Check, way: Way) -> (Vec<Filter>, Options) {
    let Options { status, currency, listed, .. } = check.options;
    let options = Options { status, currency, listed, ..Options::of(&check.item) };
    let options = Options { bulk: options.bulk && check.bulk_allowed, ..options };
    let mut filters = check.defaults.clone();
    if way == Way::Item {
        return (filters, options);
    }
    for filter in &mut filters {
        filter.on = false;
    }
    // A map keeps any boss of its family, and its influence.
    for filter in filters.iter_mut().filter(|filter| kept_on_base(&filter.id)) {
        filter.on = true;
    }
    for row in check.rows.iter().filter(|row| row.fractured) {
        if let Some(filter) = row.filter.and_then(|index| filters.get_mut(index)) {
            filter.on = true;
            if let Some((low, _)) = row.range {
                filter.min = Some(low);
            }
        }
    }
    let options = Options {
        base: matches!(&check.plan, Plan::Gear { base: Some(_), .. }),
        item_level: check.item.item_level.map(|level| level.min(BASE_LEVEL)),
        fractured: check.item.fractured.then_some(true),
        // A base is a base whatever its rarity.
        magic: false,
        ..options
    };
    (filters, options)
}

/// What a chip under the estimate asks of the item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemOption {
    Base,
    ItemLevel,
    Quality,
    Links,
    Corrupted,
    /// One influence or the synthesis.
    Influence,
    Mirrored,
    Split,
    Identified,
    /// Which unique an unidentified one is: a menu.
    Unique,
    /// A Foulborn unique asks for Foulborn copies only.
    Foulborn,
    /// A magic item asks for magic listings only.
    Magic,
    /// A map is priced on the bulk exchange.
    Bulk,
    /// A free prefix, a free suffix.
    OpenPrefix,
    OpenSuffix,
    /// "Any N" of the stats that are on.
    Any,
}

/// One chip of the item's options.
#[derive(Debug, Clone, PartialEq)]
pub struct OptionChip {
    pub option: ItemOption,
    /// The base's name, `ilvl`, `Quality`, `5 links`, `Not corrupted`.
    pub label: String,
    /// The number it holds, written after its label: the least the search asks for when the chip
    /// is on, what the item has otherwise.
    pub number: Option<u32>,
    /// It is part of the search.
    pub on: bool,
    /// What a click on it is.
    pub click: Action,
}

/// One of the three chips above the buttons: an option of every search, which can be pinned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// Instant buyout, or every listing.
    Status,
    Currency,
    /// How old a listing may be.
    Listed,
}

/// The settings in the order of their chips.
pub const SETTINGS: [Setting; 3] = [Setting::Status, Setting::Currency, Setting::Listed];

/// What has a menu of values: a chip, or the minimum field of a mod whose range is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chip {
    Corrupted,
    /// Which unique an unidentified one is.
    Unique,
    Setting(Setting),
    /// The minimum of this filter, when the range its mod was rolled in is known: see `rolls`.
    Roll(usize),
    /// The mod line at this place of a map: its tag's menu.
    MapMod(usize),
}

/// The lines of the menu of a roll, in the order of `rolls`.
pub const ROLLS: [&str; 3] = ["Min roll", "This item", "Max roll"];

/// The three values the minimum of a mod is set to in one gesture, for the filter at `index`:
/// the low end of the range the mod was rolled in, the value the item has, and the high end,
/// each as the stat counts it (for a mod of several numbers, their mean). `None` for a filter
/// no row has, and when the item does not give the range, as it never does for a property.
pub fn rolls(check: &Check, index: usize) -> Option<[f64; 3]> {
    let row = check.rows.iter().find(|row| row.filter == Some(index))?;
    let (low, high) = row.range?;
    Some([low, row.value?, high])
}

/// One line of a chip's menu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Choice<'a> {
    pub label: &'a str,
    /// What picking it is.
    pub action: Action,
    /// It is the value the chip has now.
    pub current: bool,
}

/// A yes-or-no option switched by a chip: on, it asks for `value`; off, it asks for nothing.
fn toggled(now: Option<bool>, value: bool) -> Option<bool> {
    if now.is_some() { None } else { Some(value) }
}

/// What the corrupted chip says. Off, it says what a click turns it to.
fn corrupted_label(corrupted: Option<bool>) -> &'static str {
    if corrupted == Some(true) { "Corrupted" } else { "Not corrupted" }
}

fn status_label(status: Status) -> &'static str {
    match status {
        Status::Securable => "Instant buyout",
        Status::Available => "All listings",
        // Not offered: only a setting written by hand has them.
        Status::Online => "Online sellers",
        Status::Any => "Any seller",
    }
}

fn currency_label(currency: Option<Currency>) -> &'static str {
    match currency {
        Some(Currency::ChaosDivine) => "Chaos and divine",
        Some(Currency::Chaos) => "Chaos only",
        Some(Currency::Divine) => "Divine only",
        None => "Any currency",
    }
}

fn listed_label(listed: Option<Listed>) -> &'static str {
    match listed {
        Some(Listed::Days3) => "Last 3 days",
        Some(Listed::Day1) => "Last day",
        Some(Listed::Week1) => "Last 7 days",
        None => "Any age",
    }
}

/// The values a click goes through, in order, for each chip that has several.
const CORRUPTED: [Option<bool>; 3] = [Some(false), Some(true), None];
pub const STATUSES: [Status; 2] = [Status::Securable, Status::Available];
pub const CURRENCIES: [Option<Currency>; 4] = [Some(Currency::ChaosDivine), Some(Currency::Chaos), Some(Currency::Divine), None];
pub const AGES: [Option<Listed>; 4] = [Some(Listed::Days3), Some(Listed::Day1), Some(Listed::Week1), None];

/// The value after `now` in `values`, going round; the first one for a value that is not there.
fn next<T: Copy + PartialEq>(values: &[T], now: T) -> T {
    let at = values.iter().position(|value| *value == now).map_or(0, |at| at + 1);
    values[at % values.len()]
}

/// The menu of a chip: every value it can take, in the order a click goes through them, the one
/// it has now marked. The menu of a roll: the values of `rolls`, the one the minimum is at
/// marked; it has no line when there is nothing to offer.
pub fn choices(check: &Check, chip: Chip) -> Vec<Choice<'_>> {
    let options = &check.options;
    match chip {
        Chip::Roll(index) => {
            let min = check.filters.get(index).and_then(|filter| filter.min);
            // An excluded stat has no minimum to set.
            let values = rolls(check, index).into_iter().flatten().take(if check.excluded(index) { 0 } else { 3 });
            let mut lines: Vec<Choice> =
                ROLLS.into_iter().zip(values).map(|(label, value)| Choice { label, action: Action::SetMin(index, Some(value)), current: min == Some(value) }).collect();
            if check.excluded(index) {
                lines.push(Choice { label: "Include", action: Action::IncludeFilter(index), current: false });
            } else if check.can_exclude(index) {
                if check.can_pure(index) {
                    lines.push(Choice { label: "Pure tier", action: Action::TogglePure(index), current: check.is_pure(index) });
                }
                lines.push(Choice { label: "Exclude", action: Action::ExcludeFilter(index), current: false });
            }
            lines
        }
        Chip::Unique => check
            .candidates
            .iter()
            .enumerate()
            .map(|(index, name)| Choice { label: name, action: Action::PickUnique(Some(index)), current: check.unique.as_deref() == Some(name.as_str()) })
            .collect(),
        Chip::MapMod(_) => Vec::new(),
        Chip::Corrupted => CORRUPTED
            .into_iter()
            .map(|value| Choice {
                label: if value.is_none() { "Any" } else { corrupted_label(value) },
                action: Action::SetCorrupted(value),
                current: options.corrupted == value,
            })
            .collect(),
        Chip::Setting(Setting::Status) => STATUSES
            .into_iter()
            .map(|value| Choice { label: status_label(value), action: Action::SetStatus(value), current: options.status == value })
            .collect(),
        Chip::Setting(Setting::Currency) => CURRENCIES
            .into_iter()
            .map(|value| Choice { label: currency_label(value), action: Action::SetCurrency(value), current: options.currency == value })
            .collect(),
        Chip::Setting(Setting::Listed) => AGES
            .into_iter()
            .map(|value| Choice { label: listed_label(value), action: Action::SetListed(value), current: options.listed == value })
            .collect(),
    }
}

/// What a click on the chip of a setting is: its next value.
pub fn cycle(setting: Setting) -> Action {
    match setting {
        Setting::Status => Action::CycleStatus,
        Setting::Currency => Action::CycleCurrency,
        Setting::Listed => Action::CycleListed,
    }
}

/// What the player did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// The mod line at this place of the map gets this tag, or none.
    MapTag(usize, Option<crate::maps::Tag>),
    /// Each with the index of the filter in `Check::filters`.
    ToggleFilter(usize),
    SetMin(usize, Option<f64>),
    SetMax(usize, Option<f64>),
    /// So many notches of the wheel: up is more.
    StepMin(usize, i32),
    StepMax(usize, i32),
    /// The mod must not be on the listings; and back to a normal row.
    ExcludeFilter(usize),
    IncludeFilter(usize),
    /// A pure tier of the stat on or off: no hybrid partner, no other tier.
    TogglePure(usize),
    /// "Any N" on or off, and so many notches of the wheel over it: up is more.
    ToggleAny,
    StepAny(i32),
    ToggleBase,
    /// Shows or hides again the rows a display rule hides.
    ToggleHidden,
    SetItemLevel(Option<u32>),
    SetQuality(Option<u32>),
    SetLinks(Option<u32>),
    ToggleInfluence(Influence),
    ToggleMirrored,
    ToggleSplit,
    ToggleIdentified,
    /// The unique an unidentified one is: the candidate at this index. `None`: nothing, the chip
    /// only opens its menu.
    PickUnique(Option<usize>),
    ToggleFoulborn,
    ToggleMagic,
    ToggleBulk,
    ToggleOpenPrefix,
    ToggleOpenSuffix,
    CycleCorrupted,
    SetCorrupted(Option<bool>),
    CycleStatus,
    SetStatus(Status),
    CycleCurrency,
    SetCurrency(Option<Currency>),
    CycleListed,
    SetListed(Option<Listed>),
    /// Price the item this way from now on: see `way_set`.
    SetWay(Way),
    SearchAgain,
    OpenTradeSite,
    Close,
    /// The quick menu's button at this place of `launcher::items`.
    Launch(usize),
    /// The saved search at this place of the list: sent to the game.
    RunSearch(usize),
    /// Opens the Settings window on the saved searches.
    EditSearches,
}

/// What is to be done outside the check once an action is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    None,
    /// Send `request` of the check: it is loading from now on.
    Search,
    /// Open this address of the trade site.
    Open(String),
    Close,
}

/// Applies what the player did. Anything that changes a filter or an option marks the outcome
/// as out of date; nothing here searches by itself. "Search again" is refused while a search is
/// running, for an item that is not searched, and for filters that ask for the impossible.
///
/// Another way of pricing replaces the filters and the options with the set of that way: what
/// was changed by hand is not kept. The way the check has already is no choice, and an item
/// that has no ways has none to choose.
pub fn apply(check: &mut Check, action: &Action) -> Effect {
    let before = (check.filters.clone(), check.options, check.not.clone(), check.pure.clone(), check.any_count());
    match *action {
        // The tags of a map are the map view's: a check has nothing to do for them.
        Action::MapTag(..) => {}
        Action::ExcludeFilter(index) => {
            if check.can_exclude(index) && !check.excluded(index) {
                check.not.push(index);
                check.pure.retain(|other| *other != index);
                check.filters[index].on = false;
            }
        }
        Action::TogglePure(index) => {
            if check.pure.contains(&index) {
                check.pure.retain(|other| *other != index);
            } else if check.can_pure(index) {
                check.pure.push(index);
                // A pure tier is a search of the stat: it switches it on.
                if !check.filters[index].on {
                    switch_on(check, index);
                }
            }
        }
        Action::IncludeFilter(index) => {
            if check.excluded(index) {
                check.not.retain(|other| *other != index);
                switch_on(check, index);
            }
        }
        Action::ToggleAny => {
            check.any = match check.any_count() {
                Some(_) => None,
                None => Some((check.stats_on() as u32).saturating_sub(1)).filter(|n| *n >= 1),
            };
        }
        Action::StepAny(notches) => {
            if let Some(now) = check.any_count() {
                let most = check.stats_on() as i64;
                check.any = Some((i64::from(now) + i64::from(notches)).clamp(1, most) as u32);
            }
        }
        Action::ToggleFilter(index) if check.excluded(index) => {
            check.not.retain(|other| *other != index);
            switch_on(check, index);
        }
        Action::ToggleFilter(index) => {
            let default = check.start_min(index);
            if let Some(filter) = check.filters.get_mut(index) {
                filter.on = !filter.on;
                if filter.on && filter.min.is_none() {
                    filter.min = default;
                }
            }
        }
        Action::SetMin(index, min) => edit(check, index, |filter| filter.min = min),
        Action::SetMax(index, max) => edit(check, index, |filter| filter.max = max),
        Action::StepMin(index, notches) => {
            let fine = check.fine(index);
            edit(check, index, |filter| {
            // A filter without a minimum starts from nothing: the wheel can only give it one.
            if filter.min.is_some() || notches > 0 {
                filter.min = Some(stepped_by(filter.min.unwrap_or(0.0), notches, fine));
            }
            })
        }
        Action::StepMax(index, notches) => {
            let fine = check.fine(index);
            edit(check, index, |filter| {
                filter.max = filter.max.map(|max| stepped_by(max, notches, fine));
            })
        }
        Action::ToggleHidden => check.reveal = !check.reveal,
        Action::ToggleBase => {
            // Only a base the site knows can be asked for.
            if matches!(&check.plan, Plan::Gear { base: Some(_), .. } | Plan::Exact { base: Some(_), .. }) {
                check.options.base = !check.options.base;
            }
        }
        Action::PickUnique(Some(index)) => {
            check.candidates_open = false;
            if let Some(name) = check.candidates.get(index).filter(|name| check.unique.as_ref() != Some(name)) {
                check.unique = Some(name.clone());
                check.stale = true;
            }
        }
        Action::PickUnique(None) => {}
        Action::SetItemLevel(level) => check.options.item_level = level,
        Action::SetQuality(quality) => check.options.quality = quality,
        Action::SetLinks(links) => check.options.links = links,
        Action::ToggleFoulborn => check.options.foulborn = if check.options.foulborn.is_some() { None } else { Some(true) },
        Action::ToggleMagic => check.options.magic = !check.options.magic,
        Action::ToggleBulk => check.options.bulk = !check.options.bulk,
        Action::ToggleOpenPrefix => check.options.open_prefix = !check.options.open_prefix,
        Action::ToggleOpenSuffix => check.options.open_suffix = !check.options.open_suffix,
        Action::ToggleInfluence(influence) => check.options.influences ^= influence.bit(),
        Action::ToggleMirrored => check.options.mirrored = toggled(check.options.mirrored, true),
        Action::ToggleSplit => check.options.split = toggled(check.options.split, true),
        Action::ToggleIdentified => check.options.identified = toggled(check.options.identified, false),
        Action::CycleCorrupted => check.options.corrupted = next(&CORRUPTED, check.options.corrupted),
        Action::SetCorrupted(corrupted) => check.options.corrupted = corrupted,
        Action::CycleStatus => check.options.status = next(&STATUSES, check.options.status),
        Action::SetStatus(status) => check.options.status = status,
        Action::CycleCurrency => check.options.currency = next(&CURRENCIES, check.options.currency),
        Action::SetCurrency(currency) => check.options.currency = currency,
        Action::CycleListed => check.options.listed = next(&AGES, check.options.listed),
        Action::SetListed(listed) => check.options.listed = listed,
        Action::SetWay(way) => {
            if check.has_ways() && check.way != way {
                (check.filters, check.options) = way_set(check, way);
                check.way = way;
                check.not.clear();
                check.pure.clear();
                check.any = None;
            }
        }
        Action::SearchAgain => {
            if !check.searchable() || check.result.loading || !check.valid() {
                return Effect::None;
            }
            // What the last search brought goes; its league is still the one searched.
            check.result = Outcome { league: check.result.league.take(), loading: true, ..Outcome::default() };
            check.stale = false;
            return Effect::Search;
        }
        Action::OpenTradeSite => return check.trade_url().map_or(Effect::None, Effect::Open),
        Action::Close => return Effect::Close,
        Action::Launch(_) | Action::RunSearch(_) | Action::EditSearches => return Effect::None,
    }
    if before != (check.filters.clone(), check.options, check.not.clone(), check.pure.clone(), check.any_count()) {
        check.stale = true;
    }
    Effect::None
}

/// Turns the filter at `index` on, from the minimum it started with when it has none.
fn switch_on(check: &mut Check, index: usize) {
    let default = check.start_min(index);
    if let Some(filter) = check.filters.get_mut(index) {
        filter.on = true;
        if filter.min.is_none() {
            filter.min = default;
        }
    }
}

fn edit(check: &mut Check, index: usize, change: impl FnOnce(&mut Filter)) {
    if let Some(filter) = check.filters.get_mut(index) {
        change(filter);
    }
}

/// The default minimum the player chose in the settings, as `Slack::ALL` indexes it.
static SLACK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Sets how far under the item's value a new check starts its minimums.
pub fn set_slack(slack: zenith_core::matching::Slack) {
    let at = zenith_core::matching::Slack::ALL.iter().position(|s| *s == slack).unwrap_or(0);
    SLACK.store(at, std::sync::atomic::Ordering::Relaxed);
}

fn slack() -> zenith_core::matching::Slack {
    let at = SLACK.load(std::sync::atomic::Ordering::Relaxed);
    zenith_core::matching::Slack::ALL.get(at).copied().unwrap_or_default()
}

/// Whether a range or a value has a fractional part.
pub fn has_decimals(range: Option<(f64, f64)>, value: Option<f64>) -> bool {
    let frac = |n: f64| n.is_finite() && (n - n.round()).abs() > 1e-9;
    range.is_some_and(|(a, b)| frac(a) || frac(b)) || value.is_some_and(frac)
}

/// The step of the wheel going away from zero from a value of this size, and going toward zero.
fn step_of(size: f64, away: bool) -> f64 {
    let under = |limit: f64| if away { size < limit } else { size <= limit };
    if under(100.0) {
        1.0
    } else if under(500.0) {
        5.0
    } else {
        10.0
    }
}

/// `value` moved by so many notches of the wheel, each to the next multiple of its step: 1 under
/// 100, 5 under 500, 10 from there. A value that is not negative never becomes negative.
pub fn stepped(value: f64, notches: i32) -> f64 {
    stepped_by(value, notches, false)
}

/// `stepped`, in tenths under 10 for a value that has decimals (`fine`).
pub fn stepped_by(value: f64, notches: i32, fine: bool) -> f64 {
    let up = notches > 0;
    let mut moved = value;
    for _ in 0..notches.unsigned_abs() {
        let size = moved.abs();
        if fine && (if up == (moved >= 0.0) { size < 10.0 } else { size <= 10.0 }) {
            // Counted in tenths, so that 1.5 is 15 and not 15.000000000000002.
            let tenths = moved * 10.0;
            let near = tenths.round();
            let tenths = if (tenths - near).abs() < 1e-6 { near } else { tenths };
            moved = (if up { tenths.floor() + 1.0 } else { tenths.ceil() - 1.0 }) / 10.0;
            continue;
        }
        let step = step_of(moved.abs(), up == (moved >= 0.0));
        let multiple = moved / step;
        moved = if up { (multiple.floor() + 1.0) * step } else { (multiple.ceil() - 1.0) * step };
    }
    if value >= 0.0 {
        moved = moved.max(0.0);
    }
    // Not "-0".
    moved + 0.0
}

/// What the "Search again" button does right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Again {
    /// It sends a search.
    Ready,
    /// The request budget is spent: nothing is sent for so many seconds.
    Wait(u32),
    /// Nothing: a search is running, the item is not searched, or a filter asks for the
    /// impossible.
    Off,
}

pub fn again(check: &Check, budget: Budget) -> Again {
    if !check.searchable() || check.result.loading || !check.valid() {
        Again::Off
    } else if budget.left == 0 {
        Again::Wait(budget.wait_s.max(1))
    } else {
        Again::Ready
    }
}

/// `apply`, with the request budget in the way of "Search again": at zero, nothing is sent.
///
/// Another way of pricing is a choice of what to search, so it searches at once, like "Search
/// again", when that would send a search right now. Otherwise (a search is running, or the
/// budget is spent) the check is out of date like after any other change.
pub fn act(check: &mut Check, action: &Action, budget: Budget) -> Effect {
    match *action {
        Action::SearchAgain if again(check, budget) != Again::Ready => Effect::None,
        // A unique picked for an unidentified one is searched at once.
        Action::PickUnique(Some(index)) => {
            let picked = check.candidates.get(index).is_some_and(|name| check.unique.as_ref() != Some(name));
            apply(check, action);
            if picked && again(check, budget) == Again::Ready { apply(check, &Action::SearchAgain) } else { Effect::None }
        }
        Action::SetWay(way) => {
            let chosen = check.has_ways() && check.way != way;
            apply(check, action);
            if chosen && again(check, budget) == Again::Ready { apply(check, &Action::SearchAgain) } else { Effect::None }
        }
        _ => apply(check, action),
    }
}

/// What "Search again" sends: for a unique its name, for other gear its category, the filters
/// that are on, each where the site expects it, and the options. `None` for an item that is not
/// searched. Gear is asked for as anything but a unique; an item searched by its very base has
/// no rarity, and none is asked for.
///
/// A defence is asked for in whole numbers. The defences and the physical damage are asked for as
/// they are at 20% quality, which is what the site compares and what the filters hold. For a unique the request holds its name, the mods
/// that are on, its linked sockets when they are asked for, and the options of the listing (its status, currency, age
/// and corruption): the base, the item level, the quality and the fracture say nothing of a
/// search by name.
pub fn request(check: &Check) -> Option<serde_json::Value> {
    let chosen = &check.options;
    let options = SearchOptions {
        status: chosen.status,
        currency: chosen.currency,
        listed: chosen.listed,
        corrupted: chosen.corrupted,
        influences: chosen.influences,
        mirrored: chosen.mirrored,
        split: chosen.split,
        identified: chosen.identified,
        mutated: chosen.foulborn,
        vestigial: check.item.vestigial.then_some(true),
    };
    // The unique chosen for an unidentified one: the search of that unique, still unidentified.
    if let (Some(name), Plan::Gear { .. }) = (&check.unique, &check.plan) {
        return Some(unique_request(search_name(name), &check.item.base, chosen.links, &[], &options));
    }
    match &check.plan {
        Plan::Nothing | Plan::NeedsData | Plan::Stack => None,
        Plan::Gem { name, disc } => Some(gem_request(name, disc.as_deref(), chosen.item_level, chosen.quality, &options)),
        Plan::Unique => {
            let name = check.item.name.as_deref()?;
            let stats: Vec<_> = check.filters.iter().filter(|filter| filter.on && !is_property(&filter.id)).map(|filter| (filter.id.clone(), filter.min, filter.max)).collect();
            let mut request = unique_request(search_name(name), &check.item.base, chosen.links, &stats, &options);
            let mut weapon = GearSearch::default();
            let whole = |bound: Option<f64>| bound.map(|bound| bound.max(0.0) as u32);
            for filter in check.filters.iter().filter(|filter| filter.on) {
                match filter.id.as_str() {
                    PHYSICAL_DPS => (weapon.pdps_min, weapon.pdps_max) = (filter.min, filter.max),
                    ELEMENTAL_DPS => (weapon.edps_min, weapon.edps_max) = (filter.min, filter.max),
                    TOTAL_DPS => (weapon.dps_min, weapon.dps_max) = (filter.min, filter.max),
                    CRITICAL_CHANCE => (weapon.crit_min, weapon.crit_max) = (filter.min, filter.max),
                    ATTACKS_PER_SECOND => (weapon.aps_min, weapon.aps_max) = (filter.min, filter.max),
                    BASE_PERCENTILE => weapon.base_percentile_min = whole(filter.min),
                    _ => {}
                }
            }
            add_weapon_filters(&mut request, &weapon);
            add_armour_filters(&mut request, &weapon);
            zenith_trade::add_quality(&mut request, chosen.quality);
            if check.item.class == "Maps" {
                as_unique_map(&mut request, search_name(name), &check.item.base);
            }
            Some(grouped(check, request))
        }
        Plan::Gear { category, .. } if category == MAP_CATEGORY && chosen.bulk => Some(serde_json::json!({ BULK: crate::quote::name_of(&check.item) })),
        Plan::Gear { category, base } | Plan::Exact { category, base } => {
            let whole = |bound: Option<f64>| bound.map(|bound| bound.max(0.0) as u32);
            let mut stats = Vec::new();
            let mut search = GearSearch {
                category: Some(category),
                base: base.as_deref().filter(|_| chosen.base),
                rarity_magic: matches!(check.plan, Plan::Gear { .. }) && chosen.magic,
                rarity_nonunique: matches!(check.plan, Plan::Gear { .. }) && check.item.rarity != Rarity::Unique,
                rarity_unique: matches!(check.plan, Plan::Gear { .. }) && check.item.rarity == Rarity::Unique,
                item_level_min: chosen.item_level,
                quality_min: chosen.quality,
                quality_max: zenith_trade::jewellery_quality_max(category, chosen.quality),
                links_min: chosen.links,
                fractured: chosen.fractured,
                // A map is searched at its tier, whatever else is asked.
                map_tier: if category == MAP_CATEGORY { parse_properties(&check.item).map_tier } else { None },
                map_reward: if category == MAP_CATEGORY { map_reward(&check.item) } else { None },
                map_blight: if category == MAP_CATEGORY { blight_of(&check.item.base).map(|(disc, _)| disc) } else { None },
                ..GearSearch::default()
            };
            for filter in check.filters.iter().filter(|filter| filter.on) {
                match filter.id.as_str() {
                    ARMOUR => (search.armour_min, search.armour_max) = (whole(filter.min), whole(filter.max)),
                    EVASION => (search.evasion_min, search.evasion_max) = (whole(filter.min), whole(filter.max)),
                    ENERGY_SHIELD => {
                        (search.energy_shield_min, search.energy_shield_max) = (whole(filter.min), whole(filter.max));
                    }
                    BLOCK => (search.block_min, search.block_max) = (whole(filter.min), whole(filter.max)),
                    BASE_PERCENTILE => search.base_percentile_min = whole(filter.min),
                    MAP_BONUS => search.map_bonus_min = whole(filter.min),
                    MAP_QUANTITY => search.map_quantity_min = whole(filter.min),
                    MAP_RARITY => search.map_rarity_min = whole(filter.min),
                    MAP_PACK_SIZE => search.map_pack_size_min = whole(filter.min),
                    PHYSICAL_DPS => (search.pdps_min, search.pdps_max) = (filter.min, filter.max),
                    ELEMENTAL_DPS => (search.edps_min, search.edps_max) = (filter.min, filter.max),
                    TOTAL_DPS => (search.dps_min, search.dps_max) = (filter.min, filter.max),
                    CRITICAL_CHANCE => (search.crit_min, search.crit_max) = (filter.min, filter.max),
                    ATTACKS_PER_SECOND => (search.aps_min, search.aps_max) = (filter.min, filter.max),
                    id if map_pseudo(id).is_some() => {
                        // The number of affixes is asked exactly.
                        let max = if id == MAP_MODIFIERS { filter.min } else { filter.max };
                        stats.push((map_pseudo(id).unwrap_or_default().to_owned(), filter.min, max));
                    }
                    id if !is_property(id) => stats.push((filter.id.clone(), filter.min, filter.max)),
                    _ => {}
                }
            }
            for (asked, id) in [(chosen.open_prefix, OPEN_PREFIX), (chosen.open_suffix, OPEN_SUFFIX)] {
                if asked {
                    stats.push((id.to_owned(), Some(1.0), None));
                }
            }
            search.stats = &stats;
            Some(grouped(check, gear_request(&search, &options)))
        }
    }
}

/// When the first search of a check found no listing, the "Any N" value to search again with: one
/// short of the stats that are on. Only for two stats or more, with "Any N" not on already, and
/// while a search is left in the budget.
pub fn broaden(check: &Check, found: u32, left: u32) -> Option<u32> {
    let on = check.stats_on() as u32;
    (found == 0 && left > 0 && on >= 2 && check.any_count().is_none()).then(|| on - 1)
}

/// The request of `check` with "Any N" on at `n`.
pub fn relaxed_request(check: &Check, n: u32) -> Option<serde_json::Value> {
    request(&Check { any: Some(n), ..check.clone() })
}

/// A request with the excluded stats in a `not` group and, when "Any N" is on, the stats that are
/// on in a `count` group.
fn grouped(check: &Check, mut request: serde_json::Value) -> serde_json::Value {
    let mut not: Vec<String> = check.not.iter().filter_map(|index| check.filters.get(*index)).map(|filter| filter.id.clone()).collect();
    // A pure tier has none of the hybrid partners of its stat, unless the search asks for that stat.
    for index in check.pure.iter().filter(|index| check.is_pure(**index)) {
        for partner in zenith_core::matching::hybrid_partners(&check.filters[*index].id) {
            let asked = check.filters.iter().any(|filter| filter.on && filter.id == *partner);
            if !asked && !not.iter().any(|id| id == partner) {
                not.push((*partner).to_owned());
            }
        }
    }
    // A Valdo map is worth more when a death does not send its players to the Void.
    if check.is_map() && map_reward(&check.item).is_some() && !check.item.sections.iter().flatten().any(|line| line.contains("sent to the Void")) {
        not.push(VOID_DEATH.to_owned());
    }
    zenith_trade::regroup_stats(&mut request, &not, check.any_count(), &[OPEN_PREFIX, OPEN_SUFFIX]);
    any_boss_group(&mut request);
    request
}

/// The site has no "any" option for a stat of options: any boss of a family is asked as a count
/// group, one of its four bosses.
fn any_boss_group(request: &mut serde_json::Value) {
    let Some(groups) = request.pointer_mut("/query/stats").and_then(serde_json::Value::as_array_mut) else { return };
    let mut asked = Vec::new();
    for group in groups.iter_mut() {
        if let Some(filters) = group.get_mut("filters").and_then(serde_json::Value::as_array_mut) {
            filters.retain(|filter| match filter.get("id").and_then(serde_json::Value::as_str) {
                // The boss itself has its option; any boss has none.
                Some(id) if is_any_boss(id) && filter.pointer("/value/option").is_none() => {
                    asked.push(id.to_owned());
                    false
                }
                _ => true,
            });
        }
    }
    for id in asked {
        let bosses: Vec<_> = (1..=4).map(|option| serde_json::json!({ "id": id, "value": { "option": option }, "disabled": false })).collect();
        groups.push(serde_json::json!({ "type": "count", "value": { "min": 1 }, "filters": bosses }));
    }
}

/// "Players who Die in area are sent to the Void", which a Valdo map can have.
const VOID_DEATH: &str = "explicit.stat_1095765106";

/// The trade category of maps.
pub const MAP_CATEGORY: &str = "map";

/// The key of the request of a check priced on the bulk exchange: `{"bulk": "Nightmare Map"}`,
/// the name the exchange is asked for. No search request has it.
const BULK: &str = "bulk";

/// A mod of a unique that has no range while the item gives the ranges of its other mods: its
/// numbers are the unique's own, not a roll.
pub fn fixed_unique_mod(check: &Check, row: &Row) -> bool {
    check.plan == Plan::Unique && is_unique_kind(row) && row.range.is_none() && check.rows.iter().any(|row| row.range.is_some())
}

/// A mod the unique itself has (`Unique`, `Foulborn Unique`), not a corruption, an enchant or a
/// craft, whose number matters even without a range.
pub fn is_unique_kind(row: &Row) -> bool {
    !row.corruption && row.kind.as_deref().is_some_and(|kind| kind.ends_with("Unique"))
}

/// The bosses a map can hold, by family: the trade stat of the family, whose option is the boss,
/// and what the panel calls any of them.
pub const MAP_BOSSES: [(&str, &str); 2] = [("implicit.stat_2563183002", "Any conqueror"), ("implicit.stat_3624393862", "Any Elder guardian")];

/// The influence of a map of the Shaper's or the Elder's guardians: kept on the base view, as
/// the boss family is.
const MAP_INFLUENCE: &str = "implicit.stat_1792283443|";

/// Whether a stat asks for any boss of a family.
pub fn is_any_boss(id: &str) -> bool {
    MAP_BOSSES.iter().any(|(family, _)| *family == id)
}

/// What the base view of a map keeps on: any boss of its family, and its influence.
fn kept_on_base(id: &str) -> bool {
    is_any_boss(id) || id.starts_with(MAP_INFLUENCE)
}

/// The name a request asks the bulk exchange for, when it is a bulk one.
pub fn bulk_name(request: &serde_json::Value) -> Option<&str> {
    request.get(BULK)?.as_str()
}

/// A map that is priced on the bulk exchange from the start, as the players trade it: all but an
/// identified rare one that is not blighted, whose numbers are worth searching.
fn starts_in_bulk(item: &Item) -> bool {
    let blighted = item.base.starts_with("Blighted ") || item.base.starts_with("Blight-ravaged ");
    item.class == "Maps" && item.rarity != Rarity::Unique && (item.rarity != Rarity::Rare || !item.identified || blighted)
}

/// The pseudo stats of the trade site for the free slots of a rare.
const OPEN_PREFIX: &str = "pseudo.pseudo_number_of_empty_prefix_mods";
const OPEN_SUFFIX: &str = "pseudo.pseudo_number_of_empty_suffix_mods";

/// A stat id always has a dot; the ids of the property filters never do.
pub fn is_property(id: &str) -> bool {
    !id.contains('.')
}

/// Whether the item is a Foulborn unique.
fn is_foulborn(item: &Item) -> bool {
    item.rarity == Rarity::Unique && item.name.as_deref().is_some_and(|name| name.starts_with("Foulborn "))
}

/// The name to search a unique by.
///
/// A unique changed by the Foulborn mechanic is copied as `Foulborn <name>`; the trade site
/// knows it under the plain name (checked: the plain name with the `mutated` filter finds
/// Foulborn copies), so the Foulborn copies are told apart by that filter, not by the name.
/// A unique map as the site lists it: its name and the type "Map", both told apart as maps; a
/// Valdo map has a type of its own.
fn as_unique_map(request: &mut serde_json::Value, name: &str, base: &str) {
    let Some(query) = request.get_mut("query").and_then(serde_json::Value::as_object_mut) else { return };
    if base.ends_with("Valdo Map") {
        query.insert("type".into(), serde_json::json!("Valdo Map"));
    } else {
        query.insert("name".into(), serde_json::json!({ "option": name, "discriminator": "map" }));
        query.insert("type".into(), serde_json::json!({ "option": "Map", "discriminator": "map" }));
    }
}

/// A blighted map's discriminator on the site and its area: `Blight-ravaged Arcade Map` is
/// `("uberblighted", "Arcade")`.
pub fn blight_of(base: &str) -> Option<(&'static str, &str)> {
    let (disc, rest) = match (base.strip_prefix("Blighted "), base.strip_prefix("Blight-ravaged ")) {
        (Some(rest), _) => ("blighted", rest),
        (_, Some(rest)) => ("uberblighted", rest),
        _ => return None,
    };
    Some((disc, rest.strip_suffix(" Map")?))
}

/// A cluster jewel that is not a unique: it is searched by its size and its passives.
fn is_cluster(item: &Item) -> bool {
    item.rarity != Rarity::Unique && item.base.ends_with("Cluster Jewel")
}

/// The item level a cluster jewel is asked from: the step it is at, as the notables it can have
/// change at 1, 50, 68, 75 and 84.
fn cluster_level(level: u32) -> u32 {
    [84, 75, 68, 50, 1].into_iter().find(|step| level >= *step).unwrap_or(1)
}

/// The unique a Valdo map gives once completed: its line `Reward: Foil <unique>`.
fn map_reward(item: &Item) -> Option<&str> {
    item.sections.iter().flatten().find_map(|line| line.strip_prefix("Reward: Foil ")).map(str::trim)
}

pub fn search_name(name: &str) -> &str {
    name.strip_prefix("Foulborn ").unwrap_or(name)
}

/// Where the page of a search is: the league and the id of the search follow.
fn trade_page() -> String {
    format!("{BASE_URL}/trade/search/")
}

/// Whether `url` is the page of a search on the trade site and nothing else: the only thing the
/// application ever hands to the system to open.
pub fn is_trade_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix(trade_page().as_str()) else {
        return false;
    };
    let plain = |part: &str| {
        !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'%'))
    };
    matches!(rest.split_once('/'), Some((league, query)) if plain(league) && plain(query))
}

/// Percent-encodes everything but unreserved characters, for one segment of an address.
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
