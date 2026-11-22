//! What a hotkey makes the program send to the game, worked out here and sent elsewhere.
//!
//! A chat command is pasted, never typed: its text goes on the clipboard, the chat is opened
//! with a key, the text is pasted, and Enter sends it when it is to be sent. `chat_keys` gives
//! the keys of one command and `strokes` the key presses and releases they are, after the
//! release of whatever modifier the player still holds from the hotkey itself. `copy_strokes`
//! are those of the copy of the item under the cursor. `wiki_url` is the page of an item.
//!
//! Nothing here reads the keyboard, the clipboard or a window, and nothing here sends anything:
//! `copy` does, for one hotkey the player pressed and never by itself. One command for one
//! press: no text here ever holds two.

use zenith_core::{Item, Rarity};
use zenith_data::Pack;

/// A key of a chat command, with the Ctrl it needs when it needs one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Opens the chat, and sends what it holds.
    Enter,
    /// Opens the chat on a whisper to whoever whispered last: `@name `.
    CtrlEnter,
    /// Ctrl+A: what the chat holds is selected, so that the paste replaces it.
    SelectAll,
    /// Ctrl+V.
    Paste,
    /// Ctrl+F: focuses the search box of the stash or of the panel in front.
    CtrlF,
    Home,
    Delete,
    Up,
    Escape,
}

/// The whisper of the last player, in the text of a command.
const LAST: &str = "@last";

/// The characters the game clears the chat for by itself when a text starts with one.
const CLEARING: [char; 6] = ['#', '%', '@', '$', '&', '/'];

/// After a command was sent the chat stays on it: these bring the channel it was on back.
const RESTORE_CHANNEL: [Key; 4] = [Key::Enter, Key::Up, Key::Up, Key::Escape];

/// The text to put on the clipboard and the keys to press for one chat command, sent at once or
/// left in the chat for the player to finish.
///
/// A plain text: Enter opens the chat, Ctrl+A selects what it holds unless the text starts with
/// a character the game clears the chat for, Ctrl+V pastes.
///
/// A text that ends with `@last`, as `/trade @last`: the rest of it is pasted in front of the
/// name. Ctrl+Enter opens the chat on `@name `, Home and Delete take the `@` away, and the paste
/// lands before the name: `/trade name `.
///
/// A text that starts with `@last `: Ctrl+Enter, then the rest pasted after `@name `.
///
/// When the command is sent: Enter, then the four keys that put the chat back on its channel.
pub fn chat_keys(text: &str, send: bool) -> (String, Vec<Key>) {
    let (clipboard, mut keys) = if let Some(rest) = text.strip_prefix(LAST).and_then(|rest| rest.strip_prefix(' ')) {
        (rest, vec![Key::CtrlEnter, Key::Paste])
    } else if text == LAST {
        // The whisper alone: there is nothing to paste.
        ("", vec![Key::CtrlEnter])
    } else if let Some(rest) = text.strip_suffix(LAST) {
        (rest, vec![Key::CtrlEnter, Key::Home, Key::Delete, Key::Paste])
    } else if text.starts_with(CLEARING) {
        (text, vec![Key::Enter, Key::Paste])
    } else {
        (text, vec![Key::Enter, Key::SelectAll, Key::Paste])
    };
    if send {
        keys.push(Key::Enter);
        keys.extend(RESTORE_CHANNEL);
    }
    (clipboard.to_owned(), keys)
}

/// The text to put on the clipboard and the keys for one saved search: Ctrl+F focuses the search
/// box, Ctrl+A selects what it holds, Ctrl+V pastes the search, Enter applies it. One press of the
/// search's key is exactly these keys and nothing else.
pub fn search_keys(text: &str) -> (String, Vec<Key>) {
    (text.to_owned(), vec![Key::CtrlF, Key::SelectAll, Key::Paste, Key::Enter])
}

/// Whether a text can be a chat command: something, on one line. A line break in a pasted text
/// would be a second command.
pub fn is_command_text(text: &str) -> bool {
    !text.trim().is_empty() && !text.chars().any(char::is_control)
}

// The virtual keys used here, as Windows numbers them.
pub const VK_RETURN: u16 = 0x0D;
pub const VK_ESCAPE: u16 = 0x1B;
pub const VK_HOME: u16 = 0x24;
pub const VK_UP: u16 = 0x26;
pub const VK_DELETE: u16 = 0x2E;
pub const VK_A: u16 = 0x41;
pub const VK_C: u16 = 0x43;
pub const VK_F: u16 = 0x46;
pub const VK_V: u16 = 0x56;
pub const VK_LSHIFT: u16 = 0xA0;
pub const VK_RSHIFT: u16 = 0xA1;
pub const VK_LCONTROL: u16 = 0xA2;
pub const VK_RCONTROL: u16 = 0xA3;
pub const VK_LMENU: u16 = 0xA4;
pub const VK_RMENU: u16 = 0xA5;

/// The modifier keys a hotkey can be made with, each side of the keyboard apart: the ones to
/// look at for what the player still holds.
pub const MODIFIERS: [u16; 6] = [VK_LSHIFT, VK_RSHIFT, VK_LCONTROL, VK_RCONTROL, VK_LMENU, VK_RMENU];

/// A key going down or coming up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stroke {
    pub vk: u16,
    pub down: bool,
}

const fn down(vk: u16) -> Stroke {
    Stroke { vk, down: true }
}

const fn up(vk: u16) -> Stroke {
    Stroke { vk, down: false }
}

fn is_ctrl(vk: u16) -> bool {
    matches!(vk, VK_LCONTROL | VK_RCONTROL)
}

/// A key of the blocks beside the letters, or a modifier of the right-hand side: Windows tells
/// those from their twins of the number pad and of the left-hand side by a flag.
pub fn is_extended(vk: u16) -> bool {
    matches!(vk, VK_HOME | VK_UP | VK_DELETE | VK_RCONTROL | VK_RMENU)
}

/// One key pressed and released, inside a Ctrl of our own when it needs one.
fn tap(out: &mut Vec<Stroke>, vk: u16, ctrl: bool) {
    if ctrl {
        out.push(down(VK_LCONTROL));
    }
    out.extend([down(vk), up(vk)]);
    if ctrl {
        out.push(up(VK_LCONTROL));
    }
}

/// The presses and releases of a chat command, to be handed to Windows in one go so that nothing
/// comes between them. First the release of every modifier the player still holds (`held`, from
/// the hotkey: Shift for Shift+F2), without which the game would see Shift+Enter and Shift+V;
/// then each key, with a Ctrl pressed and released around those that need it. A modifier that
/// was released is not pressed again: the player lets go of the real key a moment later.
pub fn strokes(held: &[u16], keys: &[Key]) -> Vec<Stroke> {
    let mut out: Vec<Stroke> = held.iter().copied().map(up).collect();
    for key in keys {
        match key {
            Key::Enter => tap(&mut out, VK_RETURN, false),
            Key::CtrlEnter => tap(&mut out, VK_RETURN, true),
            Key::SelectAll => tap(&mut out, VK_A, true),
            Key::Paste => tap(&mut out, VK_V, true),
            Key::CtrlF => tap(&mut out, VK_F, true),
            Key::Home => tap(&mut out, VK_HOME, false),
            Key::Delete => tap(&mut out, VK_DELETE, false),
            Key::Up => tap(&mut out, VK_UP, false),
            Key::Escape => tap(&mut out, VK_ESCAPE, false),
        }
    }
    out
}

/// The presses and releases that make the game copy the item under the cursor: Ctrl+C and no
/// other modifier. A Ctrl the player holds (the hotkey's own, as in Ctrl+D) is used as it is and
/// left alone, and then a lone C is all that is sent; without one, a Ctrl is pressed and
/// released around the C. Any other modifier that is held (Alt for Alt+W) is released first: the
/// game copies something else on Ctrl+Alt+C.
pub fn copy_strokes(held: &[u16]) -> Vec<Stroke> {
    let mut out: Vec<Stroke> = held.iter().copied().filter(|vk| !is_ctrl(*vk)).map(up).collect();
    tap(&mut out, VK_C, !held.iter().copied().any(is_ctrl));
    out
}

/// The releases owed after only the first strokes of a batch went in: one for each key they
/// left down, the last pressed first. A key that was only released (a modifier of the player's)
/// is owed nothing.
pub fn left_down(sent: &[Stroke]) -> Vec<Stroke> {
    let mut pressed: Vec<u16> = Vec::new();
    for stroke in sent {
        if stroke.down {
            pressed.push(stroke.vk);
        } else if let Some(at) = pressed.iter().rposition(|vk| *vk == stroke.vk) {
            pressed.remove(at);
        }
    }
    pressed.into_iter().rev().map(up).collect()
}

/// Where the pages of the wiki are.
const WIKI: &str = "https://www.poewiki.net/wiki/";

/// What the wiki calls the item: a unique by its name, anything else by its base type. The only
/// line of a magic item holds its affixes too: its base is the longest base of the pack that the
/// line has as whole words, and the whole line when the pack has none or there is no pack.
pub fn wiki_name(item: &Item, pack: Option<&Pack>) -> String {
    match (&item.rarity, &item.name) {
        (Rarity::Unique, Some(name)) => name.clone(),
        (Rarity::Magic, _) => {
            let line = format!(" {} ", item.base);
            pack.into_iter()
                .flat_map(|pack| pack.bases.values().flatten())
                .filter(|base| line.contains(&format!(" {base} ")))
                .max_by_key(|base| base.len())
                .cloned()
                .unwrap_or_else(|| item.base.clone())
        }
        _ => item.base.clone(),
    }
}

/// The page of the wiki for a name: its spaces as underscores, and everything but letters,
/// digits and `-._~` percent-encoded. `None` for a name that is empty.
pub fn wiki_url(name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let mut url = String::from(WIKI);
    for byte in name.bytes() {
        match byte {
            b' ' => url.push('_'),
            byte if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') => url.push(char::from(byte)),
            byte => url.push_str(&format!("%{byte:02X}")),
        }
    }
    Some(url)
}

/// Whether `url` is a page of the wiki as `wiki_url` writes one and nothing else: with the page
/// of a search on the trade site, the only thing the program ever hands to the system to open.
pub fn is_wiki_url(url: &str) -> bool {
    url.strip_prefix(WIKI).is_some_and(|page| {
        !page.is_empty() && page.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'%'))
    })
}
