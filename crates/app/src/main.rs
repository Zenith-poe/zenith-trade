//! Zenith: a hotkey-driven, non-activating, per-pixel transparent overlay panel that prices the
//! item under the cursor in the game: a unique by its name, other gear on its mods.
//!
//!   zenith                      run; Ctrl+D over an item in the game copies it and shows its
//!                               price, Ctrl+D again, Esc or a left click outside the panel
//!                               hides, Ctrl+Shift+Q quits
//!   zenith --key F              use Ctrl+F instead of Ctrl+D, whatever the settings file says
//!   zenith --no-game-hotkeys    no wiki key and no command key in this run: see below
//!   zenith --update-data        download the game data (stats, bases, uniques) from the trade
//!                               site, save it and say what changed: to run after a game patch
//!   zenith --update-prices [league]
//!                               download the price list of a league (the current one when none is
//!                               named) from poe.ninja and save it. The panel itself fetches a list
//!                               at most every 30 minutes, and five minutes after a failure
//!   zenith --price item.txt     price the item copied into a text file, on the console: no
//!                               window, no hotkey, no input sent
//!   zenith --no-search          with --price: read the item, list its mods and the filters a
//!                               search would use, and stop there; no request at all
//!   zenith --way base           with --price: price a piece of gear as a base (`base`) or as
//!                               the item it is (`item`), whatever it opens on; the report says
//!                               which way was used
//!   zenith --no-trim            keep the working set when the panel hides (it is emptied by default)
//!   zenith --no-stash-scroll    leave the wheel alone: see below
//!
//! With Ctrl held and the pointer over the inventory panel of the game (the right of its window,
//! as wide as the window is tall makes it), a notch of the mouse wheel changes stash tab: toward
//! you the next tab, away from you the previous one. One arrow key is sent to the game for one
//! notch and the wheel is not given to the game. Over the stash (the left panel) the game does
//! that by itself, and over the world Ctrl + wheel zooms: there the wheel is never touched. Only
//! while the game is the foreground window. It is on unless `--no-stash-scroll` is given or the
//! settings file says `"stash_scroll": false`.
//!
//! The keys are the game's only: they are registered while the game is the foreground window
//! and given back to Windows as soon as another program is in front, so that Ctrl+D is that
//! program's own shortcut again. The price-check key alone stays while the panel itself has the
//! keyboard (a number field is being typed in): it is what hides the panel. Ctrl+Shift+Q, which
//! quits, is the program's own and works anywhere.
//!
//! Besides the price check: a key that opens the wiki page of the item under the cursor in the
//! browser (Alt+W), and keys that each put one chat command in the game's chat, sent at once or
//! left there to be finished (F5 for `/hideout`). Which keys and which commands is the player's
//! to say in the settings file: see `hotkeys`. A command is pasted, so the clipboard is borrowed
//! for a moment and gets its text back. One command for one press of a key, and none unless the
//! game is in front. `--no-game-hotkeys` leaves the price check alone and registers no other
//! key; the runs that show a fake item never register one.
//!
//! In the panel, a click on a property or a mod switches it on or off in the search. A row that
//! is searched has two number fields, its minimum and its maximum: the wheel over one steps it,
//! and a click in one lets it be typed in. Nothing is searched again by itself: "Search again"
//! (or Enter in a field) sends the search as it stands, as long as the site's limits leave a
//! request (the pips beside it), and "Open trade site" opens the last search in the browser.
//!
//! Under the estimate of a piece of gear, the two ways of pricing it: "Item", on its defences
//! and its best mods, and "Base" ("Fractured base" for a fractured item, which opens on it), on
//! its base, its item level and its fractured mods alone. A click on the other one replaces the
//! filters and the options with its own and searches at once, when a request is left.
//!
//! Under them, the chips of the item's own options: its base, its item level, its
//! quality, its linked sockets, corrupted or not. A click lights a chip, which makes it part of
//! the search; the number of a lit chip is a field like the others. Above the buttons, the three
//! settings of every search: buyout mode, currency, age of the listings. A click on one goes to
//! its next value, a right click lists them all, and the pin at its left keeps the value for
//! every check to come. Esc closes an open menu first, then leaves a field, then hides the panel.
//!
//! The mods of a unique are switches too, all off to start with: the item is searched by its
//! name. One that is switched on is asked from the value the item has. The bar at the left of a
//! mod shows where its roll sits in the range it was rolled in.
//!
//! A searched mod whose range is known has two quick ways to its minimum. A right click on the
//! minimum lists the rolls it can be set to at once: the worst, this item's, the best. And while
//! the pointer is over its row, or one of its fields is being typed in, a slider shows under its
//! text, inside the row: the range from its low end to its high end, filled as far as the
//! minimum, with a tick at the item's own roll. A press on it sets the minimum there, a drag
//! takes it along, in whole numbers over a range of 10 or more and in tenths otherwise, and the
//! release is what changes the search; a double click sets the minimum to the high end; the
//! wheel over it steps the minimum as it does over the field.
//!
//! A click in the panel never takes the keyboard from the game, except a click in a number
//! field: the panel has the keyboard while a field has the caret, and hands it back when the
//! field is left (Esc, Enter, a click elsewhere in the panel) and whenever the panel hides. The
//! slider is the pointer's alone, like the switch of a row.
//!
//! The league is the current trade league of the site's list; the environment variable
//! ZENITH_LEAGUE names another. The game data is kept in `%APPDATA%\Zenith Trade\pack.json`; the
//! environment variable ZENITH_PACK names another file. The pinned settings are kept in
//! `%APPDATA%\Zenith Trade\config.json`; the environment variable ZENITH_CONFIG names another
//! file.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, AtomicU64, Ordering::SeqCst};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use eframe::egui;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAK, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::{ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::ProcessStatus::{
    EmptyWorkingSet, GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::{GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, SetFocus,
    UnregisterHotKey, VK_ESCAPE, VK_LBUTTON, VK_RBUTTON,
};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass, ShellExecuteW};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};
use zenith_trade::client::Budget;

use crate::check::{Action, Effect};
use crate::state::View;

const ID_TOGGLE: i32 = 1;
const ID_ESC: i32 = 2;
const ID_QUIT: i32 = 3;
const ID_WIKI: i32 = 4;
const ID_SETTINGS: i32 = 5;
const ID_MAPS: i32 = 6;
/// The command at place n of the hotkeys is registered under this number plus n.
const ID_COMMAND: i32 = 100;
/// The saved searches' keys, above every command's.
const ID_SEARCH: i32 = 1000;

// ---- shared state (everything that matters runs on the main thread; the self-test thread
// ---- only posts messages and reads these) -------------------------------------------------
static START: OnceLock<Instant> = OnceLock::new();
static CTX: OnceLock<egui::Context> = OnceLock::new();
static PANEL: AtomicIsize = AtomicIsize::new(0);
static MAIN_THREAD: AtomicU32 = AtomicU32::new(0);
static READY: AtomicBool = AtomicBool::new(false);
static TRIM: AtomicBool = AtomicBool::new(false);
static VISIBLE: AtomicBool = AtomicBool::new(false);
static SHOWS: AtomicU64 = AtomicU64::new(0);
static PRESENTED: AtomicU64 = AtomicU64::new(0);
static UI_PASSES: AtomicU64 = AtomicU64::new(0);
static LOGIC_PASSES: AtomicU64 = AtomicU64::new(0);
static FOCUS_KEPT: AtomicU64 = AtomicU64::new(0);
static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
static HOOK_THREAD: AtomicU32 = AtomicU32::new(0);
/// The hook watches for a left click outside the panel: the panel is up.
static HOOKED: AtomicBool = AtomicBool::new(false);
/// The hook hands the turns of the wheel to the stash thread: the stash scroll is on.
static WHEEL_WATCHED: AtomicBool = AtomicBool::new(false);
/// This run changes stash tab on Ctrl and the wheel. Set once, before any thread is started.
static STASH_SCROLL: AtomicBool = AtomicBool::new(false);
static STASH_THREAD: AtomicU32 = AtomicU32::new(0);
/// The arrow keys sent to the game for the stash scroll so far.
static STASH_KEYS: AtomicU64 = AtomicU64::new(0);
static OUTSIDE_CLICKS: AtomicU64 = AtomicU64::new(0);
static TIMINGS: Mutex<Vec<f64>> = Mutex::new(Vec::new());
/// What the panel draws. Written on the main thread only.
static VIEW: Mutex<View> = Mutex::new(View::Message(String::new()));
/// The number of the price check the panel is showing. Every show starts a new one and every
/// hide abandons the current one: the worker makes no further request for an older number, and
/// what it still sends for one is dropped.
static CHECK: AtomicU64 = AtomicU64::new(0);
/// To the worker thread. Unset in a run that shows the fake item.
static JOBS: OnceLock<Sender<worker::Job>> = OnceLock::new();
/// Where the worker thread puts the data pack it has read: the first view of an item is made
/// with it. Unset in a run that shows the fake item.
static PACK: OnceLock<worker::PackSlot> = OnceLock::new();
/// When the panel was shown; `None` while it is hidden. The bars of a view that is loading pulse
/// from there.
static SHOWN_AT: Mutex<Option<Instant>> = Mutex::new(None);
/// Where the worker thread says how many searches the limits allow. Unset in a run that shows
/// the fake item.
static BUDGET: OnceLock<worker::BudgetSlot> = OnceLock::new();
/// What could be clicked in the frame last painted, for the self-test to aim at.
static ZONES: Mutex<Vec<panel::Zone>> = Mutex::new(Vec::new());
/// Which number field is being typed in and what was left in the others, for the show in
/// progress. Changed on the main thread only, but for `--render`, before its only frame.
static FIELDS: Mutex<field::Fields> = Mutex::new(field::Fields::new());
/// Where the number fields were in the frame last painted, for the self-tests to aim at.
static NUM_FIELDS: Mutex<Vec<panel::NumField>> = Mutex::new(Vec::new());
/// The window that had the foreground when the panel first took the keyboard during this show:
/// the one to give it back to. 0 while the panel has not taken it.
static RETURN_TO: AtomicIsize = AtomicIsize::new(0);
/// What the process ends with.
static EXIT_CODE: AtomicI32 = AtomicI32::new(0);
/// The keys of this run. Set once, before any is registered.
static HOTKEYS: std::sync::RwLock<Option<hotkeys::Hotkeys>> = std::sync::RwLock::new(None);

/// This run has a wiki key and command keys. Never in a run that shows the fake item.
static GAME_HOTKEYS: AtomicBool = AtomicBool::new(false);
/// The foreground window is watched: the keys come and go with the game. When Windows refuses
/// the watch, they stay registered for the whole run, as they used to.
static FRONT_WATCHED: AtomicBool = AtomicBool::new(false);
/// The numbers of the hotkeys that are registered now, the window in front they were chosen
/// for, and the ones Windows refused last time. Touched on the main thread only.
static REGISTERED: Mutex<Vec<i32>> = Mutex::new(Vec::new());
static FRONT: Mutex<Option<hotkeys::Front>> = Mutex::new(None);
static REFUSED: Mutex<Vec<i32>> = Mutex::new(Vec::new());
/// To the thread that sends a chat command or opens the wiki, with when the key was pressed.
/// Unset in a run that has neither.
static KEY_JOBS: OnceLock<Sender<(Instant, KeyJob)>> = OnceLock::new();
/// A test run that shows a sample item off screen: its windows stay cloaked
/// from first to last. They are created, shown, painted and measured as in real use, and the
/// compositor draws none of them on the desktop. Set once, before any window exists. Never set in
/// a real run: a price check is always there to be seen.
static UNSEEN: AtomicBool = AtomicBool::new(false);
/// The settings the player pinned: what a new check starts from. Changed on the main thread only.
static CONFIG: Mutex<config::Config> = Mutex::new(config::Config::NONE);
/// The thread that writes them to their file. Unset in a run that keeps them in memory only.
static SAVER: OnceLock<config::Saver> = OnceLock::new();
/// The chip whose menu is open, for the show in progress.
static MENU: Mutex<Option<check::Chip>> = Mutex::new(None);
/// The slider that is held: the button went down on its track and is not released yet.
static SLIDER: Mutex<Option<slider::Drag>> = Mutex::new(None);
/// Where the sliders were in the last frame: for the self-test.
static TRACKS: Mutex<Vec<panel::Track>> = Mutex::new(Vec::new());
/// Since when a slider is held although Windows says the button is up: see `button_is_down`.
static RELEASE_MISSED: Mutex<Option<Instant>> = Mutex::new(None);
/// How long a slider stays held with the button up before its release is taken as missed, and
/// how often that is looked at while one is held.
const RELEASE_GRACE: Duration = Duration::from_millis(150);
const RELEASE_WATCH: Duration = Duration::from_millis(50);
/// Where the pins, the chips that have a menu and the open menu were in the frame last painted,
/// for the self-test to aim at.
static PIN_ZONES: Mutex<Vec<panel::PinZone>> = Mutex::new(Vec::new());
static CHIP_ZONES: Mutex<Vec<panel::MenuZone>> = Mutex::new(Vec::new());
static MENU_AT: Mutex<Option<egui::Rect>> = Mutex::new(None);
/// The file the pinned settings are saved in, when they are saved.
static CONFIG_FILE: OnceLock<std::path::PathBuf> = OnceLock::new();

/// The time between two frames while the bars pulse: about 30 a second. Nothing else in the
/// panel moves, so nothing else asks for a frame after a delay.
const PULSE_FRAME: Duration = Duration::from_millis(33);

struct Pending {
    seq: u64,
    t0: Instant,
    fg_before: isize,
    shown_ms: f64,
    logic_ms: Option<f64>,
    built_ms: Option<f64>,
}

macro_rules! log {
    ($($arg:tt)*) => {{
        let t = $crate::START.get().map_or(0.0, |s| s.elapsed().as_secs_f64());
        let line = format!("[{t:9.3}] {}", format!($($arg)*));
        println!("{line}");
        $crate::log_to_file(&line);
    }};
}

static LOG_FILE: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();

/// Appends a line to %APPDATA%\Zenith Trade\zenith.log (the previous run's file becomes
/// zenith.old.log). The release exe has no console, so this file is where its output goes.
/// Any failure is ignored: logging must never stop the program.
fn log_to_file(line: &str) {
    use std::io::Write;
    let file = LOG_FILE.get_or_init(|| {
        let dir = config::data_dir();
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join("zenith.log");
        let _ = std::fs::rename(&path, dir.join("zenith.old.log"));
        std::fs::File::create(&path).ok().map(Mutex::new)
    });
    if let Some(Ok(mut f)) = file.as_ref().map(|m| m.lock()) {
        let _ = writeln!(f, "{line}");
    }
}

// After the macro, which they use.
// Some of its items serve only the developer runs: see the `dev` feature.
#[cfg_attr(not(feature = "dev"), allow(dead_code))]
mod check;
// Some of its items serve only the developer runs: see the `dev` feature.
#[cfg_attr(not(feature = "dev"), allow(dead_code))]
mod config;
mod look;
mod settings;
mod tray;
mod update;
mod copy;
mod field;
mod hotkeys;
// Some of its items serve only the developer runs: see the `dev` feature.
#[cfg_attr(not(feature = "dev"), allow(dead_code))]
mod maps;
mod keys;
mod launcher;
// Some of its items serve only the developer runs: see the `dev` feature.
#[cfg_attr(not(feature = "dev"), allow(dead_code))]
mod panel;
// Some of its items serve only the developer runs: see the `dev` feature.
#[cfg_attr(not(feature = "dev"), allow(dead_code))]
mod quote;
mod rebind;
// Some of its items serve only the developer runs: see the `dev` feature.
#[cfg_attr(not(feature = "dev"), allow(dead_code))]
mod slider;
mod stash;
mod stashsearch;
mod state;
// Some of its items serve only the developer runs: see the `dev` feature.
#[cfg_attr(not(feature = "dev"), allow(dead_code))]
mod worker;
#[cfg(feature = "dev")]
mod dev;
/// A user build has no developer run: each of these is always no.
#[cfg(not(feature = "dev"))]
mod dev {
    pub fn demo() -> bool {
        false
    }
    pub fn zoom() -> Option<f32> {
        None
    }
    pub fn scripted() -> bool {
        false
    }
    pub fn focus_testing() -> bool {
        false
    }
    pub fn keys_while_hidden() -> bool {
        false
    }
}

/// "working set 12.3 MB, private 45.6 MB" for this process.
pub(crate) fn memory() -> String {
    let mut c = PROCESS_MEMORY_COUNTERS_EX::default();
    let size = size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    unsafe {
        let _ = GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS,
            size,
        );
    }
    let mb = |b: usize| b as f64 / (1024.0 * 1024.0);
    format!("working set {:.1} MB, private {:.1} MB", mb(c.WorkingSetSize), mb(c.PrivateUsage))
}

fn panel() -> HWND {
    HWND(PANEL.load(SeqCst) as *mut c_void)
}

fn foreground() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
}

/// "0x1234 'Window title'". Reading a title does not send the other window any message.
fn describe(hwnd: isize) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetWindowTextW(HWND(hwnd as *mut c_void), &mut buf) }.max(0) as usize;
    format!("{hwnd:#x} '{}'", String::from_utf16_lossy(&buf[..n]))
}

// ---- window styles ------------------------------------------------------------------------

const WANT_STYLE: u32 = WS_POPUP.0 | WS_CLIPSIBLINGS.0 | WS_CLIPCHILDREN.0;
const WANT_EX: u32 = WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0;

/// winit gives an undecorated window caption styles (and rewrites them whenever one of its own
/// window flags changes). Put ours back if they are gone. Returns true when it had to.
fn ensure_styles(hwnd: HWND) -> bool {
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let style_ok = style & !WS_VISIBLE.0 == WANT_STYLE;
        let ex_ok = ex == WANT_EX;
        if style_ok && ex_ok {
            return false;
        }
        SetWindowLongPtrW(hwnd, GWL_STYLE, (WANT_STYLE | (style & WS_VISIBLE.0)) as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, WANT_EX as isize);
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        log!("styles were style={style:#x} ex={ex:#x}; set to style={WANT_STYLE:#x} ex={WANT_EX:#x}");
        true
    }
}

fn cloak(hwnd: HWND, on: bool) {
    let value: i32 = on as i32;
    unsafe {
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_CLOAK, &value as *const i32 as *const c_void, 4);
    }
}

/// Sits in front of winit's window procedure.
unsafe extern "system" fn subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        // A click on the panel must not activate it (WS_EX_NOACTIVATE alone is not absolute).
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        // The client area is the whole window. winit would shift it down by one pixel to get
        // a DWM shadow on undecorated windows, which shows as a line on a transparent one.
        WM_NCCALCSIZE if wparam.0 != 0 => LRESULT(0),
        _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
    }
}

// ---- a left click outside the panel hides it ----------------------------------------------
//
// A low-level mouse hook, installed while the panel is visible and removed when it hides, so
// nothing is hooked while hidden. The hook only watches: it always passes the click on.
//
// When the stash scroll is on, the same hook also hands the turns of the wheel to the stash
// thread (see further down), and that needs it for as long as the program runs: it is then
// installed once and stays. The clicks are watched exactly as before all the same, only while
// the panel is visible: hidden, a click is not even looked at.
//
// It lives on a thread of its own, not on the UI thread. Windows runs a low-level hook on the
// thread that installed it and holds every mouse event of the whole system until that thread
// answers. The UI thread is busy for the length of a frame whenever it paints; this thread does
// nothing but wait for messages, and never writes to the console (a console with a selection
// in progress blocks writers), so the mouse is never held up.

/// To the hook thread: install, remove.
const WM_HOOK_ON: u32 = WM_APP + 0x3A0;
const WM_HOOK_OFF: u32 = WM_APP + 0x3A1;
/// To the main thread: the hook watches for clicks (wParam 1), no longer does (0), could not be
/// installed (2, lParam the error code), or watches the wheel (3).
const WM_HOOK_STATE: u32 = WM_APP + 0x3A2;
/// To the hook thread: watch the wheel from now on, for the stash scroll.
const WM_HOOK_WHEEL: u32 = WM_APP + 0x3A4;
/// To the stash thread: the wheel turned. wParam is how far, as Windows counts it (120 a notch,
/// below zero toward the user), lParam the point, packed like the one of a click.
const WM_WHEEL_TURNED: u32 = WM_APP + 0x3A5;
/// To the main thread: a left click landed outside the panel. wParam is the number of the show
/// it belongs to, lParam the point packed like WM_NCHITTEST packs one.
const WM_OUTSIDE_CLICK: u32 = WM_APP + 0x3A3;
/// To the main thread: the hotkeys were changed in the Settings window.
const WM_KEYS_CHANGED: u32 = WM_APP + 0x3A6;
/// To the main thread: the stash scroll was turned on in the Settings window.
const WM_STASH_ON: u32 = WM_APP + 0x3A7;

/// The whole decision. Both are screen coordinates in physical pixels; right and bottom are
/// exclusive, as in every Win32 rectangle.
fn is_outside(pt: POINT, rect: RECT) -> bool {
    pt.x < rect.left || pt.x >= rect.right || pt.y < rect.top || pt.y >= rect.bottom
}

fn pack_point(pt: POINT) -> LPARAM {
    LPARAM((pt.x as u16 as isize) | (pt.y as u16 as isize) << 16)
}

fn unpack_point(l: LPARAM) -> (i32, i32) {
    (l.0 as u16 as i16 as i32, (l.0 >> 16) as u16 as i16 as i32)
}

/// Runs on the hook thread for every mouse event of the system while the hook is installed.
/// Keep it tiny: no logging, no locks, no window messages. A left click is looked at only while
/// the panel is visible, a turn of the wheel only when the stash scroll is on; a turn is not
/// counted here, only handed over, but whether it is ours is: only then is it swallowed.
unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        match wparam.0 as u32 {
            WM_LBUTTONDOWN if HOOKED.load(SeqCst) => {
                let pt = unsafe { (*(lparam.0 as *const MSLLHOOKSTRUCT)).pt };
                let mut wr = RECT::default();
                if unsafe { GetWindowRect(panel(), &mut wr) }.is_ok() && is_outside(pt, wr) {
                    OUTSIDE_CLICKS.fetch_add(1, SeqCst);
                    // The self-tests count real clicks but do not let them hide the panel under them.
                    if !dev::scripted() && !dev::focus_testing() {
                        unsafe {
                            let _ = PostThreadMessageW(
                                MAIN_THREAD.load(SeqCst),
                                WM_OUTSIDE_CLICK,
                                WPARAM(SHOWS.load(SeqCst) as usize),
                                pack_point(pt),
                            );
                        }
                    }
                }
            }
            WM_MOUSEWHEEL if WHEEL_WATCHED.load(SeqCst) => {
                let event = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
                // A wheel a hand turned, not one another program made up: only the first is
                // something the user did.
                if event.flags & LLMHF_INJECTED == 0 && wheel_is_ours(event.pt) {
                    let delta = stash::wheel_delta(event.mouseData);
                    unsafe {
                        let _ = PostThreadMessageW(
                            STASH_THREAD.load(SeqCst),
                            WM_WHEEL_TURNED,
                            WPARAM(stash::delta_to_message(delta)),
                            pack_point(event.pt),
                        );
                    }
                    // Ours: the game must not also scroll or zoom with it.
                    return LRESULT(1);
                }
            }
            _ => {}
        }
    }
    // Anything that is not the wheel over the inventory panel with Ctrl goes on to whatever is under it.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Owns the hook. Sleeps in GetMessageW; the hook callback is delivered from inside that call.
fn hook_thread(ready: std::sync::mpsc::Sender<u32>) {
    unsafe {
        let mut msg = MSG::default();
        // A thread has no message queue until it asks for one.
        let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
        let _ = ready.send(GetCurrentThreadId());
        let report = |state: usize, detail: isize| {
            let _ = PostThreadMessageW(MAIN_THREAD.load(SeqCst), WM_HOOK_STATE, WPARAM(state), LPARAM(detail));
        };
        let mut hook: Option<HHOOK> = None;
        // The hook is there while somebody wants it: the clicks while the panel is visible, the
        // wheel for the rest of the run once it is asked for. Returns whether it is there now.
        let ensure = |hook: &mut Option<HHOOK>| {
            if hook.is_none() {
                let module = GetModuleHandleW(None).ok().map(Into::into);
                match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), module, 0) {
                    Ok(h) => *hook = Some(h),
                    Err(e) => report(2, e.code().0 as isize),
                }
            }
            hook.is_some()
        };
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let asked = msg.message;
            if asked == WM_HOOK_ON && !HOOKED.load(SeqCst) && ensure(&mut hook) {
                HOOKED.store(true, SeqCst);
                report(1, 0);
            } else if asked == WM_HOOK_OFF && HOOKED.load(SeqCst) {
                HOOKED.store(false, SeqCst);
                // Without the stash scroll nothing else wants the hook: it goes, as ever.
                if !WHEEL_WATCHED.load(SeqCst)
                    && let Some(h) = hook.take()
                {
                    let _ = UnhookWindowsHookEx(h);
                }
                report(0, 0);
            } else if asked == WM_HOOK_WHEEL && !WHEEL_WATCHED.load(SeqCst) && ensure(&mut hook) {
                WHEEL_WATCHED.store(true, SeqCst);
                report(3, 0);
            }
        }
    }
}

/// Asks the hook thread to install (panel shown) or remove (panel hidden) the mouse hook.
fn set_mouse_hook(on: bool) {
    let message = if on { WM_HOOK_ON } else { WM_HOOK_OFF };
    if let Err(e) = unsafe { PostThreadMessageW(HOOK_THREAD.load(SeqCst), message, WPARAM(0), LPARAM(0)) } {
        log!("could not reach the mouse hook thread: {e}");
    }
}

/// The keys now. They change while the program runs, when the Settings window gives others.
pub fn current_hotkeys() -> Option<hotkeys::Hotkeys> {
    HOTKEYS.read().ok().and_then(|keys| keys.clone())
}

/// Any thread: these are the keys from now on. They are saved, and the main thread registers
/// them in place of the old ones.
pub fn set_hotkeys(keys: hotkeys::Hotkeys) {
    if let Some(saver) = SAVER.get() {
        saver.save_hotkeys(keys.to_value());
    }
    if let Ok(mut now) = HOTKEYS.write() {
        *now = Some(keys);
    }
    unsafe {
        let _ = PostThreadMessageW(MAIN_THREAD.load(SeqCst), WM_KEYS_CHANGED, WPARAM(0), LPARAM(0));
    }
}

/// Main thread: the keys were changed: nothing stays registered under the old ones.
fn on_keys_changed() {
    if let Ok(mut registered) = REGISTERED.lock() {
        for id in registered.drain(..) {
            unsafe {
                let _ = UnregisterHotKey(None, id);
            }
        }
    }
    REFUSED.lock().unwrap().clear();
    *FRONT.lock().unwrap() = None;
    sync_hotkeys();
}

/// Any thread: the stash scroll was turned on while the program runs.
pub fn stash_scroll_on() {
    STASH_SCROLL.store(true, SeqCst);
    unsafe {
        let _ = PostThreadMessageW(MAIN_THREAD.load(SeqCst), WM_STASH_ON, WPARAM(0), LPARAM(0));
    }
}

/// Main thread: starts what the stash scroll needs, when the run began without it.
fn on_stash_on() {
    if STASH_THREAD.load(SeqCst) != 0 || dev::demo() {
        return;
    }
    let (ready, thread_id) = std::sync::mpsc::channel();
    if std::thread::Builder::new().name("stash-scroll".into()).spawn(move || stash_thread(ready)).is_err() {
        log!("stash scroll: could not start its thread");
        return;
    }
    let Ok(id) = thread_id.recv() else {
        return;
    };
    STASH_THREAD.store(id, SeqCst);
    if let Err(e) = unsafe { PostThreadMessageW(HOOK_THREAD.load(SeqCst), WM_HOOK_WHEEL, WPARAM(0), LPARAM(0)) } {
        log!("stash scroll: could not reach the mouse hook thread: {e}");
    }
}

/// Main thread: what the hook thread reported.
fn on_hook_state(state: usize, detail: isize) {
    // With the stash scroll the hook is there for the whole run: only what it watches changes.
    let stays = STASH_SCROLL.load(SeqCst);
    match state {
        1 if stays => log!("mouse hook: a left click outside the panel hides it"),
        1 => log!("mouse hook installed (a left click outside the panel hides it)"),
        0 if stays => log!("mouse hook: clicks are no longer watched"),
        0 => log!("mouse hook removed"),
        3 => log!("mouse hook installed for the stash scroll: it stays until the program quits"),
        _ => log!("could not install the mouse hook: error {:#x}", detail as u32),
    }
}

// ---- Ctrl and the wheel change stash tab ----------------------------------------------------
//
// The hook only says that the wheel turned, how far and where. Everything else is done here, on
// a thread of its own that sleeps until then: is the report fresh, is Ctrl held, is the game
// the foreground window, where is its client area, is the pointer over the stash or over the
// panel. `stash` decides, and `copy` sends the one key. The hook thread never waits for any of
// it, so the mouse is never held up, and neither is a frame of the panel.

/// The client area of a window, in screen coordinates. Asks the window manager, not the window:
/// the game is sent no message.
fn client_rect(window: HWND) -> Option<RECT> {
    let (mut client, mut origin) = (RECT::default(), POINT::default());
    unsafe {
        GetClientRect(window, &mut client).ok()?;
        if !ClientToScreen(window, &mut origin).as_bool() {
            return None;
        }
    }
    Some(RECT { left: origin.x, top: origin.y, right: origin.x + client.right, bottom: origin.y + client.bottom })
}

/// Whether a point of the screen is over the panel while it is up.
fn over_panel(pt: POINT) -> bool {
    let mut wr = RECT::default();
    VISIBLE.load(SeqCst) && unsafe { GetWindowRect(panel(), &mut wr) }.is_ok() && !is_outside(pt, wr)
}

/// Hook thread: whether a turn of the wheel at `pt` is the stash scroll's: Ctrl held, the game in
/// front, the pointer over its inventory panel and not over our own. Cheap reads only, no waiting.
fn wheel_is_ours(pt: POINT) -> bool {
    if !STASH_SCROLL.load(SeqCst) || !copy::ctrl_is_held() {
        return false;
    }
    let Some(client) = copy::game_window().and_then(client_rect) else {
        return false;
    };
    stash::claims(true, true, over_panel(pt), stash::in_inventory_area(pt, client))
}

/// Stash thread: the wheel turned by `delta` at `at`, as the hook reported it at `reported` on
/// the clock of Windows. Sends one arrow key to the game when `stash` says this turn is one.
fn on_wheel_turned(wheel: &mut stash::Wheel, delta: i32, at: (i32, i32), reported: u32) {
    // A report that waited (this thread was held up) is no key any more, and nothing is owed.
    if stash::stale(reported, unsafe { GetTickCount() }) {
        *wheel = stash::Wheel::default();
        return;
    }
    // Turned off in the Settings window: the hook still hands over the wheel, and it is ignored.
    if !STASH_SCROLL.load(SeqCst) {
        return;
    }
    let pt = POINT { x: at.0, y: at.1 };
    // Most turns of the wheel are without Ctrl: nothing more is asked for them.
    let ctrl = copy::ctrl_is_held();
    let client = if ctrl { copy::game_window().and_then(client_rect) } else { None };
    // A game whose client area cannot be read is as good as not there.
    let (on_panel, on_stash) = client.map_or((false, false), |client| (over_panel(pt), stash::in_inventory_area(pt, client)));
    let Some(key) = wheel.turn(delta, ctrl, client.is_some(), on_panel, on_stash) else {
        return;
    };
    match copy::press_stash_key(key) {
        Ok(()) => {
            let sent = STASH_KEYS.fetch_add(1, SeqCst) + 1;
            log!("stash scroll: {key:?} arrow sent to the game for a notch of the wheel at {},{} (key {sent} of this run)", pt.x, pt.y);
        }
        Err(why) => log!("stash scroll: no key sent for a notch of the wheel: {why:?}"),
    }
}

/// Owns the count of the wheel. Sleeps in GetMessageW until the hook reports a turn.
fn stash_thread(ready: std::sync::mpsc::Sender<u32>) {
    unsafe {
        let mut msg = MSG::default();
        // A thread has no message queue until it asks for one.
        let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
        let _ = ready.send(GetCurrentThreadId());
        let mut wheel = stash::Wheel::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            if msg.message == WM_WHEEL_TURNED {
                on_wheel_turned(&mut wheel, stash::delta_from_message(msg.wParam.0), unpack_point(msg.lParam), msg.time);
            }
        }
    }
}

/// Main thread: a left click outside the panel, seen by the hook during show number `seq`.
/// Hides through the same function as Esc.
fn on_outside_click(seq: u64, at: (i32, i32)) {
    // A click still in the queue from an earlier show must not hide this one.
    if VISIBLE.load(SeqCst) && seq == SHOWS.load(SeqCst) {
        hide(&format!("left click outside the panel, at {},{}", at.0, at.1));
    }
}

// ---- show / hide --------------------------------------------------------------------------

/// The clock the ages of the listings are counted from: the real one, or the fake item's.
fn view_clock() -> i64 {
    if dev::demo() {
        state::parse_utc(state::DEMO_NOW).unwrap_or(0)
    } else {
        state::now_unix()
    }
}

/// The window's size in physical pixels for the view to show: as wide as it is, as tall as the
/// plate and its shadow. The scale is read off the window's width, as in `place_sized`.
fn window_size(hwnd: HWND, view: &View) -> (i32, i32) {
    let mut wr = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut wr);
    }
    let (w, h) = (wr.right - wr.left, wr.bottom - wr.top);
    let Some(ctx) = CTX.get() else {
        return (w, h);
    };
    let height = panel::measure(ctx, view, view_clock()) + panel::SHADOW;
    (w, (height * w as f32 / panel::WIDTH - 0.01).ceil() as i32)
}

/// Makes `view` the one the panel draws. Returns the size the window must have for it.
fn set_view(view: View) -> (i32, i32) {
    let size = window_size(panel(), &view);
    *VIEW.lock().unwrap() = view;
    size
}

/// A new view while the panel is up: the window grows or shrinks with it, in place, and moves
/// up only when it would leave the bottom of the work area. Never activates.
fn refit(view: View) {
    let hwnd = panel();
    let (w, h) = set_view(view);
    unsafe {
        let mut wr = RECT::default();
        let _ = GetWindowRect(hwnd, &mut wr);
        if (wr.right - wr.left, wr.bottom - wr.top) == (w, h) {
            return;
        }
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let corner = POINT { x: wr.left, y: wr.top };
        let _ = GetMonitorInfoW(MonitorFromPoint(corner, MONITOR_DEFAULTTONEAREST), &mut mi);
        let top = wr.top.min(mi.rcWork.bottom - h).max(mi.rcWork.top);
        let _ = SetWindowPos(hwnd, None, wr.left, top, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

/// Where a panel of `w` by `h` physical pixels goes: 16px (scaled) to the right of the cursor,
/// inside the work area of the monitor under the cursor. Falls to the left of the cursor when the
/// right has no room.
fn place_sized(w: i32, h: i32) -> RECT {
    unsafe {
        let mut cur = POINT::default();
        let _ = GetCursorPos(&mut cur);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(MonitorFromPoint(cur, MONITOR_DEFAULTTONEAREST), &mut mi);
        let work = mi.rcWork;
        let gap = (16.0 * w as f32 / panel::WIDTH).round() as i32;
        let mut x = cur.x + gap;
        if x + w > work.right {
            x = cur.x - gap - w;
        }
        let x = x.min(work.right - w).max(work.left);
        let y = (cur.y - h / 3).min(work.bottom - h).max(work.top);
        RECT { left: x, top: y, right: x + w, bottom: y + h }
    }
}

/// Shows the panel with `view`.
fn show(t0: Instant, view: View) {
    show_placed(t0, view, false);
}

/// Where the quick menu goes: top centre of the game's window, or of the screen under the cursor.
fn place_launcher(w: i32, h: i32) -> RECT {
    unsafe {
        let mut cur = POINT::default();
        let _ = GetCursorPos(&mut cur);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(MonitorFromPoint(cur, MONITOR_DEFAULTTONEAREST), &mut mi);
        let work = mi.rcWork;
        let mut game = RECT { left: work.left, top: work.top, right: work.right, bottom: work.bottom };
        if let Some(window) = copy::game_window() {
            let _ = GetWindowRect(window, &mut game);
        }
        let drop = (64.0 * w as f32 / panel::WIDTH).round() as i32;
        let (x, y) = launcher::anchor((game.left, game.top, game.right), (w, h), drop, (work.left, work.top, work.right, work.bottom));
        RECT { left: x, top: y, right: x + w, bottom: y + h }
    }
}

/// The quick menu, shown. The key of the Settings window comes here.
fn show_launcher(t0: Instant) {
    show_placed(t0, View::Launcher, true);
}

fn show_placed(t0: Instant, view: View, at_top: bool) {
    let hwnd = panel();
    let fg_before = foreground();
    // No field has the caret in a panel just shown, no menu is open, no slider is held, and
    // nothing is owed to any window.
    FIELDS.lock().unwrap().clear();
    *MENU.lock().unwrap() = None;
    *SLIDER.lock().unwrap() = None;
    RETURN_TO.store(0, SeqCst);
    let (w, h) = set_view(view);
    let at = if at_top { place_launcher(w, h) } else { place_sized(w, h) };
    ensure_styles(hwnd);
    // The frame painted while the window appears is the one to time: arm the stopwatch first.
    let seq = SHOWS.fetch_add(1, SeqCst) + 1;
    *PENDING.lock().unwrap() =
        Some(Pending { seq, t0, fg_before, shown_ms: 0.0, logic_ms: None, built_ms: None });
    VISIBLE.store(true, SeqCst);
    *SHOWN_AT.lock().unwrap() = Some(t0);
    unsafe {
        // One call that moves, sizes, raises and shows without activating. Moving first and
        // showing second costs 10 ms: eframe sleeps that long after any event on an invisible
        // window.
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), at.left, at.top, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        // Esc is ours only while the panel is up.
        if let Err(e) = RegisterHotKey(None, ID_ESC, MOD_NOREPEAT, VK_ESCAPE.0 as u32) {
            log!("could not register Esc: {e}");
        }
    }
    if let Some(p) = PENDING.lock().unwrap().as_mut() {
        p.shown_ms = t0.elapsed().as_secs_f64() * 1000.0;
    }
    set_mouse_hook(true);
    if let Some(ctx) = CTX.get() {
        ctx.request_repaint();
    }
}

fn hide(why: &str) {
    // Whatever check was running is abandoned, with what was being typed for it.
    CHECK.fetch_add(1, SeqCst);
    FIELDS.lock().unwrap().clear();
    *MENU.lock().unwrap() = None;
    *SLIDER.lock().unwrap() = None;
    // Before the window goes: Windows would otherwise pick the next foreground window itself.
    give_back();
    RETURN_TO.store(0, SeqCst);
    set_mouse_hook(false);
    unsafe {
        let _ = ShowWindow(panel(), SW_HIDE);
        let _ = UnregisterHotKey(None, ID_ESC);
    }
    VISIBLE.store(false, SeqCst);
    *SHOWN_AT.lock().unwrap() = None;
    *PENDING.lock().unwrap() = None;
    log!(
        "hidden ({why}); foreground {}; ui passes so far {}",
        describe(foreground()),
        UI_PASSES.load(SeqCst)
    );
    trim();
}

/// Unless --no-trim: hand the working set back to Windows while hidden. The pages come back on demand.
fn trim() {
    if TRIM.load(SeqCst) {
        unsafe {
            let _ = EmptyWorkingSet(GetCurrentProcess());
        }
    }
}

fn summary() {
    let mut t = TIMINGS.lock().unwrap().clone();
    t.sort_by(|a, b| a.total_cmp(b));
    if !t.is_empty() {
        log!(
            "hotkey to first frame presented over {} shows: min {:.1} ms, median {:.1} ms, max {:.1} ms, mean {:.1} ms",
            t.len(),
            t[0],
            (t[(t.len() - 1) / 2] + t[t.len() / 2]) / 2.0,
            t[t.len() - 1],
            t.iter().sum::<f64>() / t.len() as f64
        );
    }
    if STASH_SCROLL.load(SeqCst) {
        log!("stash scroll: {} arrow keys sent to the game in this run", STASH_KEYS.load(SeqCst));
    }
    log!(
        "shows {}, foreground unchanged in {} of them; ui passes {}, logic passes {}; {}",
        SHOWS.load(SeqCst),
        FOCUS_KEPT.load(SeqCst),
        UI_PASSES.load(SeqCst),
        LOGIC_PASSES.load(SeqCst),
        memory()
    );
}

fn quit() -> ! {
    // A pin changed a moment ago is on its way to the file: let it get there.
    if let Some(saver) = SAVER.get()
        && !saver.flush(Duration::from_secs(2))
    {
        log!("settings: the last change may not have been saved");
    }
    summary();
    tray::remove();
    unsafe {
        for id in REGISTERED.lock().unwrap().drain(..) {
            let _ = UnregisterHotKey(None, id);
        }
        let _ = UnregisterHotKey(None, ID_ESC);
        let _ = UnregisterHotKey(None, ID_QUIT);
    }
    log!("bye");
    std::process::exit(EXIT_CODE.load(SeqCst));
}

/// Called from winit's message loop for every WM_HOTKEY posted to the main thread.
fn on_hotkey(id: i32) {
    let t0 = Instant::now();
    match id {
        ID_QUIT => quit(),
        _ if !READY.load(SeqCst) => log!("hotkey ignored: still warming up"),
        ID_TOGGLE if VISIBLE.load(SeqCst) => hide("hotkey"),
        ID_TOGGLE => check(t0),
        ID_MAPS if VISIBLE.load(SeqCst) => hide("hotkey"),
        ID_MAPS => check_maps(t0),
        ID_ESC if VISIBLE.load(SeqCst) => escape(),
        ID_WIKI => key_job(t0, KeyJob::Wiki),
        ID_SETTINGS if VISIBLE.load(SeqCst) => hide("hotkey"),
        ID_SETTINGS => show_launcher(t0),
        id if id >= ID_SEARCH => {
            let search = current_hotkeys().and_then(|keys| keys.searches.get((id - ID_SEARCH) as usize).cloned());
            if let Some(search) = search {
                key_job(t0, KeyJob::Search(search));
            }
        }
        id if id >= ID_COMMAND => {
            let command = current_hotkeys().and_then(|keys| keys.commands.get((id - ID_COMMAND) as usize).cloned());
            if let Some(command) = command {
                key_job(t0, KeyJob::Command(command));
            }
        }
        _ => {}
    }
}

// ---- the keys come and go with the game -----------------------------------------------------
//
// A registered hotkey is taken from every program: while Ctrl+D is registered, no browser sees
// it. So the keys are registered only while the game is the foreground window, and unregistered
// as soon as another window is. Windows says when the foreground window changes (a WinEvent,
// delivered to the main thread between two messages), and `sync_hotkeys` then compares the keys
// that are registered with the ones `hotkeys::wanted` gives for the window now in front.
//
// One case needs care: a click in a number field makes the panel itself the foreground window,
// so that what is typed goes to the field. The game is then not in front, and yet the
// price-check key must still hide the panel. So that key, and that key alone, also stays while
// the panel is the foreground window; the keys that would send something to the game go, as it
// is not there to receive it. Hiding gives the foreground back to the game, and they return.

/// What the hotkey with this purpose is registered under.
fn hotkey_id(purpose: hotkeys::Purpose) -> i32 {
    match purpose {
        hotkeys::Purpose::PriceCheck => ID_TOGGLE,
        hotkeys::Purpose::Wiki => ID_WIKI,
        hotkeys::Purpose::Settings => ID_SETTINGS,
        hotkeys::Purpose::MapMods => ID_MAPS,
        hotkeys::Purpose::Command(index) => ID_COMMAND + index as i32,
        hotkeys::Purpose::Search(index) => ID_SEARCH + index as i32,
    }
}

/// Which window has the foreground, as far as the hotkeys care.
fn front() -> hotkeys::Front {
    let window = foreground();
    if window != 0 && window == panel().0 as isize {
        hotkeys::Front::Panel
    } else if copy::foreground_is_game() {
        hotkeys::Front::Game
    } else {
        hotkeys::Front::Other
    }
}

/// Main thread: registers the hotkeys the window in front calls for and unregisters the others.
/// A key Windows refuses (another program has it) is said once and tried again at the next
/// change: nothing here stops the program.
fn sync_hotkeys() {
    let Some(keys) = current_hotkeys() else {
        return;
    };
    // Without the watch nothing would ever bring a key back: they all stay, as they used to.
    let front = if FRONT_WATCHED.load(SeqCst) { front() } else { hotkeys::Front::Game };
    let wanted: Vec<(i32, hotkeys::Combo)> = hotkeys::wanted(&keys, front, GAME_HOTKEYS.load(SeqCst))
        .into_iter()
        .map(|(purpose, combo)| (hotkey_id(purpose), combo))
        .collect();
    // Windows only calls here between two messages of this thread, so this never runs inside
    // itself; should it ever, the call that is under way finishes the work.
    let Ok(mut registered) = REGISTERED.try_lock() else {
        return;
    };
    registered.retain(|id| {
        let stays = wanted.iter().any(|(wanted, _)| wanted == id);
        if !stays {
            unsafe {
                let _ = UnregisterHotKey(None, *id);
            }
        }
        stays
    });
    let mut refused = REFUSED.lock().unwrap();
    for (id, combo) in &wanted {
        if registered.contains(id) {
            continue;
        }
        let mut modifiers = MOD_NOREPEAT;
        for (held, modifier) in [(combo.ctrl, MOD_CONTROL), (combo.alt, MOD_ALT), (combo.shift, MOD_SHIFT)] {
            if held {
                modifiers |= modifier;
            }
        }
        match unsafe { RegisterHotKey(None, *id, modifiers, u32::from(combo.key)) } {
            Ok(()) => {
                registered.push(*id);
                refused.retain(|other| other != id);
            }
            Err(e) if !refused.contains(id) => {
                refused.push(*id);
                log!("could not register {} ({e}): another program has it. It does nothing here until that program lets go of it; the settings file can give it other keys", combo.label());
            }
            Err(_) => {}
        }
    }
    let mut last = FRONT.lock().unwrap();
    if *last != Some(front) {
        *last = Some(front);
        log!(
            "hotkeys: {}: {} registered",
            match front {
                hotkeys::Front::Game if FRONT_WATCHED.load(SeqCst) => "the game has the foreground",
                hotkeys::Front::Game => "the foreground window could not be watched, the keys stay for the whole run",
                hotkeys::Front::Panel => "the panel has the keyboard",
                hotkeys::Front::Other => "the game does not have the foreground",
            },
            match registered.len() {
                0 => "no key".to_owned(),
                1 => "1 key".to_owned(),
                n => format!("{n} keys"),
            }
        );
    }
}

/// Windows calls this on the main thread, between two messages, whenever another window becomes
/// the foreground window.
unsafe extern "system" fn foreground_changed(_hook: HWINEVENTHOOK, _event: u32, _window: HWND, _object: i32, _child: i32, _thread: u32, _time: u32) {
    sync_hotkeys();
}

// ---- a chat command, the wiki ----------------------------------------------------------------
//
// Both send keys to the game and wait a moment for it (the paste, the copy), so neither is done
// on the main thread: a thread of their own takes them one at a time. It never sends anything
// by itself: one job is one press of a key by the player.

/// What a key of the game asks for.
enum KeyJob {
    Command(hotkeys::Command),
    Search(stashsearch::Search),
    Wiki,
}

/// A job that waited longer than this is dropped: the player pressed the key for the game as it
/// was then.
const KEY_JOB_STALE: Duration = Duration::from_millis(1000);

/// Main thread: hands a job to the thread that sends. Never in a run that shows the fake item.
fn key_job(pressed: Instant, job: KeyJob) {
    if dev::demo() {
        return;
    }
    // The key is ours only while the game is in front: should it still be registered when it
    // is not, it goes now, and nothing is sent.
    if !copy::foreground_is_game() {
        sync_hotkeys();
        return;
    }
    if let Some(jobs) = KEY_JOBS.get()
        && jobs.send((pressed, job)).is_err()
    {
        log!("the thread that sends the game's keys is gone");
    }
}

/// Owns everything a key of the game makes the program send. Sleeps until one is pressed.
fn key_thread(jobs: Receiver<(Instant, KeyJob)>) {
    for (pressed, job) in jobs {
        if pressed.elapsed() > KEY_JOB_STALE {
            log!("a key of the game was left alone: it waited {:.0} ms", pressed.elapsed().as_secs_f64() * 1000.0);
            continue;
        }
        match job {
            KeyJob::Command(command) => {
                let how = if command.send { "sent" } else { "typed in the chat" };
                match copy::send_chat(&command.text, command.send) {
                    Ok(()) => log!("{}: {} {how}", command.keys.label(), command.text),
                    Err(why) => log!("{}: {} not {how}: {why:?}", command.keys.label(), command.text),
                }
            }
            KeyJob::Search(search) => {
                let keys = search.keys.map(|keys| keys.label()).unwrap_or_default();
                match copy::send_search(&search.text) {
                    Ok(()) => log!("{keys}: search {} put in the search box", search.title()),
                    Err(why) => log!("{keys}: search {} not put in the search box: {why:?}", search.title()),
                }
            }
            KeyJob::Wiki => wiki(),
        }
    }
}

/// The wiki key: copies the item under the cursor and opens its page in the default browser.
fn wiki() {
    let text = match copy::copy_hovered_item() {
        Ok(text) => text,
        Err(error) => {
            log!("wiki: nothing copied: {error:?}");
            return;
        }
    };
    let Ok(item) = zenith_core::parse_item(&text) else {
        log!("wiki: what was copied is not an item");
        return;
    };
    let pack = PACK.get().and_then(worker::pack_in);
    let name = keys::wiki_name(&item, pack.as_deref());
    match keys::wiki_url(&name) {
        Some(url) if keys::is_wiki_url(&url) => match shell_open(&url) {
            Ok(()) => log!("wiki: opened {url} for {name}"),
            Err(code) => log!("wiki: could not open {url}: code {code}"),
        },
        _ => log!("wiki: no page for {name:?}"),
    }
}

/// The hotkey while the panel is hidden: copy the item under the cursor, show what the text says
/// at once, and hand the search to the worker.
fn check(t0: Instant) {
    let seq = CHECK.fetch_add(1, SeqCst) + 1;
    #[cfg(feature = "dev")]
    if dev::demo() {
        dev::fake_check(seq, t0);
        return;
    }
    let text = match copy::copy_hovered_item() {
        Ok(text) => text,
        Err(copy::CopyError::GameNotForeground) => {
            // No panel over another program.
            log!("hotkey ignored: the game is not the foreground window ({})", describe(foreground()));
            return;
        }
        Err(error) => {
            log!("nothing copied: {error:?}");
            show(t0, View::Message(state::NOT_AN_ITEM.into()));
            return;
        }
    };
    // With the pack the worker has read, if any: no file is read here.
    let pack = PACK.get().and_then(worker::pack_in);
    let pins = *CONFIG.lock().unwrap();
    let view = state::read_pinned(&text, pack.as_deref(), &pins);
    let search = view.loading();
    log!("copied {} characters in {:.0} ms", text.chars().count(), t0.elapsed().as_secs_f64() * 1000.0);
    show(t0, view);
    if search
        && let Some(jobs) = JOBS.get()
        && jobs.send(worker::Job::Check { seq, text, pins }).is_err()
    {
        log!("the price-check thread is gone");
        refit(View::Message("The price check stopped working: restart Zenith Trade".into()));
    }
}

/// The map mods key while the panel is hidden: copy the item under the cursor and, when it is a
/// map, show its mods with the player's tags. Nothing is searched.
fn check_maps(t0: Instant) {
    CHECK.fetch_add(1, SeqCst);
    #[cfg(feature = "dev")]
    if dev::demo() {
        dev::fake_maps(t0);
        return;
    }
    match copy::copy_hovered_item() {
        Ok(text) => match maps::read_map(&text) {
            Some(view) => show(t0, view),
            None => show(t0, View::Message("This is not a map.".into())),
        },
        Err(copy::CopyError::GameNotForeground) => log!("hotkey ignored: the game is not the foreground window ({})", describe(foreground())),
        Err(error) => {
            log!("nothing copied: {error:?}");
            show(t0, View::Message(state::NOT_AN_ITEM.into()));
        }
    }
}

// ---- what the player does in the panel ------------------------------------------------------

/// How many searches may be sent now: what the worker last forecast, or the fake budget.
fn budget_now() -> Budget {
    #[cfg(feature = "dev")]
    if dev::demo() {
        return dev::fake_budget();
    }
    BUDGET
        .get()
        .and_then(|slot| slot.lock().ok().map(|forecast| forecast.budget(Instant::now())))
        .unwrap_or(Budget { left: 1, wait_s: 0 })
}

/// A click or a key in the panel: the check changes, and what follows from it is done here.
/// Nothing in here activates the panel or sends anything to the game.
fn act(action: Action, budget: Budget) {
    if matches!(action, Action::Launch(_) | Action::RunSearch(_) | Action::EditSearches) {
        act_menu(action);
        return;
    }
    let mut view = VIEW.lock().unwrap().clone();
    let effect = match &mut view {
        View::Item(check) => check::act(check, &action, budget),
        // A sentence has "Close" and nothing else.
        View::Maps(maps) => match action {
            Action::Close => Effect::Close,
            Action::MapTag(index, tag) => {
                maps.tag(index, tag);
                Effect::None
            }
            _ => Effect::None,
        },
        View::Message(_) if action == Action::Close => Effect::Close,
        View::Message(_) | View::Launcher | View::Searches(_) => Effect::None,
    };
    let stale = view.check().is_some_and(|check| check.stale);
    log!("{action:?}: {effect:?}{}; foreground {}", if stale { ", filters changed" } else { "" }, describe(foreground()));
    // A pinned setting that changed is pinned at its new value.
    if let Some(check) = view.check() {
        let followed = {
            let mut config = CONFIG.lock().unwrap();
            config.follow(&check.options).then_some(*config)
        };
        if let Some(config) = followed {
            log!("pinned settings follow: {config:?}");
            save_config(config);
        }
    }
    match effect {
        Effect::Close => hide("Close button"),
        Effect::Open(url) => open_trade_site(&url),
        Effect::None => refit(view),
        Effect::Search => {
            let request = view.check().and_then(check::request);
            refit(view);
            search_again(request);
        }
    }
    // The frame that saw the click painted the panel as it was.
    if let Some(ctx) = CTX.get() {
        ctx.request_repaint();
    }
}

/// A click in the quick menu or in the saved searches. Nothing here activates the panel: the game
/// keeps the foreground, so a search clicked goes to it exactly as its key sends it.
fn act_menu(action: Action) {
    log!("{action:?}; foreground {}", describe(foreground()));
    let open_settings = |page: Option<&str>| {
        if let Some(page) = page {
            settings::set_page(page);
        }
        settings::open();
        hide("quick menu");
    };
    match action {
        Action::Launch(index) => match launcher::target(index) {
            Some(launcher::Target::Searches) => {
                let titles = current_hotkeys().map(|keys| keys.searches.iter().map(|s| s.title().to_owned()).collect()).unwrap_or_default();
                refit(View::Searches(titles));
            }
            Some(launcher::Target::Settings(page)) => open_settings(page),
            None => {}
        },
        Action::EditSearches => open_settings(Some("Stash searches")),
        Action::RunSearch(index) => {
            let search = current_hotkeys().and_then(|keys| keys.searches.get(index).cloned());
            if let Some(search) = search {
                key_job(Instant::now(), KeyJob::Search(search));
            }
        }
        _ => {}
    }
    if let Some(ctx) = CTX.get() {
        ctx.request_repaint();
    }
}

// ---- the menus and the pins -------------------------------------------------------------------

/// Hands the pinned settings to the thread that writes them. Never waits for the disk.
fn save_config(config: config::Config) {
    if let Some(saver) = SAVER.get() {
        saver.save(config);
    }
}

/// A right click on a chip that has several values: its menu opens, in place of any other. What
/// was being typed in a field is committed first: while a menu is open, only its lines answer.
fn open_menu(chip: check::Chip) {
    leave_field("a menu opens");
    *MENU.lock().unwrap() = Some(chip);
    log!("menu of {chip:?} open; foreground {}", describe(foreground()));
    if let Some(ctx) = CTX.get() {
        ctx.request_repaint();
    }
}

/// Closes the menu that is open. Says whether one was.
fn close_menu(why: &str) -> bool {
    let Some(chip) = MENU.lock().unwrap().take() else {
        return false;
    };
    log!("menu of {chip:?} closed ({why})");
    if let Some(ctx) = CTX.get() {
        ctx.request_repaint();
    }
    true
}

/// A click on the pin of a setting: pinned at the value it has now, or no longer pinned. What is
/// pinned is saved at once, off this thread.
fn toggle_pin(setting: check::Setting) {
    let Some(options) = VIEW.lock().unwrap().check().map(|check| check.options) else {
        return;
    };
    let config = {
        let mut config = CONFIG.lock().unwrap();
        config.toggle(setting, &options);
        *config
    };
    log!("{setting:?} {}: {config:?}; foreground {}", if config.pinned(setting) { "pinned" } else { "no longer pinned" }, describe(foreground()));
    save_config(config);
    if let Some(ctx) = CTX.get() {
        ctx.request_repaint();
    }
}

// ---- the keyboard, borrowed for a number field ---------------------------------------------
//
// The panel never has the keyboard by being shown or clicked. A click in a number field is the
// one thing that takes it: the panel becomes the foreground window, so that what is typed goes
// to the field and not to the game. The window that had the foreground is remembered, once per
// show, and gets it back as soon as no field has the caret any more and whenever the panel
// hides, if that window still exists and the panel still has the foreground by then.
//
// Esc never comes as a key. It is a registered hotkey while the panel is up, and Windows takes
// a hotkey before any window sees the key, whoever has the keyboard: every Esc arrives as the
// thread message `on_hotkey` gets. `escape` is therefore where the order is decided: a menu that
// is open is closed first, then a field that has the caret is left, and only an Esc with neither
// hides the panel.

/// Whether a window belongs to this process.
fn ours(hwnd: isize) -> bool {
    let mut process = 0u32;
    unsafe {
        GetWindowThreadProcessId(HWND(hwnd as *mut c_void), Some(&mut process));
    }
    hwnd != 0 && process == unsafe { GetCurrentProcessId() }
}

/// Makes the panel the foreground window, so that keys reach it, and remembers who had the
/// foreground if nobody is remembered for this show. Says whether the panel has it now.
///
/// A run that shows the fake item (the self-tests, the captures, the timed runs) never takes the
/// foreground from another program: there it only goes from one window of this process to
/// another.
fn take_keyboard() -> bool {
    let hwnd = panel();
    let before = foreground();
    if before == hwnd.0 as isize {
        return true;
    }
    if dev::demo() && !ours(before) {
        log!("keyboard not taken: this run shows the fake item and {} has the foreground", describe(before));
        return false;
    }
    let _ = RETURN_TO.compare_exchange(0, before, SeqCst, SeqCst);
    unsafe {
        let _ = SetForegroundWindow(hwnd);
    }
    let got = foreground() == hwnd.0 as isize;
    if got {
        unsafe {
            let _ = SetFocus(Some(hwnd));
        }
    }
    log!(
        "keyboard taken from {}: {}",
        describe(before),
        if got { "yes, the panel is the foreground window".to_owned() } else { format!("NO, the foreground is {}", describe(foreground())) }
    );
    // Without waiting for Windows to say so: the price-check key stays, the game's other keys go.
    sync_hotkeys();
    got
}

/// Gives the foreground back to the window it was taken from in this show, if it was taken, if
/// the panel still has it, and if that window still exists.
fn give_back() {
    let to = RETURN_TO.load(SeqCst);
    if to == 0 {
        return;
    }
    let now = foreground();
    if now != panel().0 as isize {
        log!("keyboard not given back: {} has the foreground already", describe(now));
        return;
    }
    let window = HWND(to as *mut c_void);
    if !unsafe { IsWindow(Some(window)) }.as_bool() {
        log!("keyboard not given back: the window it was taken from ({to:#x}) is gone");
        return;
    }
    unsafe {
        let _ = SetForegroundWindow(window);
    }
    let now = foreground();
    log!("keyboard given back to {}: {}", describe(to), if now == to { "yes".to_owned() } else { format!("NO, the foreground is {}", describe(now)) });
    sync_hotkeys();
}

/// The caret is taken from the field that has it, and what the field holds is committed. Says
/// whether a field had it.
fn close_field(budget: Budget) -> bool {
    let commit = FIELDS.lock().unwrap().close();
    let Some(action) = commit else {
        return false;
    };
    act(action, budget);
    true
}

/// The field that has the caret is left for nothing else: what it holds is committed and the
/// keyboard goes back where it came from. Says whether a field had the caret.
fn leave_field(why: &str) -> bool {
    if !close_field(budget_now()) {
        return false;
    }
    log!("field left ({why})");
    give_back();
    true
}

/// No field has the caret any more, and nothing it held is committed: the check it was typed for
/// is no longer the one on screen.
fn drop_fields(why: &str) {
    let mut fields = FIELDS.lock().unwrap();
    let typing = fields.active().is_some();
    fields.clear();
    drop(fields);
    if typing {
        log!("field dropped ({why})");
        give_back();
    }
}

/// Esc: the open menu closes; without one, out of the field that has the caret; the panel stays
/// up in both cases. With neither, the panel hides.
fn escape() {
    if close_menu("Esc") {
        return;
    }
    if !leave_field("Esc") && VISIBLE.load(SeqCst) {
        hide("Esc");
    }
}

/// A left click in a number field: the panel takes the keyboard and the field gets the caret,
/// its content selected. A field that could not get the keyboard gets no caret either: what is
/// typed would go to the game.
fn focus_field(target: field::Target, budget: Budget) {
    let typing = FIELDS.lock().unwrap().active().map(field::Editor::target);
    if typing == Some(target) {
        // Already the one typed in: its content is selected again.
        on_key(field::Key::All, budget);
        return;
    }
    close_field(budget);
    // The self-test never takes the keyboard: its keys are fed to the panel's own input.
    if !dev::scripted() && !take_keyboard() {
        return;
    }
    let opened = {
        let view = VIEW.lock().unwrap();
        view.check().is_some_and(|check| FIELDS.lock().unwrap().open(check, target))
    };
    log!("{target:?}: {}; foreground {}", if opened { "has the caret" } else { "no field of a searched row" }, describe(foreground()));
    if !opened {
        give_back();
    }
    if let Some(ctx) = CTX.get() {
        ctx.request_repaint();
    }
}

/// A key while the panel has the keyboard: typed in the field that has the caret, or leaving it.
fn on_key(key: field::Key, budget: Budget) {
    // Esc as a key: only the self-tests send one, the real one being a hotkey. The same either way.
    if key == field::Key::Escape {
        escape();
        return;
    }
    let keyed = {
        let view = VIEW.lock().unwrap();
        view.check().map(|check| FIELDS.lock().unwrap().key(check, key)).unwrap_or_default()
    };
    for action in keyed.actions {
        act(action, budget);
    }
    if keyed.left {
        log!("field left ({key:?})");
        give_back();
    }
    if let Some(ctx) = CTX.get() {
        ctx.request_repaint();
    }
}

/// The wheel turned over a number field: its bound steps. The keyboard has no part in it.
fn turn_wheel(target: field::Target, lines: f32, budget: Budget) {
    let action = {
        let view = VIEW.lock().unwrap();
        view.check().and_then(|check| FIELDS.lock().unwrap().wheel(check, target, lines))
    };
    if let Some(action) = action {
        act(action, budget);
    }
}

/// Whether the primary mouse button is down right now, as Windows says. It is only read: nothing
/// is sent. A slider is held from a press to a release, and the release comes as a message of
/// the panel's window; should that message never come (the button let go while the pointer is
/// over another program and the panel no longer gets the mouse), this is how the release is
/// noticed all the same.
fn button_is_down() -> bool {
    unsafe {
        // The primary button is the right one for whoever swapped them.
        let button = if GetSystemMetrics(SM_SWAPBUTTON) != 0 { VK_RBUTTON } else { VK_LBUTTON };
        GetAsyncKeyState(i32::from(button.0)) < 0
    }
}

/// The keys of the fields in an event of the panel's input.
fn keys_of(event: &egui::Event) -> Vec<field::Key> {
    use field::Key;
    match event {
        egui::Event::Text(text) => text.chars().map(Key::Char).collect(),
        egui::Event::Key { key, pressed: true, modifiers, .. } => {
            let key = match key {
                egui::Key::Backspace => Some(Key::Backspace),
                egui::Key::Delete => Some(Key::Delete),
                egui::Key::ArrowLeft => Some(Key::Left),
                egui::Key::ArrowRight => Some(Key::Right),
                egui::Key::Home => Some(Key::Home),
                egui::Key::End => Some(Key::End),
                egui::Key::Tab if modifiers.shift => Some(Key::BackTab),
                egui::Key::Tab => Some(Key::Tab),
                egui::Key::Enter => Some(Key::Enter),
                egui::Key::Escape => Some(Key::Escape),
                egui::Key::A if modifiers.command => Some(Key::All),
                _ => None,
            };
            key.into_iter().collect()
        }
        _ => Vec::new(),
    }
}

/// "Search again": the request of the check as the player left it goes to the worker, for the
/// check on screen.
fn search_again(request: Option<serde_json::Value>) {
    let seq = CHECK.load(SeqCst);
    #[cfg(feature = "dev")]
    if dev::demo() {
        dev::fake_search(seq);
        return;
    }
    let sent = match (request, JOBS.get()) {
        (Some(request), Some(jobs)) => jobs.send(worker::Job::Search { seq, request }).is_ok(),
        _ => false,
    };
    if !sent {
        log!("the price-check thread is gone");
        refit(View::Message("The price check stopped working: restart Zenith Trade".into()));
    }
}

/// Opens the page of a search in the default browser. The system is handed that address and
/// nothing else: anything `check::is_trade_url` does not accept is refused, so no file and no
/// program can ever be started from here. Never in a run that shows the fake item.
fn open_trade_site(url: &str) {
    if !check::is_trade_url(url) {
        log!("not opened: not a search page of the trade site");
        return;
    }
    if dev::demo() {
        log!("not opened in a run that shows the fake item: {url}");
        return;
    }
    match shell_open(url) {
        Ok(()) => log!("opened {url}"),
        Err(code) => log!("could not open {url}: code {code}"),
    }
}

/// Hands an address to the system to open in the default browser, or says the code it refused
/// it with. Only ever an address the trade site's check or the wiki's has accepted: it is
/// refused here again otherwise, so that no file and no program can be started from here.
fn shell_open(url: &str) -> Result<(), isize> {
    if !check::is_trade_url(url) && !keys::is_wiki_url(url) {
        return Err(0);
    }
    let wide: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
    let result = unsafe {
        // The shell may hand the address to a handler that is started through COM, and asks for
        // this before the call. It stays set up for the calling thread, which lives as long as
        // the program; asked again at the next call, it answers that it is already.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        ShellExecuteW(None, w!("open"), PCWSTR(wide.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL)
    };
    // ShellExecute's own convention: anything above 32 is a success.
    if result.0 as isize > 32 { Ok(()) } else { Err(result.0 as isize) }
}

// ---- the egui side ------------------------------------------------------------------------

#[derive(PartialEq)]
enum Phase {
    Boot,
    WarmShown,
    WarmPainted,
    Ready,
}

struct App {
    phase: Phase,
    /// From the worker thread, or from the self-test in a run that shows the fake item.
    updates: Receiver<worker::Update>,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        LOGIC_PASSES.fetch_add(1, SeqCst);
        // The Settings window, a viewport of this app, lives as long as this is called.
        settings::show(ctx);
        let hwnd = panel();
        // What the worker sent since the last pass. Only the check on screen counts: anything
        // older, or sent while the panel was being hidden, is dropped.
        while let Ok(update) = self.updates.try_recv() {
            if VISIBLE.load(SeqCst) && update.seq == CHECK.load(SeqCst) {
                // What the player changed meanwhile stays: see `View::take`.
                let mut view = VIEW.lock().unwrap().clone();
                let filters = |view: &View| {
                    view.check().map(|check| check.filters.iter().map(|filter| filter.id.clone()).collect::<Vec<_>>())
                };
                let before = filters(&view);
                view.take(update.step);
                // Read differently, its fields are no longer the ones that were typed in, nor
                // its sliders the one that was held.
                if filters(&view) != before {
                    drop_fields("the item was read again");
                    *SLIDER.lock().unwrap() = None;
                }
                refit(view);
            }
        }
        match self.phase {
            // Warm-up: paint one frame while the window is cloaked (shown to Win32, not composited).
            // That compiles the shaders and fills the glyph atlas, and it lets eframe do its own
            // "show after the first frame", which resets the window styles, before we set ours.
            Phase::Boot => {
                cloak(hwnd, true);
                ensure_styles(hwnd); // tool window from the start: never a taskbar button
                unsafe {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }
                self.phase = Phase::WarmShown;
                ctx.request_repaint();
            }
            Phase::WarmShown => ctx.request_repaint(),
            Phase::WarmPainted => {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
                ensure_styles(hwnd);
                // A run that is not to be seen keeps the cloak of its warm-up to its end.
                if !UNSEEN.load(SeqCst) {
                    cloak(hwnd, false);
                }
                // The fonts are there now: give the hidden window the size of its view.
                let view = VIEW.lock().unwrap().clone();
                refit(view);
                self.phase = Phase::Ready;
                READY.store(true, SeqCst);
                log!(
                    "ready: panel {:#x} hidden, {:.2} px per point, {} ui passes during warm-up; {}; foreground {}",
                    hwnd.0 as isize,
                    ctx.pixels_per_point(),
                    UI_PASSES.load(SeqCst),
                    memory(),
                    describe(foreground())
                );
                trim();
            }
            Phase::Ready => {
                // The pass after a show's first painted pass starts once that frame was handed
                // to the compositor (SwapBuffers has returned).
                let mut pending = PENDING.lock().unwrap();
                if let Some(p) = pending.as_mut()
                    && p.logic_ms.is_none()
                {
                    p.logic_ms = Some(p.t0.elapsed().as_secs_f64() * 1000.0);
                }
                if let Some(p) = pending.as_ref()
                    && let Some(built) = p.built_ms
                {
                    let ms = p.t0.elapsed().as_secs_f64() * 1000.0;
                    let fg_after = foreground();
                    let kept = fg_after == p.fg_before && fg_after != hwnd.0 as isize;
                    if kept {
                        FOCUS_KEPT.fetch_add(1, SeqCst);
                    }
                    let mut timings = TIMINGS.lock().unwrap();
                    timings.push(ms);
                    let mean = timings.iter().sum::<f64>() / timings.len() as f64;
                    let mut wr = RECT::default();
                    unsafe {
                        let _ = GetWindowRect(hwnd, &mut wr);
                    }
                    log!(
                        "show #{}: window shown {:.1} ms, pass began {:.1} ms, ui built {:.1} ms, frame presented {:.1} ms (mean {:.1}); at {},{} {}x{}; foreground before {} after {} -> {}",
                        p.seq,
                        p.shown_ms,
                        p.logic_ms.unwrap_or(0.0),
                        built,
                        ms,
                        mean,
                        wr.left,
                        wr.top,
                        wr.right - wr.left,
                        wr.bottom - wr.top,
                        describe(p.fg_before),
                        describe(fg_after),
                        if kept { "unchanged" } else { "CHANGED" }
                    );
                    PRESENTED.store(p.seq, SeqCst);
                    *pending = None;
                }
            }
        }
    }

    #[cfg(feature = "dev")]
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        dev::feed_pending(raw_input);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        UI_PASSES.fetch_add(1, SeqCst);
        // Where the bars are in their pulse: counted from the show, so that the first frame has
        // them as they are drawn at rest, and still while hidden (the warm-up, `--render`).
        let beat = SHOWN_AT.lock().unwrap().map_or(0.0, |at| at.elapsed().as_secs_f32());
        let budget = budget_now();
        let up = self.phase == Phase::Ready && VISIBLE.load(SeqCst);
        // The keys first, so that this frame paints what they typed. They only come while the
        // panel has the keyboard, which nothing but a click in a number field gives it.
        if up || dev::keys_while_hidden() {
            let keys: Vec<field::Key> = ui.input(|input| input.events.iter().flat_map(keys_of).collect());
            for key in keys {
                on_key(key, budget);
            }
        }
        let menu = *MENU.lock().unwrap();
        let held = *SLIDER.lock().unwrap();
        let (shown, loading, searchable) = {
            let view = VIEW.lock().unwrap();
            let fields = FIELDS.lock().unwrap();
            let face = panel::Face { fields: &fields, pins: *CONFIG.lock().unwrap(), menu, drag: held };
            let searchable = view.check().is_some_and(|check| check.searchable());
            (panel::show(ui, &view, &face, view_clock(), beat, budget), view.loading(), searchable)
        };
        *ZONES.lock().unwrap() = shown.zones;
        *NUM_FIELDS.lock().unwrap() = shown.fields;
        *PIN_ZONES.lock().unwrap() = shown.pins;
        *CHIP_ZONES.lock().unwrap() = shown.chips;
        *MENU_AT.lock().unwrap() = shown.menu;
        *TRACKS.lock().unwrap() = shown.tracks;
        if up {
            // A menu whose chip the view no longer has is no menu.
            if menu.is_some() && shown.menu.is_none() {
                close_menu("its chip is gone");
            }
            // The press comes a frame or more before the click it starts: a field is left before
            // whatever the click does elsewhere, and gets the caret as the button goes down. A
            // press beside an open menu closes it, and the click it starts is nothing more: what
            // was under the pointer did not answer while the menu was open.
            match shown.press {
                Some(panel::Press::Field(target)) => focus_field(target, budget),
                Some(panel::Press::Menu) => {}
                Some(panel::Press::Elsewhere) if close_menu("a click elsewhere in the panel") => {}
                Some(panel::Press::Elsewhere) => drop(leave_field("a click elsewhere in the panel")),
                None => {}
            }
            match shown.context {
                Some(panel::Context::Open(chip)) => open_menu(chip),
                Some(panel::Context::Elsewhere) => drop(close_menu("a right click elsewhere in the panel")),
                None => {}
            }
            // A slider: held from the press on its track to the release, which alone changes the
            // check. The pointer's business from end to end: the keyboard has no part in it.
            if shown.drag != held {
                *SLIDER.lock().unwrap() = shown.drag;
                match (held, shown.drag) {
                    (None, Some(drag)) => log!("slider of filter {} held at {}; foreground {}", drag.filter(), drag.value(), describe(foreground())),
                    (Some(drag), None) if shown.slid.is_none() => log!("slider of filter {} let go: its row is no longer there", drag.filter()),
                    _ => {}
                }
                // The frame that saw the pointer painted the slider as it was.
                ui.ctx().request_repaint();
            }
            if let Some(action) = shown.slid {
                act(action, budget);
            }
            // Still held: the release is on its way, as a message of the window. Should it be
            // missed, the button is seen up here for longer than a message takes, and what is
            // held is released as it stands. Not in the self-test, whose presses are events
            // put into the panel's input with no button behind them.
            let still = *SLIDER.lock().unwrap();
            match still {
                Some(drag) if !dev::scripted() => {
                    let mut missed = RELEASE_MISSED.lock().unwrap();
                    if button_is_down() {
                        *missed = None;
                    } else if missed.get_or_insert_with(Instant::now).elapsed() > RELEASE_GRACE {
                        *missed = None;
                        drop(missed);
                        *SLIDER.lock().unwrap() = None;
                        log!("slider of filter {}: its release was not seen, released at {}", drag.filter(), drag.value());
                        act(drag.release(false), budget);
                    }
                    ui.ctx().request_repaint_after(RELEASE_WATCH);
                }
                _ => *RELEASE_MISSED.lock().unwrap() = None,
            }
            if let Some((target, lines)) = shown.wheel {
                turn_wheel(target, lines, budget);
            }
            if let Some(lines) = shown.any_wheel {
                act(Action::StepAny(panel::any_notches(lines)), budget);
            }
            for setting in shown.pinned {
                toggle_pin(setting);
            }
            // While a menu is open its lines are all there is to click: one picked, it closes.
            let picked = shown.menu.is_some() && !shown.actions.is_empty();
            for action in shown.actions {
                act(action, budget);
            }
            if picked {
                close_menu("a line was picked");
            }
            // A field whose row was switched off under it, and a panel that lost the keyboard some
            // other way (another window was brought forward), have no caret any more.
            let stray = {
                let view = VIEW.lock().unwrap();
                FIELDS.lock().unwrap().stray(view.check())
            };
            if stray {
                drop_fields("its row is no longer searched");
            }
            let typing = FIELDS.lock().unwrap().active().is_some();
            if typing && !dev::scripted() && foreground() != panel().0 as isize {
                leave_field("the panel no longer has the keyboard");
            }
        }
        // The only frames asked for ahead of time: the next one of the pulse, while the panel is
        // up and its view is loading, and the one where the request budget next changes, while
        // the panel shows it. Once the view has finished, the budget is whole or the panel is
        // hidden nothing asks any more, and the passes stop as before.
        if loading && up {
            ui.ctx().request_repaint_after(PULSE_FRAME);
        }
        if searchable
            && up
            && let Some(slot) = BUDGET.get()
            && let Some(wait) = slot.lock().ok().and_then(|forecast| forecast.next_change(Instant::now()))
        {
            ui.ctx().request_repaint_after(wait);
        }
        if self.phase == Phase::WarmShown {
            self.phase = Phase::WarmPainted;
            ui.ctx().request_repaint();
        }
        if let Some(p) = PENDING.lock().unwrap().as_mut()
            && p.built_ms.is_none()
        {
            p.built_ms = Some(p.t0.elapsed().as_secs_f64() * 1000.0);
            ui.ctx().request_repaint(); // one more pass, to learn when this one was presented
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
}

/// Hands the main thread a hotkey's number, as Windows does when the key is pressed.
fn post(id: i32) {
    unsafe {
        let _ = PostThreadMessageW(MAIN_THREAD.load(SeqCst), WM_HOTKEY, WPARAM(id as usize), LPARAM(0));
    }
}

// ---- main ---------------------------------------------------------------------------------

fn main() {
    let copied = config::migrate_old_data();
    std::panic::set_hook(Box::new(|info| {
        let line = format!("panic: {info}");
        eprintln!("{line}");
        log_to_file(&line);
    }));
    if copied > 0 {
        log_to_file(&format!("copied {copied} data file(s) from the old folder %LOCALAPPDATA%\\zenith to {}", config::data_dir().display()));
    }
    if let Err(error) = run() {
        let line = format!("zenith stopped: {error}");
        eprintln!("{line}");
        log_to_file(&line);
        std::process::exit(1);
    }
}

fn run() -> eframe::Result {
    // Velopack's own hooks (install, update, uninstall) run here and exit; it must come first.
    velopack::VelopackApp::build().run();
    START.set(Instant::now()).unwrap();
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The data update: no window, no hotkey, no input. Its summary is for a person to read, so
    // nothing else is written around it.
    if args.iter().any(|a| a == "--update-data") {
        std::process::exit(worker::update_data());
    }
    if let Some(at) = args.iter().position(|a| a == "--update-prices") {
        let league = args.get(at + 1).filter(|word| !word.starts_with("--")).cloned();
        std::process::exit(worker::update_prices(league));
    }
    log!("start: {}", memory());
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let key_flag = value("--key").and_then(|k| k.chars().next());
    TRIM.store(!args.iter().any(|a| a == "--no-trim"), SeqCst);
    // A developer run reads its own flags here (see `dev`); a user build has none.
    #[cfg(feature = "dev")]
    let dev_run = dev::Run::from_args(&args);
    let demo = dev::demo();

    // The pinned settings: read here, once, before there is a window; written by their own
    // thread only.
    #[cfg(feature = "dev")]
    let config_file = if demo { dev_run.config_file() } else { Some(config::path()) };
    #[cfg(not(feature = "dev"))]
    let config_file = Some(config::path());
    if let Some(path) = config_file {
        let config = config::Config::load(&path);
        log!("settings: {config:?}, kept in {}", path.display());
        *CONFIG.lock().unwrap() = config;
        look::set(config.theme, config.marker);
        match config::Saver::spawn(path.clone()) {
            Ok(saver) => drop(SAVER.set(saver)),
            Err(error) => log!("settings: pins will not be saved: {error}"),
        }
        let _ = CONFIG_FILE.set(path);
    }
    // The keys: the ones the same file gives, or the built-in ones; `--key` is the price check's
    // either way. A run that shows the fake item has no wiki key and no command key, and neither
    // has a run that was asked to have none.
    let (keys_of_run, odd) = {
        let text = CONFIG_FILE.get().and_then(|path| std::fs::read_to_string(path).ok());
        hotkeys::Hotkeys::from_file_text(text.as_deref(), key_flag)
    };
    for entry in odd {
        log!("hotkeys: {entry}");
    }
    let game_hotkeys = !demo && !args.iter().any(|a| a == "--no-game-hotkeys");
    GAME_HOTKEYS.store(game_hotkeys, SeqCst);
    log!(
        "hotkeys, while the game is the foreground window: {}; {} again, Esc or a left click outside the panel hides it; {} quits, anywhere",
        keys_of_run.summary(game_hotkeys),
        keys_of_run.price_check.label(),
        hotkeys::QUIT.label()
    );
    *HOTKEYS.write().unwrap() = Some(keys_of_run);
    #[cfg(feature = "dev")]
    dev_run.apply_look();
    // The icon in the notification area is for a run that serves the player, not for a test.
    let with_tray = !demo;
    // The stash scroll: on unless the flag or the settings say no, and never in a run that
    // shows the fake item, which sends nothing to the game. Decided here, once, before the
    // thread that would send its keys exists.
    let stash_scroll = stash::enabled(CONFIG.lock().unwrap().stash_scroll, args.iter().any(|a| a == "--no-stash-scroll"), demo);
    STASH_SCROLL.store(stash_scroll.is_ok(), SeqCst);
    log!(
        "stash scroll: {}",
        match stash_scroll {
            Ok(()) => "on: with Ctrl held over the inventory panel of the game, a notch of the wheel is one arrow key, the next or the previous tab",
            Err(stash::Off::Flag) => "off (--no-stash-scroll)",
            Err(stash::Off::Setting) => "off (\"stash_scroll\": false in the settings)",
            Err(stash::Off::FakeItem) => "off (this run shows the fake item and sends nothing to the game)",
        }
    );
    // The sample item is what the warm-up frame paints: it fills the glyph atlas.
    let first = state::demo_view();
    #[cfg(feature = "dev")]
    let first = dev::first_view(first);
    *VIEW.lock().unwrap() = first;

    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Zenith Trade")
            // The height is set for each view once the fonts can measure it.
            .with_inner_size([panel::WIDTH * dev::zoom().unwrap_or(1.0), 400.0 + panel::SHADOW])
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_always_on_top()
            .with_active(false)
            .with_visible(false)
            .with_taskbar(false)
            .with_drag_and_drop(false),
        dithering: false,
        #[cfg(feature = "wgpu")]
        renderer: eframe::Renderer::Wgpu,
        #[cfg(not(feature = "wgpu"))]
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    log!("renderer: {}", if cfg!(feature = "wgpu") { "wgpu" } else { "glow (OpenGL)" });
    // Hotkeys registered without a window arrive as thread messages: catch them in winit's loop.
    options.event_loop_builder = Some(Box::new(|builder| {
        use winit::platform::windows::EventLoopBuilderExtWindows as _;
        builder.with_msg_hook(|msg| {
            let msg = unsafe { &*(msg as *const MSG) };
            if !msg.hwnd.0.is_null() {
                return false;
            }
            match msg.message {
                WM_HOTKEY => on_hotkey(msg.wParam.0 as i32),
                // From the mouse hook thread.
                WM_OUTSIDE_CLICK => on_outside_click(msg.wParam.0 as u64, unpack_point(msg.lParam)),
                WM_HOOK_STATE => on_hook_state(msg.wParam.0, msg.lParam.0),
                WM_KEYS_CHANGED => on_keys_changed(),
                WM_STASH_ON => on_stash_on(),
                _ => return false,
            }
            true
        });
    }));

    eframe::run_native(
        "Zenith",
        options,
        Box::new(move |cc| {
            use winit::raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
            let RawWindowHandle::Win32(handle) = cc.window_handle()?.as_raw() else {
                return Err("not a Win32 window".into());
            };
            let hwnd = HWND(handle.hwnd.get() as *mut c_void);
            PANEL.store(hwnd.0 as isize, SeqCst);
            log!("window and graphics context created: {}", memory());
            MAIN_THREAD.store(unsafe { GetCurrentThreadId() }, SeqCst);
            let _ = CTX.set(cc.egui_ctx.clone());
            panel::install_fonts(&cc.egui_ctx);
            // Not the system theme: in light mode egui rasterises text for dark-on-light, which
            // makes pale text on this dark plate thin.
            cc.egui_ctx.set_theme(egui::Theme::Dark);
            // While a number field has the caret the panel has the keyboard: Ctrl with + or - must
            // not change the scale the whole panel is laid out at.
            cc.egui_ctx.options_mut(|options| options.zoom_with_keyboard = false);
            if let Some(zoom) = dev::zoom() {
                cc.egui_ctx.set_zoom_factor(zoom);
            }

            // The thread that will own the mouse hook while the panel is visible. It hooks nothing
            // until the first show.
            let (ready, thread_id) = std::sync::mpsc::channel();
            std::thread::Builder::new().name("mouse-hook".into()).spawn(move || hook_thread(ready))?;
            HOOK_THREAD.store(thread_id.recv()?, SeqCst);
            // The stash scroll: the thread that decides and sends first, then the hook, which
            // stays for the run and hands that thread the turns of the wheel.
            if STASH_SCROLL.load(SeqCst) {
                let (ready, thread_id) = std::sync::mpsc::channel();
                std::thread::Builder::new().name("stash-scroll".into()).spawn(move || stash_thread(ready))?;
                STASH_THREAD.store(thread_id.recv()?, SeqCst);
                if let Err(e) = unsafe { PostThreadMessageW(HOOK_THREAD.load(SeqCst), WM_HOOK_WHEEL, WPARAM(0), LPARAM(0)) } {
                    log!("stash scroll: could not reach the mouse hook thread: {e}");
                }
            }

            unsafe {
                let _ = SetWindowSubclass(hwnd, Some(subclass), 1, 0);
                // The key that quits is the program's own: it works whatever window is in front.
                let quit_keys: HOT_KEY_MODIFIERS = MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT;
                if let Err(e) = RegisterHotKey(None, ID_QUIT, quit_keys, u32::from(hotkeys::QUIT.key)) {
                    log!("could not register {}: {e}", hotkeys::QUIT.label());
                }
                // The other keys come and go with the game: Windows says here, on this thread,
                // whenever the foreground window changes. A key another program has is said in
                // the log when the game comes to the front, and the program runs without it.
                let watch = SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, Some(foreground_changed), 0, 0, WINEVENT_OUTOFCONTEXT);
                FRONT_WATCHED.store(!watch.is_invalid(), SeqCst);
                if watch.is_invalid() {
                    log!("the foreground window cannot be watched: the keys stay registered for the whole run, in every program");
                }
                sync_hotkeys();
                // The wheel steps a number field while the game keeps the keyboard only if Windows
                // sends it to the window under the pointer, which is its default.
                let mut routing = MOUSEWHEEL_ROUTING_MOUSE_POS;
                let asked = SystemParametersInfoW(
                    SPI_GETMOUSEWHEELROUTING,
                    0,
                    Some(&mut routing as *mut u32 as *mut c_void),
                    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
                );
                match (asked, routing) {
                    (Ok(()), MOUSEWHEEL_ROUTING_MOUSE_POS) => {
                        log!("the wheel goes to the window under the pointer: it steps a number field without the keyboard");
                    }
                    (Ok(()), _) => log!(
                        "the wheel goes to the window that has the keyboard (the Windows setting \"Scroll inactive windows when hovering over them\" is off, routing {routing}): over the panel it will not step a number field while the game has the keyboard"
                    ),
                    (Err(e), _) => log!("could not read where Windows sends the wheel: {e}"),
                }
            }

            // The thread that sends a chat command or opens the wiki, for the runs that have such
            // keys.
            if GAME_HOTKEYS.load(SeqCst) {
                let (jobs, waiting) = std::sync::mpsc::channel();
                std::thread::Builder::new().name("game-keys".into()).spawn(move || key_thread(waiting))?;
                let _ = KEY_JOBS.set(jobs);
            }

            if with_tray {
                tray::spawn();
                update::spawn();
            }
            // The thread that talks to the trade site. It reads the data pack and asks for the
            // league list right away.
            #[cfg(feature = "dev")]
            let fake = demo.then(dev::fake_updates);
            #[cfg(not(feature = "dev"))]
            let fake = None;
            let updates = match fake {
                Some(updates) => updates,
                None => {
                    let worker = worker::spawn(cc.egui_ctx.clone(), &CHECK)?;
                    let _ = JOBS.set(worker.jobs);
                    let _ = PACK.set(worker.pack);
                    let _ = BUDGET.set(worker.budget);
                    worker.updates
                }
            };

            #[cfg(feature = "dev")]
            dev_run.spawn()?;
            Ok(Box::new(App { phase: Phase::Boot, updates }))
        }),
    )
}
