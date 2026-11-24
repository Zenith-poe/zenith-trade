//! Item text to structured item, its mods matched to trade stats, and the estimate. No
//! network, no interface.

pub mod estimate;
pub mod item;
pub mod maps;
pub mod matching;
pub mod mods;
pub mod origin;
pub mod props;
pub mod pseudo;

pub use estimate::{Estimate, estimate, suspicious};
pub use item::{Influence, Item, ParseError, Rarity, gem_level, imbued_support, parse_item};
pub use maps::{MapInfo, is_map, parse_map};
pub use matching::{Filter, Local, Matched, Stats, default_filters, imbued_filter, match_mod, unique_filters};
pub use origin::Origin;
pub use mods::{Mod, ModKind, ModLine, open_affixes, parse_mods};
pub use props::{Properties, parse_properties, short_number};
pub use pseudo::{Pseudo, is_pseudo, pseudo_totals};
