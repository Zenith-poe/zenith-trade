//! The map mods check: the view of a map with a tag on each of its mods, and the file that keeps
//! the tags.
//!
//! A mod is tagged `Danger` or `Good` by the player, with a right click in the panel. The tag
//! belongs to the mod's wording with its numbers taken out (`Monsters reflect #% of Physical
//! Damage`), so it holds for every map that has the mod, whatever it rolled. The tags live in
//! `%LOCALAPPDATA%\zenith\map-mods.json`; a file that cannot be read is ignored and said in the
//! log.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{Value, json};

use crate::state::View;

/// What the player thinks of a mod.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    Good,
    Warning,
    Danger,
}

impl Tag {
    fn word(self) -> &'static str {
        match self {
            Tag::Danger => "danger",
            Tag::Warning => "warning",
            Tag::Good => "good",
        }
    }

    fn of_word(word: &str) -> Option<Self> {
        match word {
            "danger" => Some(Tag::Danger),
            "warning" => Some(Tag::Warning),
            "good" => Some(Tag::Good),
            _ => None,
        }
    }
}

/// The wording of a mod with its numbers, its roll ranges and its sign taken out, in lower case:
/// `Monsters deal 90(90-110)% extra Damage` is `monsters deal #% extra damage`.
pub fn normalise(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c.is_ascii_digit() {
            while at < chars.len() && (chars[at].is_ascii_digit() || (chars[at] == '.' && chars.get(at + 1).is_some_and(char::is_ascii_digit))) {
                at += 1;
            }
            out.push('#');
        } else {
            out.push(c);
            at += 1;
        }
    }
    let out = out.replace("(#-#)", "").replace("(#\u{2013}#)", "").replace("+#", "#");
    out.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// The tags of the player, by normalised wording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapMods {
    tags: BTreeMap<String, Tag>,
    /// The version the file had when it was read; a store made in memory is of the current one.
    version: u32,
}

/// The version of the file this build writes: 2 brought the warnings.
const VERSION: u32 = 2;

impl Default for MapMods {
    fn default() -> Self {
        Self { tags: BTreeMap::new(), version: VERSION }
    }
}

impl MapMods {
    pub fn tag(&self, text: &str) -> Option<Tag> {
        self.tags.get(&normalise(text)).copied()
    }

    pub fn set(&mut self, text: &str, tag: Option<Tag>) {
        let key = normalise(text);
        match tag {
            Some(tag) => drop(self.tags.insert(key, tag)),
            None => drop(self.tags.remove(&key)),
        }
    }

    /// Every tagged wording with its tag, sorted by text (for the settings tab).
    #[allow(dead_code)]
    pub fn entries(&self) -> Vec<(String, Tag)> {
        self.tags.iter().map(|(key, tag)| (key.clone(), *tag)).collect()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.tags.len()
    }

    pub fn to_json(&self) -> String {
        let tags: serde_json::Map<String, Value> = self.tags.iter().map(|(key, tag)| (key.clone(), json!(tag.word()))).collect();
        serde_json::to_string_pretty(&json!({ "version": VERSION, "tags": tags })).unwrap_or_default()
    }

    /// Reads a file's text. A tag that is not `danger`, `warning` or `good` is left out.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let tags = value.get("tags").and_then(Value::as_object).ok_or("no \"tags\" object")?;
        let version = value.get("version").and_then(Value::as_u64).map_or(1, |v| v as u32);
        Ok(Self { tags: tags.iter().filter_map(|(key, tag)| Some((key.clone(), Tag::of_word(tag.as_str()?)?))).collect(), version })
    }

    /// Reads the file: nothing at all when there is none, and an empty store, with the reason,
    /// when it cannot be read.
    pub fn load_from(path: &Path) -> (Option<Self>, Option<String>) {
        match std::fs::read_to_string(path) {
            Ok(text) => match Self::from_json(&text) {
                Ok(store) => (Some(store), None),
                Err(why) => (Some(Self::default()), Some(format!("{} cannot be read, it is ignored: {why}", path.display()))),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (None, None),
            Err(error) => (Some(Self::default()), Some(format!("{} cannot be read, it is ignored: {error}", path.display()))),
        }
    }

    /// Writes the file whole through a temporary one beside it, so that it is never half written.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, self.to_json())?;
        std::fs::rename(&temp, path)
    }

    /// The store of the player: read from the file, and written back when it was of version 1.
    pub fn load() -> Self {
        let Some(path) = file() else {
            return Self::default();
        };
        let (loaded, warning) = Self::load_from(&path);
        if let Some(warning) = warning {
            log!("map mods: {warning}");
        }
        let mut store = loaded.unwrap_or_default();
        let changed = store.version == 1;
        store.version = VERSION;
        if changed && let Err(error) = store.save(&path) {
            log!("map mods: {} cannot be written: {error}", path.display());
        }
        store
    }

}

/// `%LOCALAPPDATA%\zenith\map-mods.json`.
fn file() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("zenith").join("map-mods.json"))
}

static STORE: Mutex<Option<MapMods>> = Mutex::new(None);

/// The store of the player, loaded on first use.
fn with_store<T>(use_it: impl FnOnce(&mut MapMods) -> T) -> T {
    let mut slot = STORE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    use_it(slot.get_or_insert_with(MapMods::load))
}

/// One mod line of the map and what the player made of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModRow {
    pub text: String,
    pub tag: Option<Tag>,
}

/// A map as the panel shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapView {
    pub name: String,
    pub tier: Option<u32>,
    pub properties: Vec<(String, String)>,
    pub mods: Vec<ModRow>,
}

impl MapView {
    pub fn new(info: zenith_core::MapInfo, store: &MapMods) -> Self {
        let mods = info.mods.into_iter().map(|text| ModRow { tag: store.tag(&text), text }).collect();
        Self { name: info.name, tier: info.tier, properties: info.properties, mods }
    }

    /// How many mods the player called dangerous.
    pub fn danger_count(&self) -> usize {
        self.count(Tag::Danger)
    }

    /// How many mods the player put a warning on.
    pub fn warning_count(&self) -> usize {
        self.count(Tag::Warning)
    }

    fn count(&self, tag: Tag) -> usize {
        self.mods.iter().filter(|row| row.tag == Some(tag)).count()
    }

    /// "2 dangerous, 1 warning", or nothing when there are none.
    pub fn danger_line(&self) -> Option<String> {
        let (danger, warning) = (self.danger_count(), self.warning_count());
        let mut parts = Vec::new();
        if danger > 0 {
            parts.push(format!("{danger} dangerous"));
        }
        if warning > 0 {
            parts.push(format!("{warning} warning{}", if warning == 1 { "" } else { "s" }));
        }
        (!parts.is_empty()).then(|| parts.join(", "))
    }

    /// Tags the mod at `index`, and every row with the same wording, and keeps it.
    pub fn tag(&mut self, index: usize, tag: Option<Tag>) {
        let Some(text) = self.mods.get(index).map(|row| row.text.clone()) else {
            return;
        };
        let key = normalise(&text);
        for row in self.mods.iter_mut().filter(|row| normalise(&row.text) == key) {
            row.tag = tag;
        }
        let saved = with_store(|store| {
            store.set(&text, tag);
            file().map(|path| store.save(&path))
        });
        if let Some(Err(error)) = saved {
            log!("map mods: the tags cannot be written: {error}");
        }
    }
}

/// The view of the copied text, or `None` when it is no map.
pub fn read_map(text: &str) -> Option<View> {
    let item = zenith_core::parse_item(text).ok()?;
    let info = zenith_core::parse_map(&item)?;
    Some(View::Maps(with_store(|store| MapView::new(info, store))))
}

/// The map of `--view map`: made up, with tags of its own.
pub fn sample() -> View {
    let text = "Item Class: Maps\nRarity: Rare\nDoom Lair\nStrand Map\n--------\nMap Tier: 16\nItem Quantity: +119% (augmented)\nItem Rarity: +69% (augmented)\nMonster Pack Size: +36% (augmented)\nMore Maps: +64% (augmented)\nMore Scarabs: +31% (augmented)\n--------\nItem Level: 83\n--------\n{ Prefix Modifier }\nMonsters reflect 18% of Physical Damage\n{ Prefix Modifier }\nMonsters have 35% increased Area of Effect\n{ Prefix Modifier }\nArea contains two Unique Bosses\n{ Suffix Modifier }\nPlayers have 20% less Recovery Rate of Life and Energy Shield\n{ Suffix Modifier }\n+30% Monster Elemental Resistances\n";
    let info = zenith_core::parse_item(text).ok().and_then(|item| zenith_core::parse_map(&item)).expect("the sample is a map");
    let mut store = MapMods::default();
    store.set("Monsters reflect #% of Physical Damage", Some(Tag::Danger));
    store.set("Players have #% less Recovery Rate of Life and Energy Shield", Some(Tag::Danger));
    store.set("Monsters have #% increased Area of Effect", Some(Tag::Warning));
    store.set("Area contains two Unique Bosses", Some(Tag::Good));
    View::Maps(MapView::new(info, &store))
}
