//! The keys the program answers to in the game, and where the player chooses them.
//!
//! The settings file may hold a `hotkeys` object, every part of which can be left out:
//!
//! ```json
//! "hotkeys": {
//!   "price_check": "Ctrl + D",
//!   "wiki": "Alt + W",
//!   "settings": "Shift + Space",
//!   "commands": [
//!     { "keys": "Ctrl + F1", "text": "/hideout", "send": true },
//!     { "keys": "Alt + F3", "text": "/trade @last", "send": true },
//!     { "keys": "Ctrl + F6", "text": "/afk", "send": false }
//!   ]
//! }
//! ```
//!
//! Keys are written as the modifiers `Ctrl`, `Alt` and `Shift` in any order, then one key: a
//! letter, a digit, `F1` to `F12` or `Space`. A part that is left out has its built-in value:
//! Ctrl + D for the price check, Alt + W for the wiki, Shift + Space for the quick menu, and the one command F5 for `/hideout`.
//! `"wiki": null` is no wiki key, and `"commands": []` no command. An entry that cannot be read,
//! or whose keys another entry already has, is left out and named: nothing here ever stops the
//! program. `send` is yes when a command does not say.
//!
//! Everything here is reading and deciding: which keys, and which of them are to be registered
//! for the window that has the foreground. `main` registers them and `copy` sends what they stand
//! for.

use serde_json::Value;

use crate::keys::is_command_text;

/// A key with the modifiers held with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Combo {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The virtual key, as Windows numbers it.
    pub key: u16,
}

const VK_SPACE: u16 = 0x20;
const VK_F1: u16 = 0x70;

impl Combo {
    pub const fn ctrl(letter: char) -> Self {
        Self { ctrl: true, alt: false, shift: false, key: letter.to_ascii_uppercase() as u16 }
    }

    const fn plain(key: u16) -> Self {
        Self { ctrl: false, alt: false, shift: false, key }
    }

    /// Reads `Ctrl + Alt + D`, `Shift + F2`, `F5`: the case and the spaces do not matter.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let Some(key) = parts.pop().filter(|key| !key.is_empty()) else {
            return Err(format!("\"{text}\" names no key"));
        };
        let mut combo = Self::plain(key_of(key).ok_or_else(|| {
            format!("\"{key}\" in \"{text}\" is no key that can be used: a letter, a digit, F1 to F12 or Space, after the modifiers")
        })?);
        for part in parts {
            let held = match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => &mut combo.ctrl,
                "alt" => &mut combo.alt,
                "shift" => &mut combo.shift,
                _ => return Err(format!("\"{part}\" in \"{text}\" is no modifier: Ctrl, Alt or Shift")),
            };
            if *held {
                return Err(format!("\"{text}\" has {part} twice"));
            }
            *held = true;
        }
        Ok(combo)
    }

    /// As `parse` reads it: `Ctrl + Alt + D`.
    pub fn label(&self) -> String {
        let key = match self.key {
            VK_SPACE => "Space".to_owned(),
            key if (VK_F1..VK_F1 + 12).contains(&key) => format!("F{}", key - VK_F1 + 1),
            key => char::from(key as u8).to_string(),
        };
        [(self.ctrl, "Ctrl"), (self.alt, "Alt"), (self.shift, "Shift")]
            .into_iter()
            .filter(|(held, _)| *held)
            .map(|(_, name)| name.to_owned())
            .chain([key])
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

/// The virtual key of a letter, a digit, `F1` to `F12` or `Space`.
fn key_of(name: &str) -> Option<u16> {
    let upper = name.to_ascii_uppercase();
    match upper.as_bytes() {
        [byte] if byte.is_ascii_uppercase() || byte.is_ascii_digit() => Some(u16::from(*byte)),
        b"SPACE" => Some(VK_SPACE),
        [b'F', number @ ..] if !number.is_empty() && number[0] != b'0' => {
            let number: u16 = std::str::from_utf8(number).ok()?.parse().ok()?;
            (1..=12).contains(&number).then(|| VK_F1 + number - 1)
        }
        _ => None,
    }
}

/// A chat command on a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub keys: Combo,
    pub text: String,
    /// Sent at once, or left in the chat for the player to finish.
    pub send: bool,
}

/// The keys of a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotkeys {
    pub price_check: Combo,
    pub wiki: Option<Combo>,
    /// Shows the quick menu (and closes it); closes the Settings window from inside it.
    pub settings: Option<Combo>,
    /// Opens the panel on a map, with the player's tags on its mods.
    pub map_mods: Option<Combo>,
    pub commands: Vec<Command>,
    /// Saved searches for the game's search boxes, each on its key when it has one.
    pub searches: Vec<Search>,
}

/// The program's own key to quit, which is not the player's to give to something else.
pub const QUIT: Combo = Combo { ctrl: true, alt: false, shift: true, key: b'Q' as u16 };

const PRICE_CHECK: Combo = Combo::ctrl('D');
const WIKI: Combo = Combo { ctrl: false, alt: true, shift: false, key: b'W' as u16 };
const SETTINGS: Combo = Combo { ctrl: false, alt: false, shift: true, key: VK_SPACE };

/// The map mods key when nothing says another: Alt + D.
const MAP_MODS: Combo = Combo { ctrl: false, alt: true, shift: false, key: b'D' as u16 };

/// More commands than anyone has keys for: what a file is read up to.
const MOST_COMMANDS: usize = 48;
use crate::stashsearch::{MOST_SEARCHES, Search};

impl Default for Hotkeys {
    /// What no `hotkeys` object gives.
    fn default() -> Self {
        Self {
            price_check: PRICE_CHECK,
            wiki: Some(WIKI),
            settings: Some(SETTINGS),
            map_mods: Some(MAP_MODS),
            commands: vec![Command { keys: Combo::plain(VK_F1 + 4), text: "/hideout".into(), send: true }],
            searches: Vec::new(),
        }
    }
}

impl Hotkeys {
    /// The keys the settings give: `hotkeys` is that object of the file, when the file has one,
    /// and `key` the letter of `--key`, which is the price check's whatever the file says. The
    /// second part of the answer names what was left out and why.
    ///
    /// The price check comes first, then the wiki, then the commands in their order: of two
    /// entries with the same keys, the later one is left out.
    pub fn read(hotkeys: Option<&Value>, key: Option<char>) -> (Self, Vec<String>) {
        let mut odd = Vec::new();
        let built_in = Self::default();
        let object = match hotkeys {
            None => None,
            Some(Value::Object(object)) => Some(object),
            Some(other) => {
                odd.push(format!("hotkeys is not an object, the built-in keys are used: {other}"));
                None
            }
        };
        let part = |name: &str| object.and_then(|object| object.get(name));

        let mut price_check = match part("price_check") {
            None => built_in.price_check,
            Some(value) => match value.as_str().ok_or_else(|| format!("{value} is not a text")).and_then(Combo::parse) {
                Ok(combo) => combo,
                Err(why) => {
                    odd.push(format!("price_check: {why}; {} is used", built_in.price_check.label()));
                    built_in.price_check
                }
            },
        };
        if let Some(letter) = key.filter(char::is_ascii_alphabetic) {
            price_check = Combo::ctrl(letter);
        }
        let mut taken = vec![(QUIT, "the key that quits".to_owned()), (price_check, "the price check".to_owned())];
        // Says who has the keys already, and otherwise gives them to `what`.
        let mut claim = |combo: Combo, what: &str| match taken.iter().find(|(other, _)| *other == combo) {
            Some((_, owner)) => Err(format!("{} is already {owner}", combo.label())),
            None => {
                taken.push((combo, what.to_owned()));
                Ok(combo)
            }
        };

        let wiki = match part("wiki") {
            None => Some(built_in.wiki.map_or(Err("none".to_owned()), |combo| claim(combo, "the wiki"))),
            // Asked for none.
            Some(Value::Null) => None,
            Some(Value::String(text)) if text.trim().is_empty() => None,
            Some(value) => {
                Some(value.as_str().ok_or_else(|| format!("{value} is not a text")).and_then(Combo::parse).and_then(|combo| claim(combo, "the wiki")))
            }
        };
        let wiki = wiki.and_then(|read| read.map_err(|why| odd.push(format!("wiki: {why}; there is no wiki key"))).ok());

        let settings = match part("settings") {
            None => Some(built_in.settings.map_or(Err("none".to_owned()), |combo| claim(combo, "the Settings window"))),
            Some(Value::Null) => None,
            Some(Value::String(text)) if text.trim().is_empty() => None,
            Some(value) => Some(
                value.as_str().ok_or_else(|| format!("{value} is not a text")).and_then(Combo::parse).and_then(|combo| claim(combo, "the Settings window")),
            ),
        };
        let settings = settings.and_then(|read| read.map_err(|why| odd.push(format!("settings: {why}; there is no key for the Settings window"))).ok());

        let map_mods = match part("map_mods") {
            None => Some(claim(built_in.map_mods.unwrap_or(MAP_MODS), "the map mods")),
            Some(Value::Null) => None,
            Some(Value::String(text)) if text.trim().is_empty() => None,
            Some(value) => Some(value.as_str().ok_or_else(|| format!("{value} is not a text")).and_then(Combo::parse).and_then(|combo| claim(combo, "the map mods"))),
        };
        let map_mods = map_mods.and_then(|read| read.map_err(|why| odd.push(format!("map_mods: {why}; there is no map mods key"))).ok());

        let entries: Vec<Result<Command, String>> = match part("commands") {
            None => built_in.commands.into_iter().map(Ok).collect(),
            Some(Value::Array(entries)) => {
                if entries.len() > MOST_COMMANDS {
                    odd.push(format!("commands: {} entries, only the first {MOST_COMMANDS} are read", entries.len()));
                }
                entries.iter().take(MOST_COMMANDS).map(command_of).collect()
            }
            Some(other) => vec![Err(format!("not a list: {other}"))],
        };
        let mut commands = Vec::new();
        for (index, entry) in entries.into_iter().enumerate() {
            let what = |command: &Command| format!("the command {}", command.text);
            match entry.and_then(|command| claim(command.keys, &what(&command)).map(|_| command)) {
                Ok(command) => commands.push(command),
                Err(why) => odd.push(format!("commands, entry {}: {why}; left out", index + 1)),
            }
        }
        let entries: Vec<Result<Search, String>> = match part("searches") {
            None => Vec::new(),
            Some(Value::Array(entries)) => {
                if entries.len() > MOST_SEARCHES {
                    odd.push(format!("searches: {} entries, only the first {MOST_SEARCHES} are read", entries.len()));
                }
                entries.iter().take(MOST_SEARCHES).map(Search::from_value).collect()
            }
            Some(other) => vec![Err(format!("not a list: {other}"))],
        };
        let mut searches = Vec::new();
        for (index, entry) in entries.into_iter().enumerate() {
            let read = entry.and_then(|mut search| {
                if let Some(keys) = search.keys {
                    claim(keys, &format!("the search {}", search.title()))?;
                }
                search.name = search.name.trim().to_owned();
                Ok(search)
            });
            match read {
                Ok(search) => searches.push(search),
                Err(why) => odd.push(format!("searches, entry {}: {why}; left out", index + 1)),
            }
        }
        (Self { price_check, wiki, settings, map_mods, commands, searches }, odd)
    }

    /// The same from the text of the settings file, when there is one that can be read as JSON:
    /// a file that cannot is the business of whoever reads the rest of it.
    pub fn from_file_text(text: Option<&str>, key: Option<char>) -> (Self, Vec<String>) {
        let file: Option<Value> = text.and_then(|text| serde_json::from_str(text).ok());
        Self::read(file.as_ref().and_then(|file| file.get("hotkeys")), key)
    }

    /// The `hotkeys` object of the file: what `read` gives back as it is.
    pub fn to_value(&self) -> Value {
        let commands: Vec<Value> =
            self.commands.iter().map(|c| serde_json::json!({ "keys": c.keys.label(), "text": c.text, "send": c.send })).collect();
        let searches: Vec<Value> = self.searches.iter().map(Search::to_value).collect();
        serde_json::json!({ "price_check": self.price_check.label(), "wiki": self.wiki.map(|w| w.label()), "settings": self.settings.map(|w| w.label()), "map_mods": self.map_mods.map(|w| w.label()), "commands": commands, "searches": searches })
    }

    /// One line for the log: every key and what it does.
    pub fn summary(&self, game_hotkeys: bool) -> String {
        let mut parts = vec![format!("{} prices the item under the cursor", self.price_check.label())];
        if !game_hotkeys {
            parts.push("no wiki key and no command key in this run".to_owned());
            return parts.join("; ");
        }
        match self.wiki {
            Some(wiki) => parts.push(format!("{} opens its wiki page", wiki.label())),
            None => parts.push("no wiki key".to_owned()),
        }
        if self.commands.is_empty() {
            parts.push("no command key".to_owned());
        }
        for command in &self.commands {
            parts.push(format!("{} {} {}", command.keys.label(), if command.send { "sends" } else { "types" }, command.text));
        }
        if let Some(keys) = self.map_mods {
            parts.push(format!("{} shows the mods of a map", keys.label()));
        }
        if !self.searches.is_empty() {
            parts.push(format!("{} saved searches, {} on a key", self.searches.len(), self.searches.iter().filter(|s| s.keys.is_some()).count()));
        }
        if let Some(settings) = self.settings {
            parts.push(format!("{} opens the quick menu", settings.label()));
        }
        parts.join("; ")
    }
}

/// One entry of `commands`: `{ "keys": "F5", "text": "/hideout", "send": true }`.
fn command_of(entry: &Value) -> Result<Command, String> {
    let Value::Object(entry) = entry else {
        return Err(format!("not an object: {entry}"));
    };
    let text_of = |name: &str| entry.get(name).and_then(Value::as_str).ok_or_else(|| format!("no \"{name}\" that is a text"));
    let keys = Combo::parse(text_of("keys")?)?;
    let text = text_of("text")?;
    if !is_command_text(text) {
        return Err(format!("its text is not one line of something: {text:?}"));
    }
    let send = match entry.get("send") {
        None => true,
        Some(Value::Bool(send)) => *send,
        Some(other) => return Err(format!("\"send\" is neither true nor false: {other}")),
    };
    Ok(Command { keys, text: text.to_owned(), send })
}

/// Which window has the foreground, as far as the hotkeys care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Front {
    /// The game.
    Game,
    /// This program's own panel, which has the keyboard while a number field is typed in.
    Panel,
    /// Anything else: another program, the desktop.
    Other,
}

/// What a hotkey is for. The numbers are the ones the keys are registered under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    PriceCheck,
    Wiki,
    /// Opens the Settings window.
    Settings,
    /// Shows the mods of the map under the cursor.
    MapMods,
    /// The command at this place of `Hotkeys::commands`.
    Command(usize),
    /// The saved search at this place of `Hotkeys::searches`.
    Search(usize),
}

/// The keys to have registered while `front` has the foreground, with what each is for.
///
/// In the game: all of them. Over the panel itself: the price check alone, which is also what
/// hides the panel, and nothing that would send keys to a game that is not in front. Anywhere
/// else: none, so that Ctrl + D is the other program's again. `game_hotkeys` is no in a run that
/// has neither wiki nor command key.
pub fn wanted(hotkeys: &Hotkeys, front: Front, game_hotkeys: bool) -> Vec<(Purpose, Combo)> {
    let mut wanted = Vec::new();
    if front != Front::Other {
        wanted.push((Purpose::PriceCheck, hotkeys.price_check));
    }
    if front == Front::Game && game_hotkeys {
        wanted.extend(hotkeys.wiki.map(|wiki| (Purpose::Wiki, wiki)));
        wanted.extend(hotkeys.commands.iter().enumerate().map(|(index, command)| (Purpose::Command(index), command.keys)));
        wanted.extend(hotkeys.searches.iter().enumerate().filter_map(|(index, search)| search.keys.map(|keys| (Purpose::Search(index), keys))));
        wanted.extend(hotkeys.map_mods.map(|keys| (Purpose::MapMods, keys)));
        wanted.extend(hotkeys.settings.map(|keys| (Purpose::Settings, keys)));
    }
    wanted
}
