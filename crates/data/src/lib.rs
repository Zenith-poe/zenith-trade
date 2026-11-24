//! The data pack: what the application knows about the game, built from the
//! trade site's public lists and kept in one local file.

pub mod build;
pub mod classes;
pub mod fetch;
pub mod pack;

pub use build::{BuildError, build};
pub use classes::{CLASSES, category, is_local_class};
pub use fetch::{FetchError, fetch};
pub use pack::{Changes, Pack, PackError, changes};

/// The variant mod lines of the uniques that draw them from a pool (Zana's Ingenuity), from
/// Path of Building's unique data: see `tools/gen-unique-variants.py` and THIRD_PARTY_NOTICES.
const UNIQUE_VARIANTS: &str = include_str!("../unique-variants.json");

/// The variant line templates of a unique, `#` for each number; a line that rolls in a range
/// starts with `~`. Empty for a unique without a pool.
pub fn unique_variants(name: &str) -> Vec<String> {
    static ALL: std::sync::OnceLock<std::collections::HashMap<String, Vec<String>>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| serde_json::from_str(UNIQUE_VARIANTS).unwrap_or_default()).get(name).cloned().unwrap_or_default()
}

/// The uniques no longer obtainable, found in Standard only, from Path of Building's unique data:
/// see `tools/gen-legacy-uniques.py` and THIRD_PARTY_NOTICES.
const LEGACY_UNIQUES: &str = include_str!("../legacy-uniques.json");

/// Whether a unique is found in Standard only.
pub fn is_legacy_unique(name: &str) -> bool {
    static ALL: std::sync::OnceLock<std::collections::HashSet<String>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| serde_json::from_str(LEGACY_UNIQUES).unwrap_or_default()).contains(name)
}

/// The tags of the mods, from the RePoE mod data: see `tools/gen-mod-tags.py` and THIRD_PARTY_NOTICES.
const MOD_TAGS: &str = include_str!("../mod-tags.json");

/// The tags the game's advanced tooltip writes after a mod's name ("defences", "energy_shield"),
/// by the name the trade site gives, the side, and the mod's level when it has one.
pub fn mod_tags(name: &str, prefix: bool, level: Option<u32>) -> Option<&'static [String]> {
    #[derive(serde::Deserialize)]
    struct File {
        tags: Vec<String>,
        mods: std::collections::HashMap<String, Vec<usize>>,
    }
    static ALL: std::sync::OnceLock<std::collections::HashMap<String, Vec<String>>> = std::sync::OnceLock::new();
    let all = ALL.get_or_init(|| {
        let file: File = serde_json::from_str(MOD_TAGS).unwrap_or(File { tags: Vec::new(), mods: Default::default() });
        file.mods.into_iter().map(|(key, ids)| (key, ids.into_iter().filter_map(|i| file.tags.get(i).cloned()).collect())).collect()
    });
    let side = if prefix { 'p' } else { 's' };
    let found = level.and_then(|level| all.get(&format!("{name}|{side}|{level}"))).or_else(|| all.get(&format!("{name}|{side}")));
    found.map(Vec::as_slice)
}

/// The defence ranges of the bases that have one that rolls, from the RePoE base item data: see
/// `tools/gen-base-defences.py` and THIRD_PARTY_NOTICES.
const BASE_DEFENCES: &str = include_str!("../base-defences.json");

/// The range a base type's defence rolls in, `(least, most)`.
pub type DefenceRange = Option<(u32, u32)>;

/// The ranges of the armour, the evasion, the energy shield and the ward of a base type, in that
/// order; `None` for a defence the base has no range for.
pub fn base_defences(base: &str) -> [DefenceRange; 4] {
    type Ranges = std::collections::HashMap<String, std::collections::HashMap<String, (u32, u32)>>;
    static ALL: std::sync::OnceLock<Ranges> = std::sync::OnceLock::new();
    let all = ALL.get_or_init(|| serde_json::from_str(BASE_DEFENCES).unwrap_or_default());
    let found = all.get(base);
    ["armour", "evasion", "energy_shield", "ward"].map(|key| found.and_then(|ranges| ranges.get(key)).copied())
}
