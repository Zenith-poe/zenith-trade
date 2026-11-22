//! The choices that stick: the settings of a search the player pinned, kept in a small file.
//!
//! A pinned setting holds a value: every new check starts from it. A setting that is not pinned
//! is not in the file, and a new check takes the built-in value for it. The file also says
//! whether Ctrl and the wheel change stash tab, `"stash_scroll": false` for no: it is yes when
//! the file says nothing of it, and the panel has no switch for it. The file is
//! `%APPDATA%\Zenith Trade\config.json`, or the one the environment variable `ZENITH_CONFIG`
//! names. It is read once, at start, and written by one thread only, the `Saver`'s, so that a
//! frame never waits for the disk and no two writers ever meet. What else the file holds, the
//! `hotkeys` object for one (see `hotkeys`), is the player's own writing: saving a pin leaves it
//! as it is.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;

use serde_json::{Map, Value};
use zenith_trade::{Currency, Listed, Status};

use crate::check::{Options, STATUSES, Setting};
use crate::look::{Marker, Theme};
use zenith_core::matching::Slack;

/// Names the file of the settings, in place of the one in the data folder.
pub const VARIABLE: &str = "ZENITH_CONFIG";

/// The keys of the file that are written from here. Every other key is left as it is.
const OWN_KEYS: [&str; 8] = ["status", "currency", "listed", "stash_scroll", "theme", "marker", "league", "default_min"];

/// How "any" is written for a setting that is pinned to no value at all.
const ANY: &str = "any";

/// The name of a league, short enough to be copied around: 40 bytes at most.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct League {
    bytes: [u8; 40],
    len: u8,
}

impl League {
    /// `None` for a name that is empty once trimmed; a longer one is cut at a character.
    pub fn new(name: &str) -> Option<Self> {
        let name = name.trim();
        let mut end = name.len().min(40);
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        let name = &name[..end];
        if name.is_empty() {
            return None;
        }
        let mut bytes = [0; 40];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        Some(Self { bytes, len: name.len() as u8 })
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}

/// What is pinned. `None`: the setting is not pinned. For the currency and the age, a pinned
/// `None` is "any".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub status: Option<Status>,
    pub currency: Option<Option<Currency>>,
    pub listed: Option<Option<Listed>>,
    /// Ctrl and the wheel change stash tab anywhere in the game: see `stash`.
    pub stash_scroll: bool,
    /// The colours of the panel; Forge when the file says nothing.
    pub theme: Theme,
    /// How a searched mod is marked; the edge bar when the file says nothing.
    pub marker: Marker,
    /// The league to price in, in place of the current trade league.
    pub league: Option<League>,
    /// How far under the item's value a filter's minimum starts; its exact value when the file
    /// says nothing.
    pub slack: Slack,
}

impl Default for Config {
    fn default() -> Self {
        Self::NONE
    }
}

impl Config {
    /// Nothing pinned, and the stash scroll on: what no file gives.
    pub const NONE: Self = Self { status: None, currency: None, listed: None, stash_scroll: true, theme: Theme::Forge, marker: Marker::Bar, league: None, slack: Slack::Exact };

    pub fn pinned(&self, setting: Setting) -> bool {
        match setting {
            Setting::Status => self.status.is_some(),
            Setting::Currency => self.currency.is_some(),
            Setting::Listed => self.listed.is_some(),
        }
    }

    /// Pins a setting at the value it has now.
    pub fn pin(&mut self, setting: Setting, options: &Options) {
        match setting {
            Setting::Status => self.status = Some(options.status),
            Setting::Currency => self.currency = Some(options.currency),
            Setting::Listed => self.listed = Some(options.listed),
        }
    }

    pub fn unpin(&mut self, setting: Setting) {
        match setting {
            Setting::Status => self.status = None,
            Setting::Currency => self.currency = None,
            Setting::Listed => self.listed = None,
        }
    }

    /// A click on a pin: pinned at the value the setting has now, or no longer pinned.
    pub fn toggle(&mut self, setting: Setting, options: &Options) {
        if self.pinned(setting) {
            self.unpin(setting);
        } else {
            self.pin(setting, options);
        }
    }

    /// Every pinned setting takes the value it has now. Says whether that changed anything.
    pub fn follow(&mut self, options: &Options) -> bool {
        let before = *self;
        for setting in crate::check::SETTINGS {
            if self.pinned(setting) {
                self.pin(setting, options);
            }
        }
        *self != before
    }

    /// What a new check starts from: the pinned values over the ones it has.
    pub fn apply(&self, options: &mut Options) {
        if let Some(status) = self.status {
            options.status = status;
        }
        if let Some(currency) = self.currency {
            options.currency = currency;
        }
        if let Some(listed) = self.listed {
            options.listed = listed;
        }
    }

    /// The file's text: one key per pinned setting, with the site's own word for its value, and
    /// the stash scroll when it is off.
    pub fn to_json(self) -> String {
        let word = |value: Value| if value.is_null() { Value::from(ANY) } else { value };
        let mut pins = Map::new();
        if let Some(status) = self.status {
            pins.insert("status".into(), serde_json::json!(status));
        }
        if let Some(currency) = self.currency {
            pins.insert("currency".into(), word(serde_json::json!(currency)));
        }
        if let Some(listed) = self.listed {
            pins.insert("listed".into(), word(serde_json::json!(listed)));
        }
        if !self.stash_scroll {
            pins.insert("stash_scroll".into(), Value::Bool(false));
        }
        if self.theme != Theme::Forge {
            pins.insert("theme".into(), Value::from(self.theme.key()));
        }
        if self.marker != Marker::Bar {
            pins.insert("marker".into(), Value::from(self.marker.key()));
        }
        if let Some(league) = self.league {
            pins.insert("league".into(), Value::from(league.as_str()));
        }
        if self.slack != Slack::Exact {
            pins.insert("default_min".into(), Value::from(self.slack.percent()));
        }
        let mut text = serde_json::to_string_pretty(&Value::Object(pins)).unwrap_or_else(|_| "{}".into());
        text.push('\n');
        text
    }

    /// The file's text once these settings are saved in a file that holds `existing`: the keys
    /// of the pinned settings and of the stash scroll as `to_json` writes them, and every other
    /// key of the file, the hotkeys among them, as it was. A file that is no JSON object keeps
    /// nothing.
    pub fn merged(self, existing: Option<&str>) -> String {
        let mut file = match existing.map(serde_json::from_str::<Value>) {
            Some(Ok(Value::Object(file))) => file,
            _ => Map::new(),
        };
        for key in OWN_KEYS {
            file.remove(key);
        }
        if let Ok(Value::Object(own)) = serde_json::from_str::<Value>(&self.to_json()) {
            file.extend(own);
        }
        let mut text = serde_json::to_string_pretty(&Value::Object(file)).unwrap_or_else(|_| self.to_json());
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text
    }

    /// `merged`, with the `hotkeys` object of the file replaced by `hotkeys`.
    #[cfg(test)]
    pub fn merged_with_hotkeys(self, existing: Option<&str>, hotkeys: Value) -> String {
        merge_hotkeys(&self.merged(existing), hotkeys)
    }

    /// Reads the file's text. A text that is no JSON object is an error. A value that is not
    /// understood leaves its setting unpinned and is named in the second part of the answer; a
    /// key that is not known is passed over.
    pub fn parse(text: &str) -> Result<(Self, Vec<String>), String> {
        let value: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
        let Value::Object(pins) = value else {
            return Err("not a JSON object".into());
        };
        let mut config = Self::default();
        let mut odd = Vec::new();
        let any = |value: &Value| value.as_str() == Some(ANY);
        for (key, value) in &pins {
            let understood = match key.as_str() {
                // Only what the chip offers: any other status could not be changed back to.
                "status" => {
                    config.status = serde_json::from_value(value.clone()).ok().filter(|status| STATUSES.contains(status));
                    config.status.is_some()
                }
                // "any" for no value at all, anything else as the site's word.
                "currency" => {
                    config.currency = if any(value) { Some(None) } else { serde_json::from_value(value.clone()).ok().map(Some) };
                    config.currency.is_some()
                }
                "listed" => {
                    config.listed = if any(value) { Some(None) } else { serde_json::from_value(value.clone()).ok().map(Some) };
                    config.listed.is_some()
                }
                // Yes or no; anything else leaves it on.
                "stash_scroll" => {
                    config.stash_scroll = value.as_bool().unwrap_or(true);
                    value.is_boolean()
                }
                "theme" => {
                    let theme = value.as_str().and_then(Theme::from_key);
                    config.theme = theme.unwrap_or(Theme::Forge);
                    theme.is_some()
                }
                "marker" => {
                    let marker = value.as_str().and_then(Marker::from_key);
                    config.marker = marker.unwrap_or(Marker::Bar);
                    marker.is_some()
                }
                // Percent under the value: 5, 10 or 20; anything else is exact.
                "default_min" => {
                    let slack = value.as_u64().and_then(|n| Slack::ALL.into_iter().find(|s| u64::from(s.percent()) == n));
                    config.slack = slack.unwrap_or(Slack::Exact);
                    slack.is_some()
                }
                "league" => {
                    config.league = value.as_str().and_then(League::new);
                    config.league.is_some()
                }
                _ => true,
            };
            if !understood {
                odd.push(format!("{key}: {value}"));
            }
        }
        Ok((config, odd))
    }

    /// The settings kept in `path`. No file is no pin. A file that cannot be read or understood
    /// is ignored and said in the log: nothing here ever stops the program.
    pub fn load(path: &Path) -> Self {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(error) => {
                log!("settings: {} could not be read: {error}", path.display());
                return Self::default();
            }
        };
        match Self::parse(&text) {
            Ok((config, odd)) => {
                for entry in odd {
                    log!("settings: {} has a value that is not understood, ignored: {entry}", path.display());
                }
                crate::check::set_slack(config.slack);
                config
            }
            Err(reason) => {
                log!("settings: {} could not be used, ignored: {reason}", path.display());
                Self::default()
            }
        }
    }

    /// Writes the settings to `path`: to a temporary file beside it first, which then takes its
    /// place, so that the file is never seen half written.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(folder) = path.parent().filter(|folder| !folder.as_os_str().is_empty()) {
            std::fs::create_dir_all(folder)?;
        }
        let mut name = path.as_os_str().to_owned();
        name.push(".tmp");
        let temporary = PathBuf::from(name);
        // What the file holds besides these settings stays in it.
        let existing = std::fs::read_to_string(path).ok();
        std::fs::write(&temporary, self.merged(existing.as_deref())).and_then(|()| std::fs::rename(&temporary, path)).inspect_err(|_| {
            let _ = std::fs::remove_file(&temporary);
        })
    }
}

/// The folder of the app's data: `%APPDATA%\Zenith Trade`.
pub fn data_dir() -> PathBuf {
    zenith_data::pack::data_dir()
}

/// Copies the files of an older install (`%LOCALAPPDATA%\zenith`, which the installer's own
/// folder shared) to the data folder, once: only when the new folder has no `config.json` and the
/// old one has a `config.json` or `map-mods.json`. Nothing is moved or deleted; icons are left to
/// download again. Returns the number of files copied; failures are ignored.
pub fn migrate_old_data() -> usize {
    let Some(local) = std::env::var_os("LOCALAPPDATA").filter(|root| !root.is_empty()) else { return 0 };
    migrate_from(&PathBuf::from(local).join("zenith"), &data_dir())
}

fn migrate_from(old: &std::path::Path, new: &std::path::Path) -> usize {
    if new.join("config.json").exists() || !(old.join("config.json").is_file() || old.join("map-mods.json").is_file()) {
        return 0;
    }
    if std::fs::create_dir_all(new).is_err() {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(old) else { return 0 };
    let mut copied = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let text = name.to_string_lossy();
        let wanted = matches!(text.as_ref(), "config.json" | "map-mods.json" | "pack.json") || (text.starts_with("prices-") && text.ends_with(".json"));
        if wanted && entry.path().is_file() && std::fs::copy(entry.path(), new.join(&name)).is_ok() {
            copied += 1;
        }
    }
    copied
}

/// Where the settings are kept, given what `ZENITH_CONFIG` names: that file, or
/// `config.json` in the data folder.
fn path_from(named: Option<OsString>) -> PathBuf {
    if let Some(named) = named.filter(|path| !path.is_empty()) {
        return PathBuf::from(named);
    }
    data_dir().join("config.json")
}

/// The file `ZENITH_CONFIG` names, when it names one.
pub fn named_path() -> Option<PathBuf> {
    std::env::var_os(VARIABLE).filter(|path| !path.is_empty()).map(PathBuf::from)
}

/// Where the settings are kept: the file `ZENITH_CONFIG` names, or the one in the data folder.
pub fn path() -> PathBuf {
    path_from(std::env::var_os(VARIABLE))
}

/// The file's text with its `hotkeys` object replaced, every other key as it was. A text that is
/// no JSON object keeps nothing.
pub fn merge_hotkeys(existing: &str, hotkeys: Value) -> String {
    let mut file = match serde_json::from_str::<Value>(existing) {
        Ok(Value::Object(file)) => file,
        _ => Map::new(),
    };
    file.insert("hotkeys".into(), hotkeys);
    let mut text = serde_json::to_string_pretty(&Value::Object(file)).unwrap_or_else(|_| "{}".into());
    text.push('\n');
    text
}

fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(folder) = path.parent().filter(|folder| !folder.as_os_str().is_empty()) {
        std::fs::create_dir_all(folder)?;
    }
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    let temporary = PathBuf::from(name);
    std::fs::write(&temporary, text).and_then(|()| std::fs::rename(&temporary, path)).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}

enum Message {
    Save(Config),
    Hotkeys(Value),
    /// Answer once everything sent before is written.
    Flush(Sender<()>),
}

/// The one writer of the settings file: a thread that waits for settings to save. Whoever
/// changes a pin hands it the settings and goes on; the disk is never waited for on a frame.
pub struct Saver {
    to: Sender<Message>,
}

impl Saver {
    /// Starts the thread that writes `path`.
    pub fn spawn(path: PathBuf) -> std::io::Result<Self> {
        let (to, messages) = channel::<Message>();
        std::thread::Builder::new().name("settings".into()).spawn(move || {
            for message in messages {
                match message {
                    Message::Save(config) => match config.save(&path) {
                        Ok(()) => log!("settings: saved in {}", path.display()),
                        Err(error) => log!("settings: could not be saved in {}: {error}", path.display()),
                    },
                    Message::Hotkeys(hotkeys) => {
                        let existing = std::fs::read_to_string(&path).unwrap_or_default();
                        match write_atomic(&path, &merge_hotkeys(&existing, hotkeys)) {
                            Ok(()) => log!("settings: hotkeys saved in {}", path.display()),
                            Err(error) => log!("settings: hotkeys could not be saved in {}: {error}", path.display()),
                        }
                    }
                    Message::Flush(done) => drop(done.send(())),
                }
            }
        })?;
        Ok(Self { to })
    }

    /// Hands the settings over to be written. Returns at once.
    pub fn save(&self, config: Config) {
        if self.to.send(Message::Save(config)).is_err() {
            log!("settings: not saved, the thread that writes them is gone");
        }
    }

    /// Hands the `hotkeys` object over to be written. Returns at once.
    pub fn save_hotkeys(&self, hotkeys: Value) {
        if self.to.send(Message::Hotkeys(hotkeys)).is_err() {
            log!("settings: hotkeys not saved, the thread that writes them is gone");
        }
    }

    /// Waits until everything handed over is written, for `limit` at most. Says whether it is.
    pub fn flush(&self, limit: Duration) -> bool {
        let (done, written) = channel();
        self.to.send(Message::Flush(done)).is_ok() && written.recv_timeout(limit).is_ok()
    }
}
