//! Builds the body of a trade search request.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// Which sellers a search keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Securable,
    Available,
    Online,
    Any,
}

/// The currency a listing must be priced in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Currency {
    ChaosDivine,
    Chaos,
    Divine,
}

/// The oldest a listing may be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Listed {
    #[serde(rename = "1day")]
    Day1,
    #[serde(rename = "3days")]
    Days3,
    #[serde(rename = "1week")]
    Week1,
}

/// The ids of the site's filters for the influences and the synthesis, in the order of the bits of
/// `SearchOptions::influences`.
pub const INFLUENCE_FILTERS: [&str; 9] = [
    "shaper_item",
    "elder_item",
    "crusader_item",
    "redeemer_item",
    "hunter_item",
    "warlord_item",
    "searing_item",
    "tangled_item",
    "synthesised_item",
];

/// `None` leaves the corresponding filter out of the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchOptions {
    pub status: Status,
    pub currency: Option<Currency>,
    pub listed: Option<Listed>,
    /// Keep corrupted listings only, or only those that are not.
    pub corrupted: Option<bool>,
    /// The influences and the synthesis the listing must have, as bits in the order of
    /// `INFLUENCE_FILTERS`.
    pub influences: u16,
    pub mirrored: Option<bool>,
    pub split: Option<bool>,
    pub identified: Option<bool>,
    /// Keep Foulborn listings only, or only those that are not; `None` for both.
    pub mutated: Option<bool>,
    /// Keep vestigial listings only, or only those that are not; `None` for both.
    pub vestigial: Option<bool>,
    /// Imbued gems only, or only those that are not; `None` for both.
    pub imbued: Option<bool>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            status: Status::Securable,
            currency: Some(Currency::ChaosDivine),
            listed: Some(Listed::Days3),
            corrupted: None,
            influences: 0,
            mirrored: None,
            split: None,
            identified: None,
            mutated: None,
            vestigial: None,
            imbued: None,
        }
    }
}

/// A stat a search asks for: its id, its minimum and its maximum.
pub type Stat = (String, Option<f64>, Option<f64>);

/// The search request for a unique item, cheapest first. `links_min`, when given, is the least
/// size of its largest group of linked sockets. Every stat of `stats` must be on the item,
/// within its bounds; without any, the item is searched by its name alone.
pub fn unique_request(
    name: &str,
    base: &str,
    links_min: Option<u32>,
    stats: &[Stat],
    opts: &SearchOptions,
) -> Value {
    let mut sockets = Map::new();
    add_range(&mut sockets, "links", links_min, None);
    let mut filters = Map::new();
    add_group(&mut filters, "socket_filters", sockets);
    add_group(&mut filters, "trade_filters", trade_filters(opts));
    add_group(&mut filters, "misc_filters", misc_filters(opts));
    json!({
        "query": {
            "status": { "option": opts.status },
            "name": name,
            "type": base,
            "stats": [{ "type": "and", "filters": stat_filters(stats) }],
            "filters": filters
        },
        "sort": { "price": "asc" }
    })
}

/// The weapon filters of a search: damage per second, critical strike chance, attack speed.
fn weapon_filters(search: &GearSearch<'_>) -> Map<String, Value> {
    let mut weapon = Map::new();
    add_range(&mut weapon, "pdps", search.pdps_min, search.pdps_max);
    add_range(&mut weapon, "edps", search.edps_min, search.edps_max);
    add_range(&mut weapon, "dps", search.dps_min, search.dps_max);
    add_range(&mut weapon, "crit", search.crit_min, search.crit_max);
    add_range(&mut weapon, "aps", search.aps_min, search.aps_max);
    weapon
}

/// Adds the weapon filters of `search` to a request built without them, such as a unique's.
pub fn add_weapon_filters(request: &mut Value, search: &GearSearch<'_>) {
    let weapon = weapon_filters(search);
    if weapon.is_empty() {
        return;
    }
    if let Some(filters) = request.pointer_mut("/query/filters").and_then(Value::as_object_mut) {
        add_group(filters, "weapon_filters", weapon);
    }
}

/// The armour filters of a search: defences, chance to block, where the defence lies in the range
/// of its base type.
fn armour_filters(search: &GearSearch<'_>) -> Map<String, Value> {
    let mut armour = Map::new();
    add_range(&mut armour, "ar", search.armour_min, search.armour_max);
    add_range(&mut armour, "ev", search.evasion_min, search.evasion_max);
    add_range(
        &mut armour,
        "es",
        search.energy_shield_min,
        search.energy_shield_max,
    );
    add_range(&mut armour, "block", search.block_min, search.block_max);
    add_range(&mut armour, "base_defence_percentile", search.base_percentile_min, None);
    armour
}

/// Adds the armour filters of `search` to a request built without them, such as a unique's.
pub fn add_armour_filters(request: &mut Value, search: &GearSearch<'_>) {
    let armour = armour_filters(search);
    if armour.is_empty() {
        return;
    }
    if let Some(filters) = request.pointer_mut("/query/filters").and_then(Value::as_object_mut) {
        add_group(filters, "armour_filters", armour);
    }
}

/// Adds the least quality to a request built without it, such as a unique's.
pub fn add_quality(request: &mut Value, quality_min: Option<u32>) {
    let mut quality = Map::new();
    add_range(&mut quality, "quality", quality_min, None);
    if quality.is_empty() {
        return;
    }
    let Some(filters) = request.pointer_mut("/query/filters").and_then(Value::as_object_mut) else { return };
    let group = filters.entry("misc_filters").or_insert_with(|| json!({ "filters": {} }));
    if let Some(group) = group.get_mut("filters").and_then(Value::as_object_mut) {
        group.extend(quality);
    }
}

/// A search for a skill or support gem by its name: `base` is the gem as the site lists it, and
/// `disc` tells a transfigured gem from it. The least level and quality are asked as the item has
/// them; no mod is asked of a gem.
pub fn gem_request(
    base: &str,
    disc: Option<&str>,
    level_min: Option<u32>,
    quality_min: Option<u32>,
    stats: &[Stat],
    opts: &SearchOptions,
) -> Value {
    let mut misc = Map::new();
    add_range(&mut misc, "gem_level", level_min, None);
    add_range(&mut misc, "quality", quality_min, None);
    misc.extend(misc_filters(opts));
    let mut filters = Map::new();
    add_group(&mut filters, "misc_filters", misc);
    add_group(&mut filters, "trade_filters", trade_filters(opts));
    let kind = match disc {
        Some(disc) => json!({ "option": base, "discriminator": disc }),
        None => json!(base),
    };
    json!({
        "query": {
            "status": { "option": opts.status },
            "type": kind,
            "stats": [{ "type": "and", "filters": stat_filters(stats) }],
            "filters": filters
        },
        "sort": { "price": "asc" }
    })
}

/// What a search for a piece of gear asks for. `None`, `false` and an empty list each
/// leave their filter out; a search that asks for nothing is still a valid request.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GearSearch<'a> {
    /// The id of an item category of the site, such as `armour.chest`.
    pub category: Option<&'a str>,
    /// The base type, such as `Vaal Regalia`.
    pub base: Option<&'a str>,
    /// Magic items only; it wins over `rarity_nonunique`.
    pub rarity_magic: bool,
    /// Leave unique items out.
    pub rarity_nonunique: bool,
    /// Unique items only: an unidentified unique is searched by its base.
    pub rarity_unique: bool,
    /// Stat id, minimum and maximum. Every stat must be on the item.
    pub stats: &'a [Stat],
    pub item_level_min: Option<u32>,
    pub quality_min: Option<u32>,
    /// The most quality a listing may have; 0 leaves out the catalysed jewellery.
    pub quality_max: Option<u32>,
    /// The size of the largest group of linked sockets.
    pub links_min: Option<u32>,
    /// Keep fractured listings only, or only those that are not.
    pub fractured: Option<bool>,
    /// The least and the most of each defence.
    pub armour_min: Option<u32>,
    pub armour_max: Option<u32>,
    pub evasion_min: Option<u32>,
    pub evasion_max: Option<u32>,
    pub energy_shield_min: Option<u32>,
    pub energy_shield_max: Option<u32>,
    /// A shield's chance to block, in percent.
    pub block_min: Option<u32>,
    pub block_max: Option<u32>,
    /// The least percentile of the defence in the range of its base type.
    pub base_percentile_min: Option<u32>,
    /// The least experience stored in a Facetor's Lens.
    pub stored_experience_min: Option<u64>,
    /// A map's tier, exactly; and the least of what it adds, in percent.
    pub map_tier: Option<u32>,
    pub map_bonus_min: Option<u32>,
    pub map_quantity_min: Option<u32>,
    pub map_rarity_min: Option<u32>,
    pub map_pack_size_min: Option<u32>,
    /// The unique a Valdo map gives once completed.
    pub map_reward: Option<&'a str>,
    /// A blighted map: the discriminator (`blighted`, `uberblighted`) that makes `base` the id of
    /// its area.
    pub map_blight: Option<&'a str>,
    /// Physical damage per second.
    pub pdps_min: Option<f64>,
    pub pdps_max: Option<f64>,
    /// Elemental damage per second.
    pub edps_min: Option<f64>,
    pub edps_max: Option<f64>,
    /// Damage per second, of every kind together.
    pub dps_min: Option<f64>,
    pub dps_max: Option<f64>,
    /// Critical strike chance, in percent.
    pub crit_min: Option<f64>,
    pub crit_max: Option<f64>,
    /// Attacks per second.
    pub aps_min: Option<f64>,
    pub aps_max: Option<f64>,
}

/// The most quality to allow in a search for an amulet, ring or belt: 0 when the item has
/// none (catalysts show as quality on jewellery, so this keeps the catalysed ones out), and
/// nothing otherwise. `category` is the site's item category id, such as `accessory.ring`.
pub fn jewellery_quality_max(category: &str, quality: Option<u32>) -> Option<u32> {
    let jewellery = matches!(
        category,
        "accessory.amulet" | "accessory.ring" | "accessory.belt"
    );
    (jewellery && quality.unwrap_or(0) == 0).then_some(0)
}

/// The search request for a piece of gear, cheapest first.
pub fn gear_request(search: &GearSearch<'_>, opts: &SearchOptions) -> Value {
    let mut kind = Map::new();
    if let Some(category) = search.category {
        kind.insert("category".into(), json!({ "option": category }));
    }
    if search.rarity_magic {
        kind.insert("rarity".into(), json!({ "option": "magic" }));
    } else if search.rarity_nonunique {
        kind.insert("rarity".into(), json!({ "option": "nonunique" }));
    } else if search.rarity_unique {
        kind.insert("rarity".into(), json!({ "option": "unique" }));
    }

    let mut sockets = Map::new();
    add_range(&mut sockets, "links", search.links_min, None);

    let mut misc = Map::new();
    add_range(&mut misc, "ilvl", search.item_level_min, None);
    add_range(&mut misc, "quality", search.quality_min, search.quality_max);
    if let Some(fractured) = search.fractured {
        misc.insert("fractured_item".into(), yes_or_no(fractured));
    }
    if let Some(xp) = search.stored_experience_min {
        misc.insert("stored_experience".into(), json!({ "min": xp }));
    }
    misc.extend(misc_filters(opts));

    let mut armour = Map::new();
    armour.extend(armour_filters(search));

    let mut map = Map::new();
    add_range(&mut map, "map_tier", search.map_tier, search.map_tier);
    add_range(&mut map, "map_bonus", search.map_bonus_min, None);
    add_range(&mut map, "map_iiq", search.map_quantity_min, None);
    add_range(&mut map, "map_iir", search.map_rarity_min, None);
    add_range(&mut map, "map_packsize", search.map_pack_size_min, None);
    if let Some(reward) = search.map_reward {
        map.insert("map_completion_reward".into(), json!({ "option": reward }));
    }
    match search.map_blight {
        Some("blighted") => _ = map.insert("map_blighted".into(), json!({ "option": "true" })),
        Some("uberblighted") => _ = map.insert("map_uberblighted".into(), json!({ "option": "true" })),
        _ => {}
    }

    let weapon = weapon_filters(search);

    let mut filters = Map::new();
    add_group(&mut filters, "type_filters", kind);
    add_group(&mut filters, "socket_filters", sockets);
    add_group(&mut filters, "misc_filters", misc);
    add_group(&mut filters, "armour_filters", armour);
    add_group(&mut filters, "weapon_filters", weapon);
    add_group(&mut filters, "map_filters", map);
    add_group(&mut filters, "trade_filters", trade_filters(opts));

    let stats = stat_filters(search.stats);

    let mut query = Map::new();
    query.insert("status".into(), json!({ "option": opts.status }));
    match (search.base, search.map_blight) {
        (Some(area), Some(disc)) => _ = query.insert("type".into(), json!({ "option": area, "discriminator": disc })),
        (Some(base), None) => _ = query.insert("type".into(), json!(base)),
        (None, _) => {}
    }
    query.insert("stats".into(), json!([{ "type": "and", "filters": stats }]));
    query.insert("filters".into(), Value::Object(filters));
    json!({ "query": query, "sort": { "price": "asc" } })
}

/// Regroups the stats of a finished request: the stats of `not` go into a `not` group, which the
/// site reads as "none of these on the item", and with `count` the stats of the `and` group go
/// into a `count` group, which wants that many of them at least, but for those of `keep`, which
/// stay in the `and` group as requirements. Empty groups are left out, and
/// a request that has no `and` group is left as it is.
pub fn regroup_stats(request: &mut Value, not: &[String], count: Option<u32>, keep: &[&str]) {
    let Some(groups) = request.pointer_mut("/query/stats").and_then(Value::as_array_mut) else {
        return;
    };
    let mut counted = None;
    if let (Some(min), Some(first)) = (count, groups.first_mut()) {
        let all = first["filters"].take();
        let (kept, alternatives): (Vec<Value>, Vec<Value>) = all
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .partition(|filter| filter["id"].as_str().is_some_and(|id| keep.contains(&id)));
        first["filters"] = Value::Array(kept);
        if !alternatives.is_empty() {
            counted = Some(json!({ "type": "count", "value": { "min": min }, "filters": alternatives, "disabled": false }));
        }
    }
    if let Some(counted) = counted {
        // Alone, the count group takes the place of the "and" group; else it follows it.
        if groups[0]["filters"].as_array().is_some_and(Vec::is_empty) {
            groups[0] = counted;
        } else {
            groups.push(counted);
        }
    }
    if !not.is_empty() {
        let filters: Vec<Value> = not.iter().map(|id| json!({ "id": id, "disabled": false })).collect();
        groups.push(json!({ "type": "not", "filters": filters }));
    }
}

/// The stats of a search, each with the bounds it has, as the site's `and` group takes them.
fn stat_filters(stats: &[Stat]) -> Vec<Value> {
    stats
        .iter()
        .map(|(id, min, max)| {
            // An imbued gem's support: the site lists one stat per gem, the `|` part of the id,
            // and asks it without a value.
            if id.starts_with("imbued.") {
                return json!({ "id": id, "disabled": false });
            }
            // A stat of options, `implicit.stat_3624393862|4`: the option is the value.
            if let Some((id, option)) = id.split_once('|') {
                let option: Value = option.parse::<u64>().map_or_else(|_| json!(option), |n| json!(n));
                return json!({ "id": id, "value": { "option": option }, "disabled": false });
            }
            let mut value = Map::new();
            if let Some(min) = min {
                value.insert("min".into(), number(*min));
            }
            if let Some(max) = max {
                value.insert("max".into(), number(*max));
            }
            json!({ "id": id, "value": value, "disabled": false })
        })
        .collect()
}

/// The filters on the listing itself: its currency and its age.
fn trade_filters(opts: &SearchOptions) -> Map<String, Value> {
    let mut trade = Map::new();
    if let Some(currency) = opts.currency {
        trade.insert("price".into(), json!({ "option": currency }));
    }
    if let Some(listed) = opts.listed {
        trade.insert("indexed".into(), json!({ "option": listed }));
    }
    trade
}

/// The filters of the site's miscellaneous group that the options decide.
fn misc_filters(opts: &SearchOptions) -> Map<String, Value> {
    let mut misc = Map::new();
    if let Some(corrupted) = opts.corrupted {
        misc.insert("corrupted".into(), yes_or_no(corrupted));
    }
    for (bit, id) in INFLUENCE_FILTERS.iter().enumerate() {
        if opts.influences & (1 << bit) != 0 {
            misc.insert((*id).into(), yes_or_no(true));
        }
    }
    for (id, value) in [
        ("mirrored", opts.mirrored),
        ("split", opts.split),
        ("identified", opts.identified),
        ("mutated", opts.mutated),
        ("vestigial", opts.vestigial),
        ("gem_imbued", opts.imbued),
    ] {
        if let Some(value) = value {
            misc.insert(id.into(), yes_or_no(value));
        }
    }
    misc
}

/// A yes-or-no option of the site: the word, not a JSON boolean.
fn yes_or_no(yes: bool) -> Value {
    json!({ "option": if yes { "true" } else { "false" } })
}

/// Adds a group of filters under its name, unless it is empty.
fn add_group(filters: &mut Map<String, Value>, name: &str, group: Map<String, Value>) {
    if !group.is_empty() {
        filters.insert(name.into(), json!({ "filters": group }));
    }
}

/// Adds `name: { "min": min, "max": max }` with the bounds there are, unless there is none.
fn add_range<T: Into<f64>>(
    group: &mut Map<String, Value>,
    name: &str,
    min: Option<T>,
    max: Option<T>,
) {
    let mut range = Map::new();
    if let Some(min) = min {
        range.insert("min".into(), number(min.into()));
    }
    if let Some(max) = max {
        range.insert("max".into(), number(max.into()));
    }
    if !range.is_empty() {
        group.insert(name.into(), Value::Object(range));
    }
}

/// A whole number without a fraction part, as a person would type it.
fn number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        json!(value as i64)
    } else {
        json!(value)
    }
}
