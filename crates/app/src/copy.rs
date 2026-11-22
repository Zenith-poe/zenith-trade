//! Copies the item under the cursor in the game: Ctrl+C, with the Ctrl the user still holds from
//! the hotkey when there is one, then one read of the clipboard. Changes stash tab: one synthetic
//! arrow key for one notch of the wheel, while the user holds Ctrl (see `stash` for when). And
//! puts one chat command in the game's chat for one press of its key: the text through the
//! clipboard, which gets its own text back afterwards, and the keys `keys::chat_keys` gives.
//!
//! This is the only place that sends input, and it does so only when the game is the foreground
//! window, for one thing the user did and never by itself. What is sent is worked out in `keys`,
//! where it is tested; here it is only handed to Windows. No test calls anything here but
//! `is_game_title`: the game may be running, and nothing here has ever been run by a test.

use std::time::{Duration, Instant};

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput, VIRTUAL_KEY,
    VK_CONTROL, VK_LEFT, VK_RIGHT,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};

use crate::keys::{self, Stroke};
use crate::stash;

/// The game's window title, whole.
const GAME_TITLE: &str = "Path of Exile";

/// How long the game gets to read a command off the clipboard before the clipboard gets its
/// own text back.
const PASTE_WAIT: Duration = Duration::from_millis(120);

/// How long the game gets to put the item on the clipboard.
const COPY_WAIT: Duration = Duration::from_millis(300);
const COPY_POLL: Duration = Duration::from_millis(4);

/// Another program may hold the clipboard for a moment (the game itself, while it writes): six
/// tries, 10 ms apart.
const OPEN_TRIES: u32 = 6;
const OPEN_RETRY: Duration = Duration::from_millis(10);

/// The standard clipboard format for UTF-16 text.
const CF_UNICODETEXT: u32 = 13;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyError {
    /// The foreground window is not the game. Nothing was sent.
    GameNotForeground,
    /// Nothing new could be read from the clipboard in time: nothing was under the cursor, the
    /// copy could not be sent (input blocked), or another program kept the clipboard open.
    Timeout,
    /// The clipboard changed but holds no text.
    NotText,
}

pub fn is_game_title(title: &str) -> bool {
    title == GAME_TITLE
}

/// The foreground window, when its title is exactly the game's. Reading the title of another
/// program's window sends that window no message.
pub fn game_window() -> Option<HWND> {
    let window = unsafe { GetForegroundWindow() };
    // Room for more than the title: a longer one must not be cut down to a match.
    let mut buf = [0u16; 64];
    let len = unsafe { GetWindowTextW(window, &mut buf) }.max(0) as usize;
    is_game_title(&String::from_utf16_lossy(&buf[..len])).then_some(window)
}

/// Whether the foreground window is the game.
pub fn foreground_is_game() -> bool {
    game_window().is_some()
}

/// Makes the game copy the item under the cursor and returns the text it put on the clipboard.
///
/// To be called from a hotkey. The game is sent Ctrl+C and nothing else: with the Ctrl the user
/// still holds from the hotkey when it has one (Ctrl+D: a lone C is sent, as ever), and
/// otherwise with a Ctrl pressed and released around the C, after the release of any other
/// modifier that is held (Alt+W): see `keys::copy_strokes`. One batch of input is sent at most
/// and the clipboard is read at most once. Blocks for up to about 350 ms.
pub fn copy_hovered_item() -> Result<String, CopyError> {
    if !foreground_is_game() {
        return Err(CopyError::GameNotForeground);
    }
    let before = unsafe { GetClipboardSequenceNumber() };
    if !send(&keys::copy_strokes(&held_modifiers())) {
        return Err(CopyError::Timeout);
    }
    let deadline = Instant::now() + COPY_WAIT;
    while unsafe { GetClipboardSequenceNumber() } == before {
        if Instant::now() >= deadline {
            return Err(CopyError::Timeout);
        }
        std::thread::sleep(COPY_POLL);
    }
    read_clipboard_text()
}

/// Why an arrow key was not sent for a notch of the wheel, or a command for its key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotSent {
    /// The foreground window is not the game.
    GameNotForeground,
    /// Ctrl was released meanwhile: a lone arrow key is not what the user asked for.
    CtrlReleased,
    /// The command could not be put on the clipboard: another program kept it open.
    Clipboard,
    /// Windows did not take the key presses, or not all of them.
    Refused,
}

/// Puts one chat command in the game's chat, and sends it when `send` says so: its text through
/// the clipboard and the keys of `keys::chat_keys`, after the release of whatever modifier the
/// user still holds from the hotkey. To be called for one press of the command's key, and for
/// nothing else. Nothing is sent unless the game is the foreground window at that very moment.
///
/// The clipboard gets the text it held back once the game has had the time to read the command;
/// when it held no text, the command stays on it. Blocks for about 150 ms.
pub fn send_chat(text: &str, send_it: bool) -> Result<(), NotSent> {
    if !foreground_is_game() {
        return Err(NotSent::GameNotForeground);
    }
    let (pasted, chat) = keys::chat_keys(text, send_it);
    paste_keys(&pasted, &chat)
}

/// Puts one saved search in the game's search box and applies it (see `keys::search_keys`), for
/// one press of the search's key and nothing else. The clipboard gets its own text back.
pub fn send_search(text: &str) -> Result<(), NotSent> {
    if !foreground_is_game() {
        return Err(NotSent::GameNotForeground);
    }
    let (pasted, chat) = keys::search_keys(text);
    paste_keys(&pasted, &chat)
}

/// The clipboard, the keys, and the clipboard back: what `send_chat` and `send_search` share.
fn paste_keys(pasted: &str, chat: &[keys::Key]) -> Result<(), NotSent> {
    let pastes = chat.contains(&keys::Key::Paste);
    let saved = if pastes { read_clipboard_text().ok() } else { None };
    if pastes && !write_clipboard_text(pasted) {
        return Err(NotSent::Clipboard);
    }
    let restore = || {
        if let Some(saved) = &saved {
            write_clipboard_text(saved);
        }
    };
    // The clipboard took a moment: whoever is in front now is who the keys would go to.
    if !foreground_is_game() {
        restore();
        return Err(NotSent::GameNotForeground);
    }
    let sent = send(&keys::strokes(&held_modifiers(), chat));
    if pastes {
        std::thread::sleep(PASTE_WAIT);
        restore();
    }
    if sent { Ok(()) } else { Err(NotSent::Refused) }
}

/// One arrow key to the game, the one it changes stash tab with: the next tab or the previous
/// one. To be called for one notch of the wheel the user turned with Ctrl held, and for nothing
/// else. At most one key press is sent, and none unless the game is the foreground window and
/// Ctrl is still held at this very moment, whatever the caller found a moment ago.
pub fn press_stash_key(key: stash::Key) -> Result<(), NotSent> {
    if !foreground_is_game() {
        return Err(NotSent::GameNotForeground);
    }
    if !ctrl_is_held() {
        return Err(NotSent::CtrlReleased);
    }
    let arrow = match key {
        stash::Key::Left => VK_LEFT,
        stash::Key::Right => VK_RIGHT,
    };
    // The arrows of the block beside the letters, not those of the number pad.
    if press(arrow, true) { Ok(()) } else { Err(NotSent::Refused) }
}

/// Reads the keyboard's state. Sends nothing.
pub fn ctrl_is_held() -> bool {
    unsafe { GetAsyncKeyState(VK_CONTROL.0 as i32) < 0 }
}

/// The modifier keys that are down at this moment, each side of the keyboard apart. Reads the
/// keyboard's state. Sends nothing.
fn held_modifiers() -> Vec<u16> {
    keys::MODIFIERS.into_iter().filter(|vk| unsafe { GetAsyncKeyState(i32::from(*vk)) } < 0).collect()
}

/// Hands the strokes to Windows in one call, so that nothing the user types comes between them.
/// Says whether all of them went in. When only the first went in, the keys they left down are
/// released at once.
fn send(strokes: &[Stroke]) -> bool {
    let input = |stroke: &Stroke| {
        // The virtual key and the scan code it has on this keyboard layout, as a real key press
        // carries both.
        let scan = unsafe { MapVirtualKeyW(u32::from(stroke.vk), MAPVK_VK_TO_VSC) } as u16;
        let block = if keys::is_extended(stroke.vk) { KEYEVENTF_EXTENDEDKEY } else { KEYBD_EVENT_FLAGS(0) };
        let way = if stroke.down { KEYBD_EVENT_FLAGS(0) } else { KEYEVENTF_KEYUP };
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT { wVk: VIRTUAL_KEY(stroke.vk), wScan: scan, dwFlags: way | block, time: 0, dwExtraInfo: 0 },
            },
        }
    };
    let size = size_of::<INPUT>() as i32;
    let inputs: Vec<INPUT> = strokes.iter().map(input).collect();
    let taken = unsafe { SendInput(&inputs, size) } as usize;
    if taken >= strokes.len() {
        return true;
    }
    let owed: Vec<INPUT> = keys::left_down(&strokes[..taken]).iter().map(input).collect();
    if !owed.is_empty() {
        unsafe { SendInput(&owed, size) };
    }
    false
}

/// One key down and the same key up, in one call so that nothing comes between them. The
/// modifiers are left alone. `extended` is for a key of the blocks beside the letters, whose scan
/// code the number pad has too. False when the press did not go in.
fn press(vk: VIRTUAL_KEY, extended: bool) -> bool {
    // The virtual key and the scan code it has on this keyboard layout, as a real key press
    // carries both.
    let scan = unsafe { MapVirtualKeyW(u32::from(vk.0), MAPVK_VK_TO_VSC) } as u16;
    let block = if extended { KEYEVENTF_EXTENDEDKEY } else { KEYBD_EVENT_FLAGS(0) };
    let key = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags | block, time: 0, dwExtraInfo: 0 },
        },
    };
    let (down, up) = (key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP));
    let size = size_of::<INPUT>() as i32;
    match unsafe { SendInput(&[down, up], size) } {
        2 => true,
        1 => {
            // The press went in without its release: the key must not stay down.
            unsafe { SendInput(&[up], size) };
            true
        }
        _ => false,
    }
}

/// The clipboard, open. Closed when dropped, whatever happens in between.
struct OpenedClipboard;

impl OpenedClipboard {
    fn open() -> Option<Self> {
        for attempt in 0..OPEN_TRIES {
            if attempt > 0 {
                std::thread::sleep(OPEN_RETRY);
            }
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Some(Self);
            }
        }
        None
    }
}

impl Drop for OpenedClipboard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

/// Makes `text` the clipboard's content, as Unicode text. Says whether it is.
fn write_clipboard_text(text: &str) -> bool {
    let units: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let Some(_clipboard) = OpenedClipboard::open() else {
        return false;
    };
    unsafe {
        let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, units.len() * size_of::<u16>()) else {
            return false;
        };
        let start = GlobalLock(memory) as *mut u16;
        if start.is_null() {
            let _ = GlobalFree(Some(memory));
            return false;
        }
        std::ptr::copy_nonoverlapping(units.as_ptr(), start, units.len());
        let _ = GlobalUnlock(memory);
        // Once the clipboard has taken the block it is the clipboard's: freed here only if not.
        if EmptyClipboard().is_err() || SetClipboardData(CF_UNICODETEXT, Some(HANDLE(memory.0))).is_err() {
            let _ = GlobalFree(Some(memory));
            return false;
        }
    }
    true
}

/// The clipboard's content as Unicode text.
fn read_clipboard_text() -> Result<String, CopyError> {
    let _clipboard = OpenedClipboard::open().ok_or(CopyError::Timeout)?;
    unsafe {
        // The handle belongs to the clipboard: it is locked to be read, never freed.
        let handle = GetClipboardData(CF_UNICODETEXT).map_err(|_| CopyError::NotText)?;
        let memory = HGLOBAL(handle.0);
        let start = GlobalLock(memory) as *const u16;
        if start.is_null() {
            return Err(CopyError::NotText);
        }
        // The text ends at its NUL; the size of the block bounds the search for it.
        let units = std::slice::from_raw_parts(start, GlobalSize(memory) / size_of::<u16>());
        let len = units.iter().position(|&unit| unit == 0).unwrap_or(units.len());
        let text = String::from_utf16_lossy(&units[..len]);
        let _ = GlobalUnlock(memory);
        Ok(text)
    }
}
