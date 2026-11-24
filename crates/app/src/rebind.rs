//! Changing the hotkeys in the Settings window: capturing the next key combo, and deciding
//! whether a set of choices can be kept. Pure: the window and the registering live elsewhere.

use eframe::egui::Key;

use crate::hotkeys::{Combo, Command, Hotkeys, QUIT};
use crate::keys::is_command_text;
use crate::stashsearch::Search;

/// What one press of the keyboard means to a button that waits for a combo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Captured {
    /// Nothing yet: only modifiers are held, or a key that cannot be a hotkey.
    Waiting,
    /// Esc: leave the binding as it was.
    Cancel,
    /// Backspace: the binding is cleared.
    Clear,
    /// A key with its modifiers.
    Set(Combo),
}

/// The virtual key of a key a hotkey can be: a letter, a digit, F1 to F12, Space.
fn virtual_key(key: Key) -> Option<u16> {
    let name = key.name();
    let bytes = name.as_bytes();
    match bytes {
        [b] if b.is_ascii_uppercase() || b.is_ascii_digit() => Some(u16::from(*b)),
        _ if key == Key::Space => Some(0x20),
        [b'F', rest @ ..] if !rest.is_empty() => {
            let number: u16 = name[1..].parse().ok()?;
            (1..=12).contains(&number).then(|| 0x70 + number - 1)
        }
        _ => None,
    }
}

/// Judges one key press: `key` is the key that went down, `None` when only a modifier did.
pub fn capture(key: Option<Key>, ctrl: bool, alt: bool, shift: bool) -> Captured {
    let Some(key) = key else {
        return Captured::Waiting;
    };
    let bare = !(ctrl || alt || shift);
    match key {
        Key::Escape if bare => Captured::Cancel,
        Key::Backspace if bare => Captured::Clear,
        key => virtual_key(key).map_or(Captured::Waiting, |key| Captured::Set(Combo { ctrl, alt, shift, key })),
    }
}

/// A chat command as the window edits it: the keys may not be set yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftCommand {
    pub keys: Option<Combo>,
    pub text: String,
    pub send: bool,
}

/// A saved search as the window edits it: the keys may not be set yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftSearch {
    pub name: String,
    pub text: String,
    pub keys: Option<Combo>,
}

/// All the bindings as the window edits them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub price_check: Option<Combo>,
    pub wiki: Option<Combo>,
    pub settings: Option<Combo>,
    pub map_mods: Option<Combo>,
    pub craft_odds: Option<Combo>,
    pub commands: Vec<DraftCommand>,
    pub searches: Vec<DraftSearch>,
}

/// Which bindings share their keys with another one (or with the key that quits).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflicts {
    pub price_check: bool,
    pub wiki: bool,
    pub settings: bool,
    pub map_mods: bool,
    pub craft_odds: bool,
    pub commands: Vec<bool>,
    pub searches: Vec<bool>,
}

impl Draft {
    pub fn from_hotkeys(hotkeys: &Hotkeys) -> Self {
        Self {
            price_check: Some(hotkeys.price_check),
            wiki: hotkeys.wiki,
            settings: hotkeys.settings,
            map_mods: hotkeys.map_mods,
            craft_odds: hotkeys.craft_odds,
            commands: hotkeys.commands.iter().map(|c| DraftCommand { keys: Some(c.keys), text: c.text.clone(), send: c.send }).collect(),
            searches: hotkeys.searches.iter().map(|s| DraftSearch { name: s.name.clone(), text: s.text.clone(), keys: s.keys }).collect(),
        }
    }

    pub fn conflicts(&self) -> Conflicts {
        let all: Vec<Option<Combo>> = [self.price_check, self.wiki, self.settings, self.map_mods, self.craft_odds]
            .into_iter()
            .chain(self.commands.iter().map(|c| c.keys))
            .chain(self.searches.iter().map(|s| s.keys))
            .collect();
        let clash = |at: usize| all[at].is_some_and(|mine| mine == QUIT || all.iter().enumerate().any(|(other, keys)| other != at && *keys == Some(mine)));
        let commands = self.commands.len();
        Conflicts {
            price_check: clash(0),
            wiki: clash(1),
            settings: clash(2),
            map_mods: clash(3),
            craft_odds: clash(4),
            commands: (0..commands).map(|at| clash(at + 5)).collect(),
            searches: (0..self.searches.len()).map(|at| clash(at + 5 + commands)).collect(),
        }
    }

    pub fn has_conflict(&self) -> bool {
        let c = self.conflicts();
        c.price_check || c.wiki || c.settings || c.map_mods || c.craft_odds || c.commands.contains(&true) || c.searches.contains(&true)
    }

    /// What is kept: nothing while two bindings clash or the price check has no keys. A command
    /// with no keys or no usable text is not part of it, and neither is a search with no usable
    /// text; a search without keys is kept, for the day it gets some.
    pub fn to_hotkeys(&self) -> Option<Hotkeys> {
        if self.has_conflict() {
            return None;
        }
        let commands = self
            .commands
            .iter()
            .filter(|c| is_command_text(&c.text))
            .filter_map(|c| c.keys.map(|keys| Command { keys, text: c.text.clone(), send: c.send }))
            .collect();
        let searches = self
            .searches
            .iter()
            .filter(|s| is_command_text(&s.text))
            .map(|s| Search { name: s.name.trim().to_owned(), text: s.text.clone(), keys: s.keys })
            .collect();
        Some(Hotkeys { price_check: self.price_check?, wiki: self.wiki, settings: self.settings, map_mods: self.map_mods, craft_odds: self.craft_odds, commands, searches })
    }
}
