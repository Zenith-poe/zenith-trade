//! Client for the trade site: query building, rate limiting, search and fetch.

pub mod client;
pub mod exchange;
pub mod limits;
pub mod query;

pub use client::{Affix, Client, Found, ListedItem, ListedMod, ListedModKind, ListedProperty, ListedRarity, Listing, TradeError};
pub use exchange::{Bulk, Offer, Row};
pub use limits::{Limiter, Rule, parse_rules};
pub use query::{
    Currency, GearSearch, INFLUENCE_FILTERS, Listed, SearchOptions, Stat, Status, add_armour_filters, add_quality, add_weapon_filters, gear_request, gem_request, jewellery_quality_max,
    regroup_stats, unique_request,
};
