//! Saved searches for the game's search boxes (the stash, the inventory, any panel with one):
//! one key presses Ctrl+F, selects what the box holds, pastes the search and sends Enter. One
//! press of the key is one search; nothing is moved and nothing leaves the computer.
//!
//! The list lives in `Hotkeys` (the `searches` of its file object). Nothing here sends a key or
//! writes anything.

use serde_json::Value;

use crate::hotkeys::Combo;

/// Most searches a file is read up to.
pub const MOST_SEARCHES: usize = 100;

/// One saved search: what it is called, the text to put in the search box, and its key if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Search {
    pub name: String,
    pub text: String,
    pub keys: Option<Combo>,
}

impl Search {
    /// What to call it in a list: its name, else its text.
    pub fn title(&self) -> &str {
        if self.name.trim().is_empty() { &self.text } else { &self.name }
    }

    /// One entry of `searches`: `{ "name": "Quality", "text": "...", "keys": "F6" }`.
    pub fn from_value(entry: &Value) -> Result<Self, String> {
        let Value::Object(entry) = entry else {
            return Err(format!("not an object: {entry}"));
        };
        let text = entry.get("text").and_then(Value::as_str).ok_or("no \"text\" that is a text")?;
        if !crate::keys::is_command_text(text) {
            return Err(format!("its text is not one line of something: {text:?}"));
        }
        let name = entry.get("name").and_then(Value::as_str).unwrap_or_default().to_owned();
        let keys = match entry.get("keys") {
            None | Some(Value::Null) => None,
            Some(Value::String(keys)) if keys.trim().is_empty() => None,
            Some(Value::String(keys)) => Some(Combo::parse(keys)?),
            Some(other) => return Err(format!("\"keys\" is not a text: {other}")),
        };
        Ok(Self { name, text: text.to_owned(), keys })
    }

    pub fn to_value(&self) -> Value {
        serde_json::json!({ "name": self.name, "text": self.text, "keys": self.keys.map(|keys| keys.label()) })
    }
}
