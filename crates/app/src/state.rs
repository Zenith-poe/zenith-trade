//! What the panel shows, and the pure decisions around it: the first view of a copied text, the
//! wording of each problem, the league to search in, and how prices and times are written.

use zenith_core::matching::{BASE_PERCENTILE, at_trade_quality, base_percentile, imbued_filter};
use zenith_core::{
    Estimate, Filter, Item, Local, Mod, ModKind, Properties, Rarity, Stats, default_filters, estimate, is_pseudo, match_mod, parse_item,
    parse_mods, parse_properties, pseudo_totals, unique_filters,
};
use zenith_data::{Changes, Pack, changes};
use zenith_trade::{Listing, TradeError};

use crate::check::{Action, ItemOption, OptionChip, Way, apply, is_property};
use zenith_prices::PriceList;

use crate::config::Config;
use crate::quote::{self, Quote};
pub use crate::check::{Check, Mark, Outcome, Plan, Row};

#[cfg(feature = "dev")]
#[path = "dev/samples.rs"]
mod samples;
#[cfg(feature = "dev")]
pub use samples::*;

pub const NOT_AN_ITEM: &str = "Not an item";
pub const UNIDENTIFIED: &str = "This unique is unidentified: it cannot be priced by name yet";
pub const NOT_PRICED: &str = "This kind of item is not priced yet";
pub const NO_LISTINGS: &str = "No listings";
/// Shown under the mods of a piece of gear when there is no data pack and none could be built.
pub const NO_GAME_DATA: &str = "Game data is missing: run the data update";
/// Shown while the data pack is being built, before the first search of a piece of gear.
pub const LOADING_DATA: &str = "Loading game data";
pub const BROWSER_CHECK: &str =
    "The trade site asks for a browser check. Open it once in your browser, then try again.";
pub const NO_LEAGUE_LIST: &str = "Could not load the league list";

/// Everything the panel draws.
#[derive(Debug, Clone, PartialEq)]
#[expect(clippy::large_enum_variant, reason = "a handful of views exist at a time, and most are items")]
pub enum View {
    /// One sentence and nothing else.
    Message(String),
    /// An item and where its check stands.
    Item(Check),
    /// A map and the player's tags on its mods.
    Maps(crate::maps::MapView),
    /// The quick menu: a row of tools.
    Launcher,
    /// The saved searches, by title, one button each.
    Searches(Vec<String>),
}

/// What a check reports as it goes.
#[derive(Debug, Clone, PartialEq)]
#[expect(clippy::large_enum_variant, reason = "one at a time, on its way to the view")]
pub enum Step {
    /// The copied text, read: the view to show from now on.
    Read(View),
    /// Where the search stands.
    Found(Outcome),
    /// The search found nothing and was sent again with "Any N" on at this value.
    Relaxed(u32),
    /// The live bulk offers of a stackable.
    Bulk(Vec<zenith_trade::Bulk>),
}

impl View {
    pub fn check(&self) -> Option<&Check> {
        match self {
            View::Message(_) | View::Maps(_) | View::Launcher | View::Searches(_) => None,
            View::Item(check) => Some(check),
        }
    }

    #[cfg_attr(not(feature = "dev"), allow(dead_code))] // only the console report of `--price` uses it
    pub fn problem(&self) -> Option<&str> {
        match self {
            View::Message(text) => Some(text),
            View::Item(check) => check.result.problem.as_deref(),
            View::Maps(_) | View::Launcher | View::Searches(_) => None,
        }
    }

    /// More is on its way: the panel has bars to pulse.
    pub fn loading(&self) -> bool {
        self.check().is_some_and(|check| check.result.loading)
    }

    /// Takes a step of the check this view shows. An item read again the same way keeps what the
    /// player changed since it was first shown; read differently, it is replaced whole.
    pub fn take(&mut self, step: Step) {
        match step {
            Step::Found(outcome) => {
                if let View::Item(check) = self {
                    check.result = outcome;
                }
            }
            Step::Bulk(bulk) => {
                if let View::Item(check) = self {
                    check.result.bulk = bulk;
                }
            }
            Step::Relaxed(n) => {
                if let View::Item(check) = self {
                    check.any = Some(n);
                }
            }
            Step::Read(View::Item(read)) => match self {
                View::Item(shown) if shown.same_reading(&read) => {
                    shown.result = read.result;
                    // The pictures of the picker come after the first reading.
                    if read.candidate_icons.iter().any(Option::is_some) {
                        shown.candidate_icons = read.candidate_icons;
                    }
                }
                _ => *self = View::Item(read),
            },
            Step::Read(view) => *self = view,
        }
    }
}

/// Whether the cached price of a unique stands in for the listings: the search failed (a network
/// error, a refusal, a fetch that gave nothing) and brought no estimate, which is not the same as
/// a search that found no listing.
pub fn cached_stands(outcome: &Outcome) -> bool {
    outcome.estimate.is_none() && outcome.listings.is_empty() && !outcome.loading && outcome.problem.as_deref().is_some_and(|problem| problem != NO_LISTINGS)
}

/// Reads a copied text with what the data pack knows, when there is one. The outcome's `loading`
/// is set exactly when something is to follow: see the check's `plan`.
///
/// An identified unique is searched by its name; with a pack, each of its mods that has a stat
/// is a filter too, off to start with. A normal, magic or rare item whose class has a
/// trade category is searched on that category and on its filters, as the item it is or, when it
/// is fractured, as a base first: see `Check::new`. An item that has no rarity and whose class
/// has a trade category (a viscera) is searched on that category and on its very base, with
/// every mod that has a stat asked from the value the item has: what tells two of them apart is
/// their rolls. Nothing else is priced.
///
/// The settings of the search are the built-in ones: see `read_pinned`.
#[cfg_attr(not(feature = "dev"), allow(dead_code))] // only the console report of `--price` uses it
pub fn read(text: &str, pack: Option<&Pack>) -> View {
    read_pinned(text, pack, &Config::default())
}

/// `read`, the check starting from the settings the player pinned: each pinned value in place of
/// the built-in one.
pub fn read_pinned(text: &str, pack: Option<&Pack>, pins: &Config) -> View {
    read_with(text, pack, pins, true)
}

/// `read_pinned`; `cover` is whether the local mods that a total row covers are hidden. The made-up
/// views of the demo keep them in view, and have no base percentile row.
fn read_with(text: &str, pack: Option<&Pack>, pins: &Config, cover: bool) -> View {
    let Ok(item) = parse_item(text) else {
        return View::Message(NOT_AN_ITEM.into());
    };
    let unique = item.rarity == Rarity::Unique;
    let categorised = zenith_data::category(&item.class).is_some();
    // No rarity at all: its one name is its base, and there is no rarity to ask for.
    let exact = item.rarity == Rarity::None && categorised;
    // An unidentified unique has no name line: it is searched by its base, as a unique.
    let named_unique = unique && item.name.is_some() && item.identified;
    let gear = (matches!(item.rarity, Rarity::Normal | Rarity::Magic | Rarity::Rare) && categorised) || exact || (unique && !named_unique && categorised);
    // A magic or rare map is searched on the site; a plain one is priced from the list, by its base.
    // A magic or rare map is searched on the site, by its properties or on the bulk exchange.
    let map = item.class == "Maps" && matches!(item.rarity, Rarity::Magic | Rarity::Rare);
    let mods = if unique || gear || map { parse_mods(&item) } else { Vec::new() };
    // The implicits of a map share one header, each line its own stat: one filter per line.
    let mods = if map { one_line_implicits(mods) } else { mods };
    let local = local_of(&item.class);
    let stats = pack.map(|pack| &pack.stats);
    let props = parse_properties(&item);

    let mut filters = Vec::new();
    let (plan, problem) = if named_unique {
        // Its mods can be asked for too, once the data pack says which stats they are.
        if local == Local::Weapon {
            filters = unique_weapon_filters(&mods, &props);
        }
        if let Some(stats) = stats {
            filters.extend(unique_filters(&mods, local, stats));
        }
        add_base_percentile(&mut filters, &mods, &props, &item.base);
        (Plan::Unique, None)
    } else if item.rarity == Rarity::Gem && item.imbued && stats.is_none() {
        // The support built into an imbued gem is known from the game data only: wait for it,
        // or the first search would ask for any gem of the name.
        (Plan::NeedsData, None)
    } else if item.rarity == Rarity::Gem {
        // By its name; a transfigured gem is its base gem and a discriminator.
        let name = item.name.as_deref().unwrap_or(&item.base);
        let found = pack.and_then(|pack| pack.gems.get(name));
        let plan = match found {
            Some((base, disc)) => Plan::Gem { name: base.clone(), disc: Some(disc.clone()) },
            None => Plan::Gem { name: name.to_owned(), disc: None },
        };
        // An imbued gem asks for the support built into it.
        if let Some(filter) = stats.and_then(|stats| imbued_filter(&item, stats)) {
            filters.push(filter);
        }
        (plan, None)
    } else if map {
        match pack {
            None => (Plan::NeedsData, None),
            Some(pack) => {
                // What the map adds, on, and its implicits (a conqueror's citadel, a guardian, the
                // Elder, the Originator's memories), on as well; its mods are there to switch on.
                filters = default_filters(&mods, &props, Local::None, &pack.stats);
                for filter in filters.iter_mut().filter(|filter| !is_property(&filter.id)) {
                    filter.on = filter.id.starts_with("implicit.");
                }
                // A blighted map is searched as its area, which an older pack does not know.
                match crate::check::blight_of(&item.base) {
                    Some((_, area)) => match pack.blight_areas.get(area) {
                        Some(id) => (Plan::Gear { category: crate::check::MAP_CATEGORY.to_owned(), base: Some(id.clone()) }, None),
                        None => (Plan::NeedsData, None),
                    },
                    None => (Plan::Gear { category: crate::check::MAP_CATEGORY.to_owned(), base: Some(item.base.clone()) }, None),
                }
            }
        }
    } else if props.stored_experience.is_some() && item.rarity == Rarity::Currency {
        // A Facetor's Lens is searched by its base and the experience it holds, not priced from the lists.
        match pack {
            None => (Plan::NeedsData, None),
            Some(pack) => {
                filters = default_filters(&mods, &props, Local::None, &pack.stats);
                (Plan::Exact { category: String::new(), base: Some(item.base.clone()) }, None)
            }
        }
    } else if !gear {
        (Plan::Nothing, Some(if unique { UNIDENTIFIED } else { NOT_PRICED }))
    } else {
        match pack {
            None => (Plan::NeedsData, None),
            // The pack keeps the categories the site offers.
            Some(pack) => match pack.categories.get(&item.class) {
                Some(category) => {
                    filters = default_filters(&mods, &props, local, &pack.stats);
                    if cover {
                        add_base_percentile(&mut filters, &mods, &props, &item.base);
                    }
                    let (category, base) = (category.clone(), base_of(&item, pack));
                    (if exact { Plan::Exact { category, base } } else { Plan::Gear { category, base } }, None)
                }
                None => (Plan::Nothing, Some(NOT_PRICED)),
            },
        }
    };
    let mut rows = rows_of(&mods, local, stats, &filters, &props);
    // The built-in support of an imbued gem: a row of its own, as a mod.
    if item.imbued {
        for (index, filter) in filters.iter().enumerate().filter(|(_, filter)| filter.id.starts_with("imbued.")) {
            rows.push(Row {
                kind: Some("Imbued".into()),
                tier: None,
                roll: None,
                lines: vec![filter.label.clone()],
                figure: None,
                q20: false,
                id: Some(filter.id.clone()),
                filter: Some(index),
                unmatched: false,
                fractured: false,
                crafted: false,
                foulborn: false,
                origin: None,
                corruption: false,
                hidden: false,
                implicit: false,
                variant: false,
                value: None,
                range: None,
            });
        }
    }
    shorten_cluster(&mut rows);
    // A map is priced by what it adds and its implicits: its other mods wait behind the Hidden
    // switch.
    if map {
        for row in rows.iter_mut().filter(|row| row.kind.is_some() && !row.implicit) {
            row.hidden = true;
        }
        any_boss(&mut rows, &mut filters);
    }
    if exact {
        exact_filters(&mut filters, &rows);
    }
    if unique {
        hide_fixed(&mut rows, &mut filters, &mods);
        // A mod that did not roll is asked for as it is, without a number.
        if rows.iter().any(|row| row.range.is_some()) {
            for row in rows.iter().filter(|row| crate::check::is_unique_kind(row) && row.range.is_none()) {
                if let Some(filter) = row.filter.and_then(|index| filters.get_mut(index)) {
                    (filter.min, filter.max) = (None, None);
                }
            }
        }
        if let Some(name) = item.name.as_deref().filter(|_| named_unique) {
            mark_variants(&mut rows, &mut filters, &mods, &zenith_data::unique_variants(name));
        }
    }
    if cover && matches!(plan, Plan::Gear { .. }) && !unique {
        hide_covered(&mut rows, &mut filters, &mods, local);
    }
    let mut check = Check::new(item, rows, filters, plan);
    // A map goes to the bulk exchange only when the settings allow it.
    check.bulk_allowed = pins.map_bulk;
    check.options.bulk &= pins.map_bulk;
    // An unidentified unique: the uniques of its base are there to choose from.
    if let Some(pack) = pack.filter(|_| unique && !named_unique) {
        let names = pack.uniques_of(&check.item.base);
        check.offer_uniques(names);
    }
    pins.apply(&mut check.options);
    check.result.problem = problem.map(str::to_owned);
    View::Item(check)
}

/// The "Base percentile" row of a piece of armour, switched off, last of the filters (the rows list the properties first): where
/// its defence lies in the range its base type rolls it in.
fn add_base_percentile(filters: &mut Vec<Filter>, mods: &[Mod], props: &Properties, base: &str) {
    let Some(percentile) = base_percentile(mods, props, zenith_data::base_defences(base)) else { return };
    let value = f64::from(percentile);
    filters.push(Filter { id: BASE_PERCENTILE.to_owned(), min: Some(value), max: None, on: false, label: format!("Base percentile {percentile}%"), tier: None });
}

/// Every line of an implicit mod as a mod of its own.
fn one_line_implicits(mods: Vec<Mod>) -> Vec<Mod> {
    mods.into_iter()
        .flat_map(|m| {
            if m.kind == ModKind::Implicit && m.lines.len() > 1 {
                m.lines.iter().map(|line| Mod { lines: vec![line.clone()], ..m.clone() }).collect()
            } else {
                vec![m]
            }
        })
        .collect()
}

/// The damage, critical strike and speed rows of a unique weapon, all off unless the unique has
/// damage that rolls: then the one of physical and elemental damage that is larger starts on.
fn unique_weapon_filters(mods: &[Mod], props: &Properties) -> Vec<Filter> {
    let ranged = |m: &Mod| m.lines.iter().any(|line| line.ranges.iter().any(Option::is_some));
    let damage = |m: &Mod| m.kind != ModKind::Implicit && m.lines.iter().any(|line| line.template.contains("Damage"));
    // A plain copy has no roll ranges: any damage line may roll.
    let rolled = if mods.iter().any(ranged) { mods.iter().any(|m| damage(m) && ranged(m)) } else { mods.iter().any(damage) };
    let mut filters = default_filters(&[], props, Local::Weapon, &Stats::new());
    for filter in &mut filters {
        filter.on = filter.on && rolled && is_damage_row(&filter.id);
    }
    filters
}

fn is_damage_row(id: &str) -> bool {
    id == zenith_core::matching::PHYSICAL_DPS || id == zenith_core::matching::ELEMENTAL_DPS
}

/// Whether a mod of a unique is one of its fixed lines: no value in it rolls, so it is the same
/// on every copy and changes no price. Only an item copied with its roll ranges can say so; the
/// others (a plain copy) keep every mod. Sockets, and what is not the unique's own text
/// (crafted, fractured, enchants, corruption implicits and foulborn mods), always stay.
fn is_fixed(m: &Mod) -> bool {
    let own = matches!(m.kind, ModKind::Unique | ModKind::Implicit);
    own && !m.crafted
        && !m.corruption
        && !m.fractured
        && !m.foulborn()
        && m.lines.iter().all(|line| line.ranges.iter().all(Option::is_none) && !line.template.contains("Socket"))
}

/// Marks the fixed mods of a unique hidden, and switches their filters off.
fn hide_fixed(rows: &mut [Row], filters: &mut [Filter], mods: &[Mod]) {
    if !mods.iter().flat_map(|m| &m.lines).any(|line| line.ranges.iter().any(Option::is_some)) {
        return;
    }
    let first = rows.len().saturating_sub(mods.len());
    for (at, m) in (first..).zip(mods) {
        let filter = rows[at].filter.and_then(|index| filters.get_mut(index));
        if let (true, Some(filter)) = (is_fixed(m), filter) {
            filter.on = false;
        }
    }
    for (at, row) in rows.iter_mut().enumerate() {
        row.hidden = at >= first && is_fixed(&mods[at - first]);
    }
}

/// Marks the rows of a unique's variant pool, and shows them whatever else hides them. A variant
/// line without a rolled number is searched by default; one with a rolled number is shown, off.
/// What the small passives of a cluster jewel grant, by the line without its numbers, and the
/// short name the panel gives it. The first line of a pair (`Axe ...` then `Sword ...`) names both.
const CLUSTER_SMALL: [(&str, &str); 51] = [
    ("Axe Attacks deal #% increased Damage with Hits and Ailments", "Axe and Sword damage"),
    ("Staff Attacks deal #% increased Damage with Hits and Ailments", "Staff and Mace damage"),
    ("Claw Attacks deal #% increased Damage with Hits and Ailments", "Claw and Dagger damage"),
    ("#% increased Damage with Bows", "Bow damage"),
    ("Wand Attacks deal #% increased Damage with Hits and Ailments", "Wand damage"),
    ("#% increased Damage with Two Handed Weapons", "Two-handed damage"),
    ("#% increased Attack Damage while Dual Wielding", "Dual wield damage"),
    ("#% increased Attack Damage while holding a Shield", "Shield attack damage"),
    ("#% increased Attack Damage", "Attack damage"),
    ("#% increased Spell Damage", "Spell damage"),
    ("#% increased Elemental Damage", "Elemental damage"),
    ("#% increased Physical Damage", "Physical damage"),
    ("#% increased Fire Damage", "Fire damage"),
    ("#% increased Lightning Damage", "Lightning damage"),
    ("#% increased Cold Damage", "Cold damage"),
    ("#% increased Chaos Damage", "Chaos damage"),
    ("Minions deal #% increased Damage", "Minion damage"),
    ("#% increased Burning Damage", "Burning damage"),
    ("#% increased Chaos Damage over Time", "Chaos damage over time"),
    ("#% increased Physical Damage over Time", "Physical damage over time"),
    ("#% increased Cold Damage over Time", "Cold damage over time"),
    ("#% increased Damage over Time", "Damage over time"),
    ("#% increased Effect of Non-Damaging Ailments", "Non-damaging ailments"),
    ("#% increased effect of Non-Curse Auras from your Skills", "Aura effect"),
    ("#% increased Effect of your Curses", "Curse effect"),
    ("#% increased Damage while affected by a Herald", "Herald damage"),
    ("Minions deal #% increased Damage while you are affected by a Herald", "Minion Herald damage"),
    ("Exerted Attacks deal #% increased Damage", "Exerted attack damage"),
    ("#% increased Critical Strike Chance", "Critical strike chance"),
    ("Minions have #% increased maximum Life", "Minion life"),
    ("#% increased Area Damage", "Area damage"),
    ("#% increased Projectile Damage", "Projectile damage"),
    ("#% increased Trap Damage", "Trap and Mine damage"),
    ("#% increased Totem Damage", "Totem damage"),
    ("#% increased Brand Damage", "Brand damage"),
    ("Channelling Skills deal #% increased Damage", "Channelling damage"),
    ("#% increased Flask Effect Duration", "Flask duration"),
    ("#% increased Life Recovery from Flasks", "Flask recovery"),
    ("#% increased maximum Life", "Life"),
    ("#% increased maximum Energy Shield", "Energy shield"),
    ("#% increased maximum Mana", "Mana"),
    ("#% increased Armour", "Armour"),
    ("#% increased Evasion Rating", "Evasion"),
    ("+#% Chance to Block Attack Damage", "Attack block"),
    ("#% Chance to Block Spell Damage", "Spell block"),
    ("+#% to Fire Resistance", "Fire resistance"),
    ("+#% to Cold Resistance", "Cold resistance"),
    ("+#% to Lightning Resistance", "Lightning resistance"),
    ("+#% to Chaos Resistance", "Chaos resistance"),
    ("+#% chance to Suppress Spell Damage", "Spell suppression"),
    ("#% increased Mana Reservation Efficiency of Skills", "Reservation efficiency"),
];

/// A text with every number written `#`.
fn hashed(text: &str) -> String {
    let mut out = String::new();
    let mut in_number = false;
    for c in text.chars() {
        if c.is_ascii_digit() || (in_number && c == '.') {
            if !in_number {
                out.push('#');
            }
            in_number = true;
        } else {
            in_number = false;
            out.push(c);
        }
    }
    out
}

/// A line of a cluster jewel as short as a player names it: `Herald damage` for what its small
/// passives grant, `5 passives`, a notable by its name alone.
fn cluster_short(line: &str) -> Option<String> {
    if let Some(rest) = line.strip_prefix(CLUSTER_GRANT) {
        let rest = hashed(rest);
        return CLUSTER_SMALL.iter().find(|(text, _)| *text == rest).map(|(_, short)| (*short).to_owned());
    }
    if let Some(count) = line.strip_prefix("Adds ").and_then(|rest| rest.strip_suffix(" Passive Skills")) {
        return Some(format!("{count} passives"));
    }
    if let Some(rest) = line.strip_prefix("Added Small Passive Skills also grant: ") {
        return Some(format!("Small: {rest}"));
    }
    line.strip_prefix("1 Added Passive Skill is ").filter(|_| !is_cluster_socket(line)).map(str::to_owned)
}

/// The second line of a pair of what the small passives of a cluster jewel grant: the copy writes
/// it on its own, and the first line names both.
const CLUSTER_SECOND: [&str; 6] = [
    "Sword Attacks deal #% increased Damage with Hits and Ailments",
    "Mace or Sceptre Attacks deal #% increased Damage with Hits and Ailments",
    "Dagger Attacks deal #% increased Damage with Hits and Ailments",
    "#% increased Damage Over Time with Bow Skills",
    "#% increased Mine Damage",
    "#% increased Mana Recovery from Flasks",
];

const CLUSTER_GRANT: &str = "Added Small Passive Skills grant: ";

/// The jewel sockets of a cluster jewel, which come with its size: one for a medium, two for a
/// large.
fn is_cluster_socket(line: &str) -> bool {
    line.ends_with("Added Passive Skill is a Jewel Socket") || line.ends_with("Added Passive Skills are Jewel Sockets")
}

/// Every line of a cluster jewel the panel can say shorter; its sockets are behind the Hidden
/// switch. A pair of what the small passives grant is named by its first line.
fn shorten_cluster(rows: &mut [Row]) {
    for row in rows.iter_mut() {
        let second = |line: &String| line.strip_prefix(CLUSTER_GRANT).is_some_and(|rest| CLUSTER_SECOND.contains(&hashed(rest).as_str()));
        if row.lines.first().is_some_and(|line| is_cluster_socket(line) || second(line)) {
            row.hidden = true;
            continue;
        }
        let Some(short) = row.lines.first().and_then(|line| cluster_short(line)) else { continue };
        if row.lines.iter().all(|line| line.starts_with(CLUSTER_GRANT)) {
            row.lines = vec![short];
        } else {
            row.lines[0] = short;
        }
    }
}

/// A map that holds a boss (a conqueror's citadel, an Elder guardian) gets a row for any boss of
/// its family, off: the base view shows and asks it in place of the very boss.
fn any_boss(rows: &mut Vec<Row>, filters: &mut Vec<zenith_core::matching::Filter>) {
    let found = rows.iter().find_map(|row| {
        let (stat, _) = filters.get(row.filter?)?.id.split_once('|')?;
        let (family, name) = crate::check::MAP_BOSSES.iter().find(|(family, _)| *family == stat)?;
        Some((row.clone(), *family, *name))
    });
    let Some((mut row, family, name)) = found else { return };
    filters.push(zenith_core::matching::Filter { id: family.to_owned(), min: None, max: None, on: false, label: name.to_owned(), tier: None });
    row.lines = vec![name.to_owned()];
    row.id = Some(family.to_owned());
    row.filter = Some(filters.len() - 1);
    rows.push(row);
}

fn mark_variants(rows: &mut [Row], filters: &mut [Filter], mods: &[Mod], variants: &[String]) {
    if variants.is_empty() {
        return;
    }
    let plain = |t: &str| t.trim_start_matches(['~', '+']).to_owned();
    let first = rows.len().saturating_sub(mods.len());
    for (at, m) in (first..).zip(mods) {
        let found = m.lines.iter().find_map(|line| variants.iter().find(|v| plain(v) == plain(&line.template)));
        let Some(found) = found else { continue };
        rows[at].variant = true;
        rows[at].hidden = false;
        if let Some(filter) = rows[at].filter.and_then(|index| filters.get_mut(index)) {
            filter.on = !found.starts_with('~');
        }
    }
}

/// The defence and damage lines that a property row of the item already adds up: the local
/// mods that feed it. A partner line of a hybrid (stun recovery, accuracy) goes with them.
fn is_covered_line(template: &str, weapon: bool) -> bool {
    const DEFENCES: [&str; 7] = ["Armour", "Evasion Rating", "Energy Shield", "Armour and Evasion", "Armour and Energy Shield", "Evasion and Energy Shield", "Armour, Evasion and Energy Shield"];
    let t = template.trim_start_matches('+');
    if weapon {
        return matches!(t, "#% increased Physical Damage" | "#% increased Attack Speed")
            || (t.starts_with("Adds # to # ") && ["Physical", "Fire", "Cold", "Lightning"].iter().any(|kind| t.ends_with(&format!("{kind} Damage"))));
    }
    // A shield's chance to block row covers its local block mod.
    t == "#% increased Chance to Block" || DEFENCES.iter().any(|d| t == format!("# to {d}") || t == format!("#% increased {d}") || t == format!("# to maximum {d}"))
}

/// Marks the local mods that a total defence or damage row covers off and hidden: the row asks for
/// what they add up to.
fn hide_covered(rows: &mut [Row], filters: &mut [Filter], mods: &[Mod], local: Local) {
    use zenith_core::matching::{ARMOUR, ENERGY_SHIELD, EVASION};
    let weapon = local == Local::Weapon;
    let has = |ids: &[&str]| filters.iter().any(|f| ids.contains(&f.id.as_str()));
    let covered = if weapon { has(&[zenith_core::matching::PHYSICAL_DPS, zenith_core::matching::ELEMENTAL_DPS]) } else { local == Local::Armour && has(&[ARMOUR, EVASION, ENERGY_SHIELD]) };
    if !covered {
        return;
    }
    let first = rows.len().saturating_sub(mods.len());
    for (at, m) in (first..).zip(mods) {
        let partner = |t: &str| if weapon { t == "+# to Accuracy Rating" } else { t == "#% increased Stun and Block Recovery" };
        let own = matches!(m.kind, ModKind::Prefix | ModKind::Suffix | ModKind::Other(_)) && !m.fractured && !m.crafted && !m.foulborn();
        let lines = || m.lines.iter().map(|line| line.template.as_str());
        if own && lines().any(|t| is_covered_line(t, weapon)) && lines().all(|t| is_covered_line(t, weapon) || partner(t)) {
            if let Some(filter) = rows[at].filter.and_then(|index| filters.get_mut(index)) {
                filter.on = false;
            }
            rows[at].hidden = true;
        }
    }
}

/// The filters of an item searched by its very base: every mod that has one is asked for, from
/// the value the item has when its stat counts one.
fn exact_filters(filters: &mut [Filter], rows: &[Row]) {
    for row in rows.iter().filter(|row| row.kind.is_some() && !row.id.as_deref().is_some_and(is_pseudo)) {
        if let Some(filter) = row.filter.and_then(|index| filters.get_mut(index)) {
            filter.on = true;
            if row.value.is_some() {
                filter.min = row.value;
            }
        }
    }
}

/// A view priced this way from the start, when a way is given and the item has it to choose: as
/// `--price --way` asks. Nothing is out of date: it is what the first search will be.
pub fn priced_as(mut view: View, way: Option<Way>) -> View {
    if let (View::Item(check), Some(way)) = (&mut view, way) {
        let stale = check.stale;
        apply(check, &Action::SetWay(way));
        check.stale = stale;
    }
    view
}

/// A view that would only say there is nothing to say: not an item, an item that is not priced,
/// a stack with no price and no offer. The price-check key opens no panel for it.
pub fn is_useless(view: &View) -> bool {
    match view {
        View::Message(message) => message == NOT_AN_ITEM,
        View::Item(check) => {
            let priceless = check.result.quote.as_ref().and_then(|quote| quote.each).is_none() && check.result.bulk.is_empty();
            !check.result.loading && (check.plan == Plan::Nothing || (check.plan == Plan::Stack && priceless))
        }
        _ => false,
    }
}

/// The first view of a check, shown before the price list is asked: an item that the list may
/// price is waited for, not said to be unpriced. The price-check thread answers it.
pub fn awaiting_quote(mut view: View) -> View {
    if let View::Item(check) = &mut view
        && check.plan == Plan::Nothing
        && quote::may_be_stackable(&check.item)
    {
        check.result.problem = None;
        check.result.loading = true;
    }
    view
}

/// What the cached price list adds to a view that has just been read. A currency, a card or any
/// item the list knows by name that is priced nowhere else becomes a stack: its whole answer is
/// the quote, and no search follows. Without a list, or without the name, it says so. A unique
/// carries its cached price, to be shown until its listings arrive; any other searched item
/// carries the divine rate, for the other currency of its estimate.
pub fn quoted(mut view: View, list: Option<&PriceList>, league: Option<&str>) -> View {
    let View::Item(check) = &mut view else {
        return view;
    };
    if check.plan == Plan::Nothing && quote::is_stackable(&check.item, list) {
        let quote = quote::quote_of(&check.item, list);
        let problem = match &quote {
            None => Some(quote::NO_PRICE_DATA.to_owned()),
            Some(Quote { each: None, .. }) => Some(format!("No price found for {}", quote::name_of(&check.item))),
            Some(_) => None,
        };
        check.plan = Plan::Stack;
        check.result = Outcome { league: league.map(str::to_owned), quote, problem, ..Outcome::default() };
    } else if check.searchable() {
        let unique = check.plan == Plan::Unique;
        check.result.quote = quote::quote_of(&check.item, list).map(|mut quote| {
            if !unique {
                quote.each = None;
                quote.trend.clear();
            }
            quote
        });
    }
    view
}

/// The divine rate for the corner of the header: `1 div = 210 c`. Nothing without a price list.
pub fn rate_text(quote: Option<&Quote>) -> Option<String> {
    let rate = quote?.divine.filter(|rate| *rate > 0.0)?;
    Some(format!("1 div = {} c", estimate_text(rate)))
}

/// The name of a currency paid, as shown.
pub fn bulk_unit(have: &str) -> &str {
    if have == "divine" { "div" } else { have }
}

/// One ratio as the exchange shows it: `1 : 6.2 chaos`, or `5 : 1 chaos` for an item that costs
/// less than one of the currency.
pub fn ratio_text(each: f64, have: &str) -> String {
    let unit = bulk_unit(have);
    if each > 0.0 && each < 1.0 {
        let per = 1.0 / each;
        format!("{} : 1 {unit}", trimmed(per, if per < 10.0 { 1 } else { 0 }))
    } else {
        format!("1 : {} {unit}", estimate_text(each))
    }
}

/// The median line of a set: `live ≈ 6.2 chaos`, for each item.
pub fn live_text(bulk: &zenith_trade::Bulk) -> Option<String> {
    Some(format!("live ≈ {} {}", estimate_text(bulk.median?), bulk_unit(&bulk.have)))
}

/// The lines of the report for a set of offers.
#[cfg_attr(not(feature = "dev"), allow(dead_code))] // only the console report of `--price` uses it
pub fn bulk_lines(bulk: &zenith_trade::Bulk) -> Vec<String> {
    let mut lines = vec![format!("bulk:     {} (now)", live_text(bulk).unwrap_or_default())];
    for offer in &bulk.offers {
        lines.push(format!("  {} | stock {} | {}", ratio_text(offer.each(), &bulk.have), offer.stock, offer.seller));
    }
    lines
}

/// A view as a new check would have it with these pins.
#[cfg(feature = "dev")]
pub fn pinned(mut view: View, pins: &Config) -> View {
    if let View::Item(check) = &mut view {
        pins.apply(&mut check.options);
    }
    view
}

/// What is known from the copied text alone: the view the panel shows at once.
pub fn first_view(text: &str, pack: Option<&Pack>) -> View {
    read_with(text, pack, &Config::default(), false)
}

/// The view of a piece of gear that cannot be searched because there is no data pack: its mods
/// stay listed, with the reason.
pub fn without_data(mut view: View) -> View {
    if let View::Item(check) = &mut view {
        check.result = Outcome { problem: Some(NO_GAME_DATA.into()), ..Outcome::default() };
    }
    view
}

/// The base type of the item as the site names it. The only line of a magic item holds its
/// affixes too: its base is the longest base of the pack that the line has as whole words.
fn base_of(item: &Item, pack: &Pack) -> Option<String> {
    if item.rarity != Rarity::Magic {
        return Some(item.base.clone());
    }
    let line = format!(" {} ", item.base);
    pack.bases
        .values()
        .flatten()
        .filter(|base| line.contains(&format!(" {base} ")))
        .max_by_key(|base| base.len())
        .cloned()
}

/// Which texts of an item of this class the site lists as local.
pub fn local_of(class: &str) -> Local {
    match zenith_data::category(class) {
        Some(category) if category.starts_with("weapon.") => Local::Weapon,
        Some(_) if zenith_data::is_local_class(class) => Local::Armour,
        _ => Local::None,
    }
}

/// The rows of the panel: the properties that can be searched, then every mod, each pointing at
/// its filter. `filters` are the search's, empty when none of its mods can be asked for;
/// `stats` is missing without a pack. A property whose filter holds the value at 20% quality
/// (`props` says which) is marked so.
fn rows_of(mods: &[Mod], local: Local, stats: Option<&Stats>, filters: &[Filter], props: &Properties) -> Vec<Row> {
    let mut rows: Vec<Row> = filters
        .iter()
        .enumerate()
        .filter(|(_, filter)| is_property(&filter.id))
        .map(|(index, filter)| {
            // "Energy shield 498": the name, then the value.
            let (name, figure) = match filter.label.rsplit_once(' ') {
                Some((name, figure)) => (name, Some(figure.to_owned())),
                None => (filter.label.as_str(), None),
            };
            Row {
                kind: None,
                tier: None,
                roll: None,
                lines: vec![name.to_owned()],
                figure,
                q20: at_trade_quality(&filter.id, props),
                id: Some(filter.id.clone()),
                filter: Some(index),
                unmatched: false,
                fractured: false,
                crafted: false,
                foulborn: false,
                origin: None,
                corruption: false,
                hidden: false,
                implicit: false,
                variant: false,
                value: None,
                range: None,
            }
        })
        .collect();

    // The pseudo totals, above the mods they add up. A total has no tier; its slider goes through
    // the summed ranges of its lines.
    let no_stats = Stats::new();
    let totals = pseudo_totals(mods, local, stats.unwrap_or(&no_stats));
    for (index, filter) in filters.iter().enumerate().filter(|(_, filter)| is_pseudo(&filter.id)) {
        let total = totals.iter().find(|total| total.id == filter.id);
        rows.push(Row {
            kind: Some("Pseudo".into()),
            tier: None,
            roll: None,
            lines: vec![filter.label.clone()],
            figure: None,
            q20: false,
            id: Some(filter.id.clone()),
            filter: Some(index),
            unmatched: false,
            fractured: false,
            crafted: false,
            foulborn: false,
            origin: None,
            corruption: false,
            hidden: false,
            implicit: false,
            variant: false,
            value: total.map(|total| total.value),
            range: total.and_then(|total| total.range),
        });
    }

    // `default_filters` makes one filter per stat, in the order of the mods: a mod whose stat is
    // the one of the filter just given joins it. `unique_filters` makes one per matched mod.
    let mut of_mods = filters.iter().enumerate().filter(|(_, filter)| !is_property(&filter.id) && !is_pseudo(&filter.id)).peekable();
    let mut given: Vec<(usize, &Filter)> = Vec::new();
    for m in mods {
        let matched = match_mod(m, local, stats.unwrap_or(&no_stats));
        let filter = matched.id.as_ref().and_then(|id| {
            if of_mods.peek().is_some_and(|(_, filter)| filter.id == *id) {
                let next = of_mods.next()?;
                given.push(next);
                Some(next)
            } else {
                given.iter().copied().find(|(_, filter)| filter.id == *id)
            }
        });
        rows.push(Row {
            kind: Some(kind_label(m)),
            tier: m.tier,
            roll: roll_of(m),
            lines: matched.label.lines().map(str::to_owned).collect(),
            figure: None,
            q20: false,
            unmatched: matched.id.is_none() && stats.is_some(),
            id: matched.id,
            filter: filter.map(|(index, _)| index),
            fractured: m.fractured,
            crafted: m.crafted,
            foulborn: m.foulborn(),
            origin: m.origin(),
            corruption: m.corruption,
            hidden: false,
            implicit: is_implicit(&m.kind),
            variant: false,
            value: matched.value,
            range: matched.range,
        });
    }
    sum_shared(&mut rows);
    rows
}

/// Rows that share a filter ask for the sum of their values: each of them gets the summed value
/// and the summed range, which is what the filter's slider goes through.
fn sum_shared(rows: &mut [Row]) {
    for at in 0..rows.len() {
        let Some(filter) = rows[at].filter.filter(|_| rows[at].kind.is_some()) else { continue };
        let sharing: Vec<usize> = (0..rows.len()).filter(|i| rows[*i].filter == Some(filter) && rows[*i].kind.is_some()).collect();
        if sharing.len() < 2 || sharing[0] != at {
            continue;
        }
        let value = sharing.iter().filter_map(|i| rows[*i].value).reduce(|a, b| a + b);
        let ranges: Vec<(f64, f64)> = sharing.iter().filter_map(|i| rows[*i].range).collect();
        let range = (!ranges.is_empty()).then(|| ranges.iter().fold((0.0, 0.0), |sum, r| (sum.0 + r.0, sum.1 + r.1)));
        for i in sharing {
            rows[i].value = value;
            rows[i].range = range;
        }
    }
}

/// Whether the game writes a mod of this kind with the implicit ones, apart from the others:
/// the implicit of the base, and those a wording this version does not know still calls so
/// (`Searing Exarch Implicit`).
fn is_implicit(kind: &ModKind) -> bool {
    match kind {
        ModKind::Implicit => true,
        ModKind::Other(wording) => wording.to_ascii_lowercase().contains("implicit"),
        _ => false,
    }
}

/// `Prefix`, `Implicit`, `Crafted Suffix`, `Fractured explicit`: the kind of a mod with its marks.
fn kind_label(m: &Mod) -> String {
    let kind = match &m.kind {
        ModKind::Implicit => "Implicit",
        ModKind::Prefix => "Prefix",
        ModKind::Suffix => "Suffix",
        ModKind::Unique => "Unique",
        ModKind::Enchant => "Enchant",
        ModKind::Other(wording) => wording,
    };
    // A wording kept whole says it already.
    let mark = |on: bool, word: &'static str| if on && !kind.contains(word) { word } else { "" };
    format!("{}{}{kind}", mark(m.fractured, "Fractured "), mark(m.crafted, "Crafted "))
}

/// Where the mod's values sit in the ranges they were rolled in: the mean over every number that
/// has a range with two ends, 0 at the low end and 1 at the high end.
fn roll_of(m: &Mod) -> Option<f32> {
    let places: Vec<f64> = m
        .lines
        .iter()
        .flat_map(|line| line.values.iter().zip(&line.ranges))
        .filter_map(|(value, range)| {
            let (low, high) = (*range)?;
            if low == high {
                return None;
            }
            let inside = |value: f64| (low.min(high)..=low.max(high)).contains(&value);
            // The sign may be written before the number and left out of the range.
            let value = if !inside(*value) && inside(-*value) { -*value } else { *value };
            Some(((value - low) / (high - low)).clamp(0.0, 1.0))
        })
        .collect();
    (!places.is_empty()).then(|| (places.iter().sum::<f64>() / places.len() as f64) as f32)
}

/// The sentence the panel shows for a refusal or a failure of the trade site.
pub fn problem_for(error: &TradeError) -> String {
    match error {
        TradeError::RateLimited { wait_s } => format!("Rate limited: wait {wait_s} s"),
        TradeError::Challenge => BROWSER_CHECK.into(),
        TradeError::Api { code, message } => format!("Trade site error {code}: {message}"),
        TradeError::Http(status) => format!("The trade site answered with status {status}"),
        TradeError::Network(reason) => format!("Could not reach the trade site: {reason}"),
    }
}

/// League ids containing one of these are not the current trade league.
const NOT_THE_TRADE_LEAGUE: [&str; 4] = ["Standard", "Hardcore", "Solo", "Ruthless"];

/// The current trade league in the answer of `/api/leagues`: the first id that is no variant of
/// another league. The answer is an array of leagues (also accepted under a `result` key).
pub fn pick_league(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let leagues = value.as_array().or_else(|| value.get("result")?.as_array())?;
    leagues
        .iter()
        .filter_map(|league| league.get("id")?.as_str())
        .find(|id| !NOT_THE_TRADE_LEAGUE.iter().any(|word| id.contains(word)))
        .map(str::to_owned)
}

/// The estimate of the listings fetched so far.
pub fn estimate_of(listings: &[Listing]) -> Option<Estimate> {
    estimate(&prices_of(listings))
}

/// Which listings look too cheap to be real, in the order of `listings`: from the same prices the
/// estimate is made of.
pub fn suspicious_of(listings: &[Listing]) -> Vec<bool> {
    zenith_core::suspicious(&prices_of(listings))
}

fn prices_of(listings: &[Listing]) -> Vec<(f64, String)> {
    listings.iter().map(|listing| (listing.amount, listing.currency.clone())).collect()
}

// ---- wording ------------------------------------------------------------------------------

/// A property as the report writes it: its name and its value, and whether that value is the
/// one at 20% quality and not the item's own.
#[cfg_attr(not(feature = "dev"), allow(dead_code))] // only the console report of `--price` uses it
fn property_text(row: &Row) -> String {
    let mut text = row.lines.join(" / ");
    if let Some(figure) = &row.figure {
        text.push(' ');
        text.push_str(figure);
    }
    if row.q20 {
        text.push_str(" (at 20% quality)");
    }
    text
}

/// The rarity as the game writes it. Nothing for an item that has none.
pub fn rarity_label(rarity: &Rarity) -> &str {
    match rarity {
        Rarity::None => "",
        Rarity::Normal => "Normal",
        Rarity::Magic => "Magic",
        Rarity::Rare => "Rare",
        Rarity::Unique => "Unique",
        Rarity::Gem => "Gem",
        Rarity::Currency => "Currency",
        Rarity::DivinationCard => "Divination Card",
        Rarity::Other(wording) => wording,
    }
}

/// The site's currency ids, shortened where the design does.
pub fn currency_label(id: &str) -> &str {
    match id {
        "divine" => "div",
        other => other,
    }
}

/// The seller as the panel names them: the site's account name without its number
/// (`Seller#0654` is `Seller`). Only digits after the last `#` are a number.
pub fn seller_name(account: &str) -> &str {
    match account.rsplit_once('#') {
        Some((name, number))
            if !name.is_empty() && !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) =>
        {
            name
        }
        _ => account,
    }
}

/// A listed price: two decimals at most, none that say nothing (`3`, `2.5`, `0.33`).
pub fn price_text(amount: f64) -> String {
    trimmed(amount, 2)
}

/// The estimate's figure: fewer decimals as it grows (`0.35`, `3.1`, `120`).
pub fn estimate_text(amount: f64) -> String {
    let decimals = if amount < 1.0 {
        2
    } else if amount < 10.0 {
        1
    } else {
        0
    };
    trimmed(amount, decimals)
}

fn trimmed(amount: f64, decimals: usize) -> String {
    let text = format!("{amount:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

/// "81 listings, 1 ignored": how many the search found and how many prices the estimate set
/// apart as too cheap.
/// "Matched: 112", then how many were fetched when that is fewer, and how many prices were ignored.
pub fn matched_line(total: u32, fetched: usize, ignored: usize) -> String {
    let mut line = format!("Matched: {total}");
    if fetched as u32 != total && fetched > 0 {
        line.push_str(&format!(", {fetched} fetched"));
    }
    if ignored > 0 {
        line.push_str(&format!(", {ignored} ignored"));
    }
    line
}

/// The change over the days of a trend, in percent: the last day with data against the first.
/// `None` with fewer than two days or a first day at nothing.
pub fn trend_change(trend: &[Option<f64>]) -> Option<f64> {
    let known: Vec<f64> = trend.iter().flatten().copied().collect();
    let (first, last) = (*known.first()?, *known.last()?);
    (known.len() >= 2 && first > 0.0).then(|| (last - first) / first * 100.0)
}

/// "-52 %" with a true minus sign, "+8 %".
pub fn change_text(percent: f64) -> String {
    let whole = percent.round() as i64;
    match whole {
        0 => "0 %".to_owned(),
        w if w < 0 => format!("\u{2212}{} %", -w),
        w => format!("+{w} %"),
    }
}

/// "Hidden (3)": the switch's label.
pub fn hidden_label(count: usize) -> String {
    format!("Hidden ({count})")
}

#[cfg_attr(not(feature = "dev"), allow(dead_code))] // only the console report of `--price` uses it
pub fn count_line(listings: u32, ignored: usize) -> String {
    let mut line = format!("{listings} listing{}", if listings == 1 { "" } else { "s" });
    if ignored > 0 {
        line.push_str(&format!(", {ignored} ignored"));
    }
    line
}

// ---- time ---------------------------------------------------------------------------------

/// Seconds since 1970 of an ISO 8601 time such as `2025-01-20T09:15:00Z`, as the trade site
/// writes `listing.indexed`. Fractions of a second are dropped; `+hh:mm` and `-hh:mm` are
/// accepted in place of `Z`.
pub fn parse_utc(text: &str) -> Option<i64> {
    let (date, time) = text.trim().split_once('T')?;
    let mut date = date.splitn(3, '-').map(|part| part.parse::<i64>().ok());
    let (year, month, day) = (date.next()??, date.next()??, date.next()??);

    let zone_at = time.find(['Z', 'z', '+', '-'])?;
    let (clock, zone) = time.split_at(zone_at);
    let mut clock = clock.splitn(3, ':');
    let hour: i64 = clock.next()?.parse().ok()?;
    let minute: i64 = clock.next()?.parse().ok()?;
    let second: i64 = match clock.next() {
        Some(seconds) => seconds.split('.').next()?.parse().ok()?,
        None => 0,
    };
    let offset = match zone {
        "Z" | "z" => 0,
        _ => {
            let (hours, minutes) = zone[1..].split_once(':')?;
            let seconds = hours.parse::<i64>().ok()? * 3600 + minutes.parse::<i64>().ok()? * 60;
            if zone.starts_with('-') { -seconds } else { seconds }
        }
    };
    let in_range = (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && (0..24).contains(&hour)
        && (0..60).contains(&minute)
        && (0..=60).contains(&second);
    in_range.then(|| days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second - offset)
}

/// Days from 1970-01-01 to a date of the Gregorian calendar (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// How long ago a listing was indexed, short: `now`, `14 min`, `1 h`, `2 days`. Empty when the
/// time cannot be read.
pub fn listed_since(indexed: &str, now: i64) -> String {
    let Some(then) = parse_utc(indexed) else {
        return String::new();
    };
    let minutes = (now - then).max(0) / 60;
    match minutes {
        0 => "now".into(),
        1..60 => format!("{minutes} min"),
        60..1440 => format!("{} h", minutes / 60),
        1440..2880 => "1 day".into(),
        _ => format!("{} days", minutes / 1440),
    }
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64)
}

/// The year, month and day of a number of days since 1970-01-01 (Howard Hinnant's
/// `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 { shifted + 3 } else { shifted - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Seconds since 1970 as an ISO 8601 time in UTC, `2025-01-20T09:15:00Z`: what a data pack is
/// stamped with.
pub fn utc_text(seconds: i64) -> String {
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let of_day = seconds.rem_euclid(86_400);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", of_day / 3600, of_day / 60 % 60, of_day % 60)
}

// ---- the data update ------------------------------------------------------------------------

/// How many names of each kind the summary of a data update lists.
const SUMMARY_NAMES: usize = 20;

/// The stamp of a pack as a person reads it: `2025-01-20 09:15 UTC`. A stamp that is not a time
/// is given back as it is.
fn built_text(built: &str) -> String {
    match parse_utc(built) {
        Some(seconds) => {
            let stamp = utc_text(seconds);
            format!("{} {} UTC", &stamp[..10], &stamp[11..16])
        }
        None => built.to_owned(),
    }
}

/// What `--update-data` prints once the new pack is saved: when it was built, how much it holds,
/// then what is new and what is gone since the pack it replaces, with the first names of each.
pub fn update_summary(old: Option<&Pack>, new: &Pack, saved: &str) -> String {
    let stats: usize = new.stats.values().flat_map(|texts| texts.values()).map(Vec::len).sum();
    let bases: usize = new.bases.values().map(Vec::len).sum();
    let mut out = format!(
        "Game data updated.\n\n  Built:    {}\n  Stats:    {stats}\n  Bases:    {bases}\n  Uniques:  {}\n  Saved in: {saved}\n\n",
        built_text(&new.built),
        new.uniques.len()
    );
    let Some(old) = old else {
        out.push_str("This is the first game data on this computer: there is nothing to compare it with.\n");
        return out;
    };
    let Changes { new_stats, gone_stats, new_bases, gone_bases, new_uniques, gone_uniques } = changes(old, new);
    let kinds = [
        ("New stats", new_stats),
        ("Stats that are gone", gone_stats),
        ("New bases", new_bases),
        ("Bases that are gone", gone_bases),
        ("New uniques", new_uniques),
        ("Uniques that are gone", gone_uniques),
    ];
    let since = built_text(&old.built);
    if kinds.iter().all(|(_, names)| names.is_empty()) {
        out.push_str(&format!("Nothing is new and nothing is gone since the previous data, built {since}.\n"));
        return out;
    }
    out.push_str(&format!("Compared with the previous data, built {since}:\n\n"));
    for (title, names) in kinds {
        out.push_str(&format!("  {title}: {}\n", names.len()));
        for name in names.iter().take(SUMMARY_NAMES) {
            // A stat of several lines stays on one.
            out.push_str(&format!("    {}\n", name.replace('\n', " / ")));
        }
        if names.len() > SUMMARY_NAMES {
            out.push_str(&format!("    ... and {} more\n", names.len() - SUMMARY_NAMES));
        }
    }
    out
}

// ---- the console report of `--price` --------------------------------------------------------

/// The view as lines of plain text: header, what a piece of gear is priced as, each mod as
/// `kind | tier | text | trade stat`, the filters of the search with their minimum, what else the
/// search asks of the item, the estimate, one line per listing, then the problem.
#[cfg_attr(not(feature = "dev"), allow(dead_code))] // only the console report of `--price` uses it
pub fn report(view: &View, now: i64) -> String {
    let check = match view {
        View::Message(text) => return format!("problem: {text}\n"),
        View::Item(check) => check,
        View::Launcher => return "quick menu
".to_owned(),
        View::Searches(titles) => return format!("saved searches: {}
", titles.join(", ")),
        View::Maps(maps) => return format!("map: {} (tier {:?}), {} mods, {} dangerous
", maps.name, maps.tier, maps.mods.len(), maps.danger_count()),
    };
    let (item, rows) = (&check.item, &check.rows);
    let Outcome { league, listings, total, estimate, problem, .. } = &check.result;
    let mut out = String::new();
    let mut line = |text: String| {
        out.push_str(&text);
        out.push('\n');
    };
    line(format!("rarity:   {}", Some(rarity_label(&item.rarity)).filter(|label| !label.is_empty()).unwrap_or("none")));
    if let Some(name) = &item.name {
        line(format!("name:     {name}"));
    }
    line(format!("base:     {}", item.base));
    let marks: Vec<&str> = [(item.fractured, "fractured"), (item.corrupted, "corrupted")]
        .into_iter()
        .filter_map(|(marked, word)| marked.then_some(word))
        .collect();
    if !marks.is_empty() {
        line(format!("marked:   {}", marks.join(", ")));
    }
    if check.has_ways() {
        line(format!("way:      {}", check.way_label(check.way)));
    }
    if !rows.is_empty() {
        // The properties that can be searched, searched or not: the defences and the damage per
        // second, each as the site compares it.
        let properties: Vec<String> = rows.iter().filter(|row| row.kind.is_none()).map(property_text).collect();
        if !properties.is_empty() {
            line(format!("props:    {}", properties.join(", ")));
        }
        if rows.iter().any(|row| row.kind.is_some()) {
            line("mods:     kind | tier | text | trade stat".into());
        }
        for row in rows {
            let Some(kind) = &row.kind else {
                continue;
            };
            let tier = row.tier.map_or_else(|| "-".to_owned(), |tier| format!("T{tier}"));
            let stat = match &row.id {
                Some(id) => id.as_str(),
                None if row.unmatched => "UNMATCHED",
                None => "not looked up: no game data",
            };
            line(format!("  {kind} | {tier} | {} | {stat}", row.lines.join(" / ")));
        }
        line("filters:  trade stat or property | minimum | text".into());
        let mut none = true;
        // A filter shared by several mods is one line, with the text of each of them.
        for row in rows.iter().filter(|row| check.mark(row) == Mark::Searched && check.leads(row)) {
            none = false;
            let minimum = check.min_of(row).map_or_else(|| "-".to_owned(), |min| trimmed(min, 2));
            let text = if row.figure.is_some() {
                property_text(row)
            } else {
                let sharing = rows.iter().filter(|other| other.filter == row.filter);
                sharing.map(|other| other.lines.join(" / ")).collect::<Vec<_>>().join(" + ")
            };
            line(format!("  {} | {minimum} | {text}", row.id.as_deref().unwrap_or("-")));
        }
        if none {
            line("  none".into());
        }
    }
    if check.searchable() {
        line(format!("options:  {}", options_line(check)));
        if !check.candidates.is_empty() {
            let pictured = check.candidate_icons.iter().filter(|icon| icon.is_some()).count();
            line(format!("picker:   {} candidates, {pictured} with a picture", check.candidates.len()));
        }
    }
    if let Some(quote) = &check.result.quote {
        for text in quote::report_lines(item, quote, now) {
            line(text);
        }
    }
    for bulk in &check.result.bulk {
        for text in bulk_lines(bulk) {
            line(text);
        }
    }
    if let Some(league) = league {
        line(format!("league:   {league}"));
    }
    if let Some(estimate) = estimate {
        line(format!(
            "estimate: about {} {} (median of {}, {} ignored)",
            estimate_text(estimate.amount),
            currency_label(&estimate.currency),
            estimate.from,
            estimate.ignored
        ));
    }
    if let Some(total) = total {
        line(format!("found:    {}, {} fetched", count_line(*total, 0), listings.len()));
    }
    for listing in listings {
        line(format!(
            "  {:>8} {:<8} {:>8}  {}",
            price_text(listing.amount),
            currency_label(&listing.currency),
            listed_since(&listing.indexed, now),
            seller_name(&listing.seller)
        ));
    }
    if let Some(problem) = problem {
        line(format!("problem:  {problem}"));
    }
    out
}

/// What a search asks of the item besides its filters, as the console report says it: the chips
/// that are on, and whether the listings must be fractured.
#[cfg_attr(not(feature = "dev"), allow(dead_code))] // only the console report of `--price` uses it
fn options_line(check: &Check) -> String {
    let word = |chip: &OptionChip| match (chip.option, chip.number) {
        (ItemOption::Base, _) => format!("base {}", chip.label),
        (ItemOption::ItemLevel, Some(level)) => format!("item level {level}"),
        (ItemOption::Quality, Some(quality)) => format!("quality {quality}"),
        _ => chip.label.to_lowercase(),
    };
    let chips = check.option_chips();
    // What is asked of the item, then of its fracture, then of its corruption.
    let (corruption, others): (Vec<&OptionChip>, Vec<&OptionChip>) =
        chips.iter().filter(|chip| chip.on).partition(|chip| chip.option == ItemOption::Corrupted);
    let fractured = check.options.fractured.map(|fractured| if fractured { "fractured" } else { "not fractured" }.to_owned());
    let asked: Vec<String> = others.into_iter().map(word).chain(fractured).chain(corruption.into_iter().map(word)).collect();
    if asked.is_empty() { "none".to_owned() } else { asked.join(" | ") }
}

// ---- the sample item: what the warm-up frame paints, to fill the glyph atlas --------------------

/// The clock the fake views are drawn at.
pub const DEMO_NOW: &str = "2025-01-20T12:00:00Z";

/// A rare body armour, made up, as the game copies one, but for this: one of its mods is
/// fractured and the item does not say that it is, so that the panel shows a fractured mod and a
/// crafted one side by side while the check still opens as any rare does. The game never copies
/// such an item: the fractured boots further down are as it writes one.
const DEMO_RARE: &str = "Item Class: Body Armours
Rarity: Rare
Kraken Shroud
Vaal Regalia
--------
Quality: +20% (augmented)
Energy Shield: 498 (augmented)
--------
Requirements:
Level: 68
Int: 194
--------
Sockets: B-B-B-B-B B
--------
Item Level: 86
--------
{ Prefix Modifier \"Unassailable\" (Tier: 1) \u{2014} Defences, Energy Shield }
98(92-100)% increased Energy Shield
{ Prefix Modifier \"Seething\" (Tier: 2) \u{2014} Defences, Energy Shield }
+84(77-90) to maximum Energy Shield
{ Prefix Modifier \"Seraphim's\" (Tier: 3) \u{2014} Defences, Energy Shield }
36(33-38)% increased Energy Shield
15(14-15)% increased Stun and Block Recovery
{ Fractured Suffix Modifier \"of the Magma\" (Tier: 2) \u{2014} Elemental, Fire, Resistance }
+44(42-45)% to Fire Resistance
{ Suffix Modifier \"of the Lightning\" (Tier: 5) \u{2014} Elemental, Lightning, Resistance }
+24(24-29)% to Lightning Resistance
{ Master Crafted Suffix Modifier \"of Craft\" }
12(10-12)% increased Effect of Non-Damaging Ailments
";

/// The few stats the fake items are matched with. The ids are made up. The crafted mod of the
/// rare and the last mod of the unique are left out, so that each has one mod no stat matches.
fn demo_pack() -> Pack {
    let mut pack = Pack { built: DEMO_NOW.into(), ..Pack::default() };
    for (group, text, id) in [
        ("explicit", "#% increased Energy Shield (Local)", "explicit.stat_demo_1"),
        ("explicit", "+# to maximum Energy Shield (Local)", "explicit.stat_demo_2"),
        ("explicit", "#% increased Stun and Block Recovery", "explicit.stat_demo_3"),
        ("fractured", "+#% to Fire Resistance", "fractured.stat_demo_4"),
        ("explicit", "+#% to Lightning Resistance", "explicit.stat_demo_5"),
        ("explicit", "+#% to all Elemental Resistances", "explicit.stat_demo_6"),
        ("explicit", "#% increased Elemental Damage", "explicit.stat_demo_7"),
        ("implicit", "+#% to all Elemental Resistances", "implicit.stat_demo_6"),
        ("implicit", "#% increased Elemental Damage", "implicit.stat_demo_7"),
        ("fractured", "#% increased Movement Speed", "fractured.stat_demo_8"),
        ("explicit", "#% increased Armour (Local)", "explicit.stat_demo_9"),
        ("explicit", "+#% to Cold Resistance", "explicit.stat_demo_10"),
        ("implicit", "Map contains Baran's Citadel", "implicit.stat_2563183002|1"),
        ("implicit", "Map is occupied by The Purifier", "implicit.stat_3624393862|4"),
        ("implicit", "Area is influenced by The Elder", "implicit.stat_1792283443|2"),
    ] {
        pack.stats.entry(group.into()).or_default().insert(text.into(), vec![id.into()]);
    }
    pack.uniques = vec![("Bloodboil Band".into(), "Prismatic Ring".into()), ("The Taming".into(), "Prismatic Ring".into()), ("Storm Cloud".into(), "Long Bow".into())];
    for class in ["Body Armours", "Rings", "Bows", "Boots", "Viscera", "Maps"] {
        if let Some(category) = zenith_data::category(class) {
            pack.categories.insert(class.into(), category.into());
        }
    }
    pack
}

/// The fake rare as the hotkey first shows it: its rows, and everything else still to come.
pub fn demo_loading() -> View {
    first_view(DEMO_RARE, Some(&demo_pack()))
}

/// The fake rare with six listings and more on their way: a property and three mods searched,
/// a mod of two lines and another one not searched, and a mod no stat matches. One listing is
/// corrupted, and the sellers are account names as the site gives them. No request is behind it.
pub fn demo_view() -> View {
    with_demo_listings(demo_loading())
}

/// What the fake search brings: six listings and more on their way.
pub fn demo_outcome() -> Outcome {
    let listing = |amount: f64, indexed: &str, seller: &str| Listing {
        id: seller.to_owned(),
        amount,
        currency: "divine".into(),
        indexed: indexed.into(),
        seller: seller.into(),
        corrupted: seller.starts_with("Dunmere"),
        item_level: Some(84),
        gem_level: Some(20),
        stored_experience: None,
        quality: Some(if indexed.contains("T11:") { 23 } else { 20 }),
        icon: None,
        tiers: Vec::new(),
        item: None,
    };
    let listings = vec![
        listing(2.5, "2025-01-20T11:46:00Z", "Orvane#0654"),
        listing(3.0, "2025-01-20T11:00:00Z", "Maelith#2291"),
        listing(3.0, "2025-01-20T10:00:00Z", "Tessaly_K#0007"),
        listing(3.2, "2025-01-20T09:00:00Z", "Dunmere#4410"),
        listing(3.5, "2025-01-20T07:00:00Z", "Quillon#1836"),
        listing(4.0, "2025-01-19T11:00:00Z", "Sablewick#5120"),
    ];
    Outcome {
        league: Some("Standard".into()),
        estimate: estimate_of(&listings),
        listings,
        total: Some(81),
        loading: true,
        problem: None,
        note: None,
        query_id: Some("demo".into()),
        quote: Some(Quote { divine: Some(162.0), ..Quote::default() }),
        bulk: Vec::new(),
    }
}

fn with_demo_listings(mut view: View) -> View {
    view.take(Step::Found(demo_outcome()));
    view
}
