//! The pack itself: its file, and what changed between two packs.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Everything the application knows about the game.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pack {
    /// When the pack was built (ISO 8601, UTC).
    pub built: String,
    /// `stats[kind][text]` lists trade ids. `kind` is the trade group id,
    /// such as `explicit`; `text` is exactly as the site gives it.
    pub stats: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    /// Base type names per trade item group, sorted.
    pub bases: BTreeMap<String, Vec<String>>,
    /// Unique items as (name, base), sorted.
    pub uniques: Vec<(String, String)>,
    /// Transfigured gems: the name the item shows to the base gem and the discriminator the
    /// site tells it by (`Arc of Oscillating` to `Arc`, `alt_y`).
    #[serde(default)]
    pub gems: BTreeMap<String, (String, String)>,
    /// Item class as written in copied items (`Body Armours`) to trade
    /// category id (`armour.chest`).
    pub categories: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackError {
    /// There is no file at the path.
    Missing,
    /// The file could not be read or written.
    Io(String),
    /// The file is not a pack.
    Format(String),
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => write!(f, "no data pack on disk"),
            Self::Io(reason) => write!(f, "data pack file: {reason}"),
            Self::Format(reason) => write!(f, "not a data pack: {reason}"),
        }
    }
}

impl std::error::Error for PackError {}

impl Pack {
    /// The unique items that have this base type, by name, in the pack's order. Empty for a base
    /// without a unique, and for a pack built before it knew the uniques.
    pub fn uniques_of(&self, base: &str) -> Vec<String> {
        self.uniques.iter().filter(|(_, of)| of == base).map(|(name, _)| name.clone()).collect()
    }

    /// Reads the pack at `path`.
    pub fn load(path: &Path) -> Result<Pack, PackError> {
        let text = fs::read_to_string(path).map_err(|error| match error.kind() {
            ErrorKind::NotFound => PackError::Missing,
            _ => PackError::Io(error.to_string()),
        })?;
        serde_json::from_str(&text).map_err(|error| PackError::Format(error.to_string()))
    }

    /// Writes the pack at `path`, creating the folder when needed. The pack
    /// goes to a temporary file beside it first (`pack.json.tmp`), then
    /// takes the place of the old one, so a failure leaves the old one whole.
    pub fn save(&self, path: &Path) -> Result<(), PackError> {
        let text =
            serde_json::to_string(self).map_err(|error| PackError::Format(error.to_string()))?;
        if let Some(folder) = path
            .parent()
            .filter(|folder| !folder.as_os_str().is_empty())
        {
            fs::create_dir_all(folder).map_err(io)?;
        }
        let mut name = path.as_os_str().to_owned();
        name.push(".tmp");
        let temporary = PathBuf::from(name);
        write_whole(&temporary, &text)
            .and_then(|()| fs::rename(&temporary, path))
            .map_err(|error| {
                let _ = fs::remove_file(&temporary);
                io(error)
            })
    }

    /// `%LOCALAPPDATA%\zenith\pack.json`. Without that variable, the same
    /// under the temporary folder.
    pub fn default_path() -> PathBuf {
        std::env::var_os("LOCALAPPDATA")
            .filter(|root| !root.is_empty())
            .map_or_else(std::env::temp_dir, PathBuf::from)
            .join("zenith")
            .join("pack.json")
    }
}

fn io(error: std::io::Error) -> PackError {
    PackError::Io(error.to_string())
}

/// Writes a file and waits until it is on the disk.
fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

/// What a new pack adds and removes, as names a person can read, sorted.
///
/// A stat is its text, whatever the kinds that carry it: a text is new when
/// no kind had it before, gone when no kind has it any more. A base is its
/// name, whatever the group. A unique is its name followed by its base, as
/// the site displays it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    pub new_stats: Vec<String>,
    pub gone_stats: Vec<String>,
    pub new_bases: Vec<String>,
    pub gone_bases: Vec<String>,
    pub new_uniques: Vec<String>,
    pub gone_uniques: Vec<String>,
}

pub fn changes(old: &Pack, new: &Pack) -> Changes {
    let (new_stats, gone_stats) = compare(&stat_texts(old), &stat_texts(new));
    let (new_bases, gone_bases) = compare(&base_names(old), &base_names(new));
    let (new_uniques, gone_uniques) = compare(&unique_names(old), &unique_names(new));
    Changes {
        new_stats,
        gone_stats,
        new_bases,
        gone_bases,
        new_uniques,
        gone_uniques,
    }
}

/// What `new` has that `old` has not, then the reverse.
fn compare(old: &BTreeSet<String>, new: &BTreeSet<String>) -> (Vec<String>, Vec<String>) {
    (
        new.difference(old).cloned().collect(),
        old.difference(new).cloned().collect(),
    )
}

fn stat_texts(pack: &Pack) -> BTreeSet<String> {
    pack.stats
        .values()
        .flat_map(BTreeMap::keys)
        .cloned()
        .collect()
}

fn base_names(pack: &Pack) -> BTreeSet<String> {
    pack.bases.values().flatten().cloned().collect()
}

fn unique_names(pack: &Pack) -> BTreeSet<String> {
    pack.uniques
        .iter()
        .map(|(name, base)| format!("{name} {base}"))
        .collect()
}
