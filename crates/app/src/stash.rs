//! Changing stash tab with Ctrl and the mouse wheel, over the inventory panel of the game: the
//! decisions, pure.
//!
//! The game changes tab by itself on Ctrl + wheel with the pointer over the stash, the left panel.
//! Over the inventory, the right panel, while the stash is open, it does not: there a notch of
//! the wheel with Ctrl held is turned into one arrow key, the game's own key for the next or the
//! previous tab, and the wheel itself is not given to the game. Everywhere else, the stash
//! included, the wheel is left alone: Ctrl + wheel zooms the game world as ever. One key at most
//! for one turn of the wheel, and none unless the game is the foreground window: see `claims` and
//! `wheel_action`.
//!
//! Nothing here reads the mouse, the keyboard or a window, and nothing here sends anything: the
//! mouse hook asks `claims`, hands the turns of the wheel to a thread of `main`, which asks these
//! functions and has `copy` press the key.

use windows::Win32::Foundation::{POINT, RECT};

/// The arrow key the game changes stash tab with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// The previous tab.
    Left,
    /// The next tab.
    Right,
}

/// What a wheel reports for one notch.
pub const NOTCH: i32 = 120;

/// Whether `point` is over the inventory panel of the game, the right one, while the stash is
/// open on the left. Both are in screen coordinates: `client` is the client rectangle of the
/// game's window.
///
/// Each panel is as wide as the window is tall makes it, whatever the width of the window (an
/// ultrawide one has them anchored to its edges all the same): for a client area `h` tall,
/// round(h * 986 / 1600). The inventory is that wide from the right edge of the client area, over
/// the whole height. The far edges are outside, as in every rectangle here. A rectangle without a
/// height has no panel. The left panel, the stash, is not here: the game changes its tab by
/// itself there.
pub fn in_inventory_area(point: POINT, client: RECT) -> bool {
    let height = client.bottom - client.top;
    if height <= 0 {
        return false;
    }
    let width = (f64::from(height) * 986.0 / 1600.0).round() as i32;
    let (x, y) = (point.x - client.left, point.y - client.top);
    let right = client.right - client.left;
    (right - width..right).contains(&x) && (0..height).contains(&y)
}

/// Whether the wheel is ours to take: Ctrl is held, the game is the foreground window, the
/// pointer is over the inventory panel and not over this program's own panel (where the wheel steps
/// a number field). When it is, the wheel is turned into keys and not given to the game; when it
/// is not, it goes on to whatever is under the pointer, untouched.
pub fn claims(ctrl: bool, game_foreground: bool, over_panel: bool, in_inventory: bool) -> bool {
    ctrl && game_foreground && !over_panel && in_inventory
}

/// The key a turn of the wheel is, if it is one: the wheel turned toward the player (`delta`
/// below zero, as Windows counts it) is the next tab, away from the player the previous one.
/// Nothing unless `claims`.
pub fn wheel_action(delta: i32, ctrl: bool, game_foreground: bool, over_panel: bool, in_inventory: bool) -> Option<Key> {
    if !claims(ctrl, game_foreground, over_panel, in_inventory) {
        return None;
    }
    match delta.signum() {
        -1 => Some(Key::Right),
        1 => Some(Key::Left),
        _ => None,
    }
}

/// The wheel as it turns, one report after the other: what is left of a notch between two of
/// them. A wheel of high resolution reports a notch in many small parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Wheel {
    /// What it has turned since the last whole notch, in the units of `NOTCH`.
    turned: i32,
}

impl Wheel {
    /// One report of the wheel, with what `wheel_action` needs to know. Returns the key to press
    /// once this report completes a notch: one key for a notch, and never more than one for a
    /// report, whatever it says the wheel turned (the part of a notch left over is kept, a second
    /// notch in the same report is not). A report that is no key for `wheel_action`, or that
    /// turns the other way, starts the count again.
    pub fn turn(&mut self, delta: i32, ctrl: bool, game_foreground: bool, over_panel: bool, in_inventory: bool) -> Option<Key> {
        let Some(key) = wheel_action(delta, ctrl, game_foreground, over_panel, in_inventory) else {
            self.turned = 0;
            return None;
        };
        if self.turned.signum() == -delta.signum() {
            self.turned = 0;
        }
        self.turned = self.turned.saturating_add(delta);
        if self.turned.unsigned_abs() < NOTCH.unsigned_abs() {
            return None;
        }
        self.turned %= NOTCH;
        Some(key)
    }
}

/// How far the wheel turned, from the `mouseData` of the event the mouse hook is given: its high
/// word, signed. Above zero the wheel turned away from the user, below zero toward the user.
pub fn wheel_delta(mouse_data: u32) -> i16 {
    (mouse_data >> 16) as u16 as i16
}

/// A turn of the wheel as the number a thread message carries, and back: the sign must come
/// through, since it is the direction.
pub fn delta_to_message(delta: i16) -> usize {
    delta as isize as usize
}

pub fn delta_from_message(carried: usize) -> i32 {
    carried as isize as i32
}

/// How old a report of the wheel may be when it is looked at, in milliseconds. One that waited
/// longer is no key any more: a key must follow the hand that turned the wheel, not come later.
pub const STALE_MS: u32 = 200;

/// Whether a report of the wheel is too old to be a key: `reported` and `now` are both
/// milliseconds on the clock of Windows, which starts again from zero every 49 days.
pub fn stale(reported: u32, now: u32) -> bool {
    now.wrapping_sub(reported) > STALE_MS
}

/// Why the wheel is left to the game alone, when it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Off {
    /// `--no-stash-scroll`.
    Flag,
    /// `stash_scroll` is `false` in the settings file.
    Setting,
    /// A run that shows the fake item sends nothing to the game, whatever it is told.
    FakeItem,
}

/// Whether this run changes stash tab on Ctrl + wheel: it does unless the flag, the settings file
/// or the kind of run says no. `setting` is what the settings file holds, which is yes when it
/// says nothing.
pub fn enabled(setting: bool, flag_off: bool, fake_item: bool) -> Result<(), Off> {
    if fake_item {
        Err(Off::FakeItem)
    } else if flag_off {
        Err(Off::Flag)
    } else if !setting {
        Err(Off::Setting)
    } else {
        Ok(())
    }
}
