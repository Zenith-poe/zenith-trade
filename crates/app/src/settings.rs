//! The Settings window: a second viewport of the one eframe app, a normal window that can take the
//! focus, so the panel, which never takes it, is not touched by it. (winit allows one event loop
//! per process: a window with an event loop of its own can never start.) The root window calls
//! `show` on every frame while the window is open, which is what keeps the viewport alive; the
//! root repaints on a timer meanwhile, because it is hidden most of the time. A choice is applied
//! at once and saved in the settings file. (A hidden root still runs its passes when asked to.)
//!
//! The window is top-most, created without activation, and given the keyboard by hand once it
//! exists. A new window that is not top-most lands in no set place against the game, which covers
//! the whole screen and keeps the foreground: it was often created behind it, and nothing was
//! seen. When it closes, the game, if it had the foreground when the window was asked for, gets it
//! back.

use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui::{self, Align, Color32, CornerRadius, FontFamily, FontId, Frame, Margin, Stroke, TextFormat, Vec2, epaint::text::VariationCoords, text::LayoutJob};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};
use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
use windows::core::w;

use zenith_core::matching::Slack;
use crate::config::{Config, League};
use crate::hotkeys::{Combo, QUIT};
use crate::look::{self, MARKERS, THEMES};
use crate::rebind::{Captured, Draft, DraftCommand, DraftSearch, capture};

static OPEN: AtomicBool = AtomicBool::new(false);
/// Asked for while the window is open: bring it to the front.
static FOCUS: AtomicBool = AtomicBool::new(false);
/// The window's state while it is open, with its size and where its top left goes.
static STATE: Mutex<Option<Open>> = Mutex::new(None);

type Open = (Window, [f32; 2], [f32; 2]);

/// Whether a pass of the root window has seen the window open since it was asked for.
static SEEN: AtomicBool = AtomicBool::new(false);
/// Frames the window painted since it was opened.
static FRAMES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// The game's window, when it had the foreground as the window was asked for: it gets it back.
static GAME: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

#[cfg(feature = "dev")]
#[path = "dev/settings_shot.rs"]
mod capture;
#[cfg(feature = "dev")]
pub use capture::{close_for_test, shoot_next, shot, window_exists};

/// The picture a developer run asked of the next window opened; never one in a user build.
fn next_shot() -> Option<String> {
    #[cfg(feature = "dev")]
    return capture::take_next();
    #[cfg(not(feature = "dev"))]
    None
}
/// Body text, in points: the price panel's rows are 13 to 14.
const BODY: f32 = 15.0;

/// Text in Manrope at an exact weight: the variable font's own default is its thinnest face, and a
/// plain label would come out as hairlines on a dark plate.
fn t(text: impl Into<String>, size: f32, weight: f32, color: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(
        &text.into(),
        0.0,
        TextFormat {
            font_id: FontId::new(size, FontFamily::Name("text-vf".into())),
            color,
            coords: VariationCoords::new([("wght", weight)]),
            ..Default::default()
        },
    );
    job
}

/// A label: the main text colour, medium weight.
fn label(text: &str) -> LayoutJob {
    t(text, BODY, 600.0, look::palette().star)
}

/// A hint or a state: the quiet colour, kept legible.
fn hint(text: &str) -> LayoutJob {
    t(text, 13.5, 500.0, look::palette().mute)
}

/// The name at the left of a row: left aligned, centred on the row's height.
fn cell(ui: &mut egui::Ui, text: &str) {
    ui.allocate_ui_with_layout(Vec2::new(140.0, 28.0), egui::Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_size(Vec2::new(140.0, 28.0));
        ui.label(label(text));
    });
}

/// A choice among several, as a button that is lit when chosen.
fn chip<T: PartialEq>(ui: &mut egui::Ui, value: &mut T, option: T, text: &str) {
    let p = look::palette();
    let on = *value == option;
    let job = t(text, BODY, 600.0, if on { p.night } else { p.star });
    if ui.add(egui::Button::new(job).selected(on).min_size(Vec2::new(0.0, 26.0))).clicked() {
        *value = option;
    }
}

/// A single line field, written at the body size.
fn field<'a>(text: &'a mut String, width: f32, hint_text: &str, color: Color32) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(text)
        .desired_width(width)
        .hint_text(hint(hint_text))
        .font(FontId::new(BODY, FontFamily::Name("text-vf".into())))
        .text_color(color)
        .margin(Margin::symmetric(8, 4))
}

const RED: Color32 = Color32::from_rgb(232, 96, 84);

/// The notice at the foot of the window.
const NOTICE: &str = "This product isn't affiliated with or endorsed by Grinding Gear Games in any way.";

const WIDTH: f32 = 820.0;
const HEIGHT: f32 = 940.0;

fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("zenith-settings")
}

/// Any thread: opens the window, or brings the one that is open to the front.
pub fn open() {
    {
        let mut state = STATE.lock().unwrap();
        let from = std::thread::current().name().unwrap_or("?").to_owned();
        log!("settings: open asked from the {from} thread (root logic passes so far {})", crate::LOGIC_PASSES.load(SeqCst));
        if state.is_none() {
            let game = unsafe { FindWindowW(None, w!("Path of Exile")) }.unwrap_or_default();
            let front = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
            GAME.store(if !game.0.is_null() && game == front { game.0 as isize } else { 0 }, SeqCst);
            let (size, position) = placement();
            *state = Some((Window::new(next_shot()), size, position));
            SEEN.store(false, SeqCst);
            FRAMES.store(0, SeqCst);
            OPEN.store(true, SeqCst);
        } else {
            FOCUS.store(true, SeqCst);
        }
    }
    if let Some(ctx) = crate::CTX.get() {
        ctx.request_repaint();
    }
}

/// Main thread, while the window still exists.
fn close() {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};
    let game = windows::Win32::Foundation::HWND(GAME.swap(0, SeqCst) as *mut core::ffi::c_void);
    let ours = hwnd();
    let mut gave = false;
    unsafe {
        if !game.0.is_null() && ours.is_some_and(|ours| ours == GetForegroundWindow()) {
            gave = SetForegroundWindow(game).as_bool();
        }
    }
    log!("settings closed{}", if gave { ", the game has the foreground again" } else { "" });
    *STATE.lock().unwrap() = None;
    OPEN.store(false, SeqCst);
    FOCUS.store(false, SeqCst);
    if let Some(ctx) = crate::CTX.get() {
        ctx.request_repaint();
    }
}

const TITLE: &str = "Zenith Trade \u{2014} Settings";

/// The window of Settings, while it exists.
fn hwnd() -> Option<windows::Win32::Foundation::HWND> {
    unsafe { FindWindowW(None, &windows::core::HSTRING::from(TITLE)).ok() }
}

/// Brings the window to the front, out of the taskbar if it was minimized, and gives it the
/// keyboard when Windows lets it.
fn forward() {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, HWND_TOPMOST, IsIconic, SW_RESTORE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetForegroundWindow, SetWindowPos,
        ShowWindow,
    };
    let Some(hwnd) = hwnd() else {
        log!("settings: no window to bring to the front");
        return;
    };
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST),0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
        let taken = SetForegroundWindow(hwnd).as_bool() && GetForegroundWindow() == hwnd;
        log!("settings: the window is in front, {}", if taken { "with the keyboard" } else { "Windows kept the keyboard where it was" });
    }
}

/// Says in the log how Windows sees the window of Settings.
fn describe_window() {
    use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
    use windows::Win32::UI::WindowsAndMessaging::{
        GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetForegroundWindow, GetWindow, GetWindowLongW, GetWindowRect, IsIconic, IsWindowVisible,
    };
    let Some(hwnd) = hwnd() else {
        log!("settings: no window titled {TITLE:?} exists");
        return;
    };
    unsafe {
        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut core::ffi::c_void, 4);
        let owner = GetWindow(hwnd, GW_OWNER).map_or(0, |owner| owner.0 as isize);
        log!(
            "settings: window {:#x} visible {} minimized {} at {},{} {}x{}, style {:#x} ex {:#x}, cloaked {cloaked}, owner {owner:#x}, foreground {:#x}",
            hwnd.0 as isize,
            IsWindowVisible(hwnd).as_bool(),
            IsIconic(hwnd).as_bool(),
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            GetWindowLongW(hwnd, GWL_STYLE),
            GetWindowLongW(hwnd, GWL_EXSTYLE),
            GetForegroundWindow().0 as isize
        );
        if let Ok(game) = FindWindowW(None, w!("Path of Exile")) {
            let mut above = false;
            let mut at = GetWindow(game, windows::Win32::UI::WindowsAndMessaging::GW_HWNDPREV).ok();
            while let Some(window) = at {
                above |= window == hwnd;
                at = GetWindow(window, windows::Win32::UI::WindowsAndMessaging::GW_HWNDPREV).ok();
            }
            let mut rect = RECT::default();
            let _ = GetWindowRect(game, &mut rect);
            log!(
                "settings: the game {:#x} ex {:#x} at {},{} {}x{}; the window is {} it",
                game.0 as isize,
                GetWindowLongW(game, GWL_EXSTYLE),
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                if above { "above" } else { "below" }
            );
        }
    }
}

/// The size, and where the top left goes in points: the window centred on the monitor of the
/// game, or of the screen when there is no game window.
fn placement() -> ([f32; 2], [f32; 2]) {
    unsafe {
        let game = FindWindowW(None, w!("Path of Exile")).unwrap_or_default();
        let monitor = MonitorFromWindow(game, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let dpi = if game.0.is_null() { GetDpiForSystem() } else { GetDpiForWindow(game) };
        let scale = if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 };
        let RECT { left, top, right, bottom } =
            if GetMonitorInfoW(monitor, &mut info).as_bool() { info.rcWork } else { RECT { left: 0, top: 0, right: 1920, bottom: 1080 } };
        let (area_w, area_h) = ((right - left) as f32 / scale, (bottom - top) as f32 / scale);
        let size = [WIDTH.min(area_w - 40.0).max(320.0), HEIGHT.min(area_h - 60.0).max(320.0)];
        let at = [left as f32 / scale + (area_w - size[0]) / 2.0, top as f32 / scale + (area_h - size[1]) / 2.0];
        (size, at)
    }
}

/// Main thread, on every frame of the root window: keeps the viewport alive while the window is
/// open. Without the call the window is gone at the end of the frame.
pub fn show(ctx: &egui::Context) {
    if !OPEN.load(SeqCst) {
        return;
    }
    let Some((size, position)) = STATE.lock().unwrap().as_ref().map(|(_, size, position)| (*size, *position)) else {
        return;
    };
    if !SEEN.swap(true, SeqCst) {
        log!("settings: a root pass saw the window open (logic pass {}), showing its viewport", crate::LOGIC_PASSES.load(SeqCst));
    }
    // The root is hidden most of the time and would otherwise sleep until something wakes it.
    ctx.request_repaint_after(Duration::from_millis(250));
    let unseen = crate::UNSEEN.load(SeqCst);
    let builder = egui::ViewportBuilder::default()
        .with_title(TITLE)
        .with_inner_size(size)
        .with_position(if unseen { [-4000.0, -4000.0] } else { position })
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        // Above the game, and created without activation: see the top of the file.
        .with_window_level(egui::WindowLevel::AlwaysOnTop)
        .with_active(false);
    ctx.show_viewport_deferred(viewport_id(), builder, |ui, _class| {
        let ctx = ui.ctx().clone();
        let mut closing = ctx.input(|input| input.viewport().close_requested());
        {
            let mut state = STATE.lock().unwrap();
            let Some((window, _, _)) = state.as_mut() else {
                return;
            };
            if FOCUS.swap(false, SeqCst) {
                forward();
            }
            let frame = FRAMES.fetch_add(1, SeqCst) + 1;
            if frame == 1 {
                log!("settings opened: its viewport painted its first frame");
                if !crate::UNSEEN.load(SeqCst) {
                    forward();
                } else if let Some(hwnd) = hwnd() {
                    crate::cloak(hwnd, true);
                }
            }
            if frame == 3 {
                describe_window();
            }
            window.ui(ui);
            closing |= window.close;
            #[cfg(feature = "dev")]
            if let Some(out) = window.shot.clone() {
                capture::save_fourth(&ctx, frame, out);
            }
        }
        if closing {
            close();
        }
    });
}

/// A download that runs in a thread of its own: what it says, and whether it is still going.
#[derive(Clone)]
struct Task {
    said: Arc<Mutex<String>>,
    busy: Arc<AtomicBool>,
}

impl Task {
    fn new() -> Self {
        Self { said: Arc::default(), busy: Arc::default() }
    }

    fn start(&self, ctx: &egui::Context, work: impl FnOnce() -> i32 + Send + 'static) {
        if self.busy.swap(true, SeqCst) {
            return;
        }
        *self.said.lock().unwrap() = "Working".into();
        let (task, ctx) = (self.clone(), ctx.clone());
        std::thread::spawn(move || {
            let code = work();
            *task.said.lock().unwrap() = if code == 0 { "Done" } else { "Failed" }.into();
            task.busy.store(false, SeqCst);
            ctx.request_repaint();
        });
    }
}

/// A binding of the Hotkeys section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    PriceCheck,
    Wiki,
    Settings,
    MapMods,
    CraftOdds,
    Command(usize),
    Search(usize),
}

struct Window {
    draft: Draft,
    /// The binding whose button waits for the next key combo.
    capturing: Option<Slot>,
    config: Config,
    /// What is typed in the league field.
    league: String,
    data: Task,
    prices: Task,
    shot: Option<String>,
    /// What the page asks of the window, what the window is besides it, the height asked of the
    /// system last: see `fit`.
    content: f32,
    chrome: f32,
    height: f32,
    page: Page,
    mods: Mods,
    mod_search: String,
    add_text: String,
    add_tag: MapTag,
    /// Set by the key that opens the window, pressed inside it.
    close: bool,
}

impl Window {
    fn new(shot: Option<String>) -> Self {
        let mut config = *crate::CONFIG.lock().unwrap();
        if shot.is_some() {
            (config.theme, config.marker) = (look::theme(), look::marker());
        }
        let mut draft = Draft::from_hotkeys(&crate::current_hotkeys().unwrap_or_default());
        if shot.is_some() {
            // What the capture shows: a command still without keys, and two searches.
            draft.searches.push(DraftSearch { name: "Quality".into(), text: "\"Quality: +20%\"".into(), keys: Combo::parse("Ctrl + F6").ok() });
            draft.searches.push(DraftSearch { name: "Ignite".into(), text: "\"!ents$|ur$|ced f\"".into(), keys: None });
        }
        let league = config.league.map(|l| l.as_str().to_owned()).unwrap_or_default();
        Self { draft, capturing: None, config, league, data: Task::new(), prices: Task::new(), page: Page::ALL[LAST_PAGE.load(SeqCst) as usize % Page::ALL.len()], mods: Mods::load(shot.is_some()), mod_search: String::new(), add_text: String::new(), add_tag: MapTag::Danger, shot, close: false, content: 0.0, chrome: 0.0, height: 0.0 }
    }

    /// Keeps `config`: the panel takes the new look on its next frame, the file is written.
    fn apply(&mut self) {
        look::set(self.config.theme, self.config.marker);
        crate::check::set_slack(self.config.slack);
        let was_on = crate::STASH_SCROLL.load(SeqCst);
        if self.config.stash_scroll && !was_on && self.shot.is_none() {
            crate::stash_scroll_on();
        } else {
            crate::STASH_SCROLL.store(self.config.stash_scroll, SeqCst);
        }
        *crate::CONFIG.lock().unwrap() = self.config;
        crate::save_config(self.config);
        if let Some(ctx) = crate::CTX.get() {
            ctx.request_repaint();
        }
    }

    fn theme(&mut self, ui: &mut egui::Ui) {
        let mut theme = self.config.theme;
        ui.horizontal_wrapped(|ui| {
            for option in THEMES {
                chip(ui, &mut theme, option, option.label());
            }
        });
        if theme != self.config.theme {
            self.config.theme = theme;
            self.apply();
        }
    }

    fn marker(&mut self, ui: &mut egui::Ui) {
        let mut marker = self.config.marker;
        ui.horizontal_wrapped(|ui| {
            for option in MARKERS {
                chip(ui, &mut marker, option, option.label());
            }
        });
        if marker != self.config.marker {
            self.config.marker = marker;
            self.apply();
        }
    }

    fn default_min(&mut self, ui: &mut egui::Ui) {
        let mut slack = self.config.slack;
        ui.horizontal_wrapped(|ui| {
            for option in Slack::ALL {
                let text = if option == Slack::Exact { "Exact".to_owned() } else { format!("\u{2212}{}%", option.percent()) };
                chip(ui, &mut slack, option, &text);
            }
        });
        if slack != self.config.slack {
            self.config.slack = slack;
            self.apply();
        }
    }

    fn league(&mut self, ui: &mut egui::Ui) {
        let field = ui.add(field(&mut self.league, 220.0, "Current league", look::palette().star));
        if field.lost_focus() {
            let league = League::new(&self.league);
            self.league = league.map(|l| l.as_str().to_owned()).unwrap_or_default();
            if league != self.config.league {
                self.config.league = league;
                self.apply();
                if let Some(jobs) = crate::JOBS.get() {
                    let _ = jobs.send(crate::worker::Job::League(league.map(|l| l.as_str().to_owned())));
                }
            }
        }
    }

    /// One button that shows a combo and waits for the next one when clicked.
    fn combo_button(&mut self, ui: &mut egui::Ui, slot: Slot, combo: Option<Combo>, clash: bool) {
        let p = look::palette();
        let waiting = self.capturing == Some(slot);
        let text = if waiting {
            t("Press keys", BODY, 700.0, p.gold)
        } else {
            match combo {
                Some(combo) if clash => t(combo.label(), BODY, 700.0, RED),
                Some(combo) => t(combo.label(), BODY, 700.0, p.gold_hi),
                None => hint("Not set"),
            }
        };
        let mut button = egui::Button::new(text).min_size(Vec2::new(150.0, 28.0));
        if clash && !waiting {
            button = button.stroke(Stroke::new(1.0, RED));
        }
        if ui.add(button).clicked() {
            self.capturing = if waiting { None } else { Some(slot) };
        }
    }

    /// While a button waits, the next key combo pressed in this window is its keys.
    fn take_keys(&mut self, ui: &egui::Ui) {
        let Some(slot) = self.capturing else {
            return;
        };
        let events = ui.input(|input| input.events.clone());
        for event in events {
            let egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } = event else {
                continue;
            };
            let result = capture(Some(key), modifiers.ctrl, modifiers.alt, modifiers.shift);
            let keys = match slot {
                Slot::PriceCheck => &mut self.draft.price_check,
                Slot::Wiki => &mut self.draft.wiki,
                Slot::Settings => &mut self.draft.settings,
                Slot::MapMods => &mut self.draft.map_mods,
                Slot::CraftOdds => &mut self.draft.craft_odds,
                Slot::Search(at) => match self.draft.searches.get_mut(at) {
                    Some(search) => &mut search.keys,
                    None => {
                        self.capturing = None;
                        return;
                    }
                },
                Slot::Command(at) => match self.draft.commands.get_mut(at) {
                    Some(command) => &mut command.keys,
                    None => {
                        self.capturing = None;
                        return;
                    }
                },
            };
            match result {
                Captured::Waiting => continue,
                Captured::Cancel => {}
                // The price check always has keys.
                Captured::Clear if slot != Slot::PriceCheck => *keys = None,
                Captured::Clear => {}
                Captured::Set(combo) => *keys = Some(combo),
            }
            self.capturing = None;
            break;
        }
    }

    fn hotkeys(&mut self, ui: &mut egui::Ui) {
        let p = look::palette();
        self.take_keys(ui);
        let clashes = self.draft.conflicts();
        let name = |ui: &mut egui::Ui, text: &str| {
            cell(ui, text);
        };
        ui.horizontal(|ui| {
            name(ui, "Price check");
            self.combo_button(ui, Slot::PriceCheck, self.draft.price_check, clashes.price_check);
        });
        ui.horizontal(|ui| {
            name(ui, "Wiki");
            self.combo_button(ui, Slot::Wiki, self.draft.wiki, clashes.wiki);
        });
        ui.horizontal(|ui| {
            name(ui, "Quick menu");
            self.combo_button(ui, Slot::Settings, self.draft.settings, clashes.settings);
        });
        ui.horizontal(|ui| {
            name(ui, "Map mods");
            self.combo_button(ui, Slot::MapMods, self.draft.map_mods, clashes.map_mods);
        });
        ui.horizontal(|ui| {
            name(ui, "Craft odds");
            self.combo_button(ui, Slot::CraftOdds, self.draft.craft_odds, clashes.craft_odds);
        });
        let mut remove = None;
        for at in 0..self.draft.commands.len() {
            ui.horizontal(|ui| {
                let command = &mut self.draft.commands[at];
                let usable = crate::keys::is_command_text(&command.text);
                let color = if usable { p.star } else { RED };
                ui.add(field(&mut command.text, 122.0, "/command", color));
                let keys = command.keys;
                self.combo_button(ui, Slot::Command(at), keys, clashes.commands[at]);
                let command = &mut self.draft.commands[at];
                ui.checkbox(&mut command.send, label("Send"));
                if ui.add(egui::Button::new(t("\u{00d7}", 17.0, 600.0, p.star)).min_size(Vec2::new(24.0, 24.0))).clicked() {
                    remove = Some(at);
                }
            });
        }
        if let Some(at) = remove {
            self.draft.commands.remove(at);
            self.capturing = None;
        }
        if ui.button(label("Add a command")).clicked() {
            self.draft.commands.push(DraftCommand { keys: None, text: String::new(), send: true });
        }
        ui.horizontal(|ui| {
            name(ui, "Quit");
            ui.label(t(QUIT.label(), BODY, 700.0, p.gold_hi));
        });
        if self.draft.has_conflict() {
            ui.label(t("Two bindings share their keys: not saved", 13.5, 600.0, RED));
        }
        // Kept and applied at once, but not while a text is being typed.
        let typing = ui.ctx().memory(|memory| memory.focused()).is_some();
        if !typing
            && self.shot.is_none()
            && let Some(kept) = self.draft.to_hotkeys()
            && crate::current_hotkeys().as_ref() != Some(&kept)
        {
            crate::set_hotkeys(kept);
        }
    }

    /// The saved searches: name, text, key.
    fn searches(&mut self, ui: &mut egui::Ui) {
        let p = look::palette();
        let clashes = self.draft.conflicts();
        let mut drop_search = None;
        for at in 0..self.draft.searches.len() {
            ui.horizontal(|ui| {
                let search = &mut self.draft.searches[at];
                let usable = crate::keys::is_command_text(&search.text);
                ui.add(field(&mut search.name, 90.0, "Name", p.star));
                ui.add(field(&mut search.text, 170.0, "Search", if usable { p.star } else { RED }));
                let keys = search.keys;
                self.combo_button(ui, Slot::Search(at), keys, clashes.searches[at]);
                if ui.add(egui::Button::new(t("\u{00d7}", 17.0, 600.0, p.star)).min_size(Vec2::new(24.0, 24.0))).clicked() {
                    drop_search = Some(at);
                }
            });
        }
        if let Some(at) = drop_search {
            self.draft.searches.remove(at);
            self.capturing = None;
        }
        if ui.button(label("Add a search")).clicked() {
            self.draft.searches.push(DraftSearch { name: String::new(), text: String::new(), keys: None });
        }
    }

    fn section(ui: &mut egui::Ui, name: &str) {
        let p = look::palette();
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
            ui.painter().circle_filled(dot.center(), 3.0, p.gold);
            ui.label(t(name, 17.0, 700.0, p.star));
        });
        ui.add_space(6.0);
    }

    fn row(ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui)) {
        ui.horizontal(|ui| {
            cell(ui, label);
            add(ui);
        });
        ui.add_space(4.0);
    }

    /// The left list: the settings pages, then the tools.
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let p = look::palette();
        let group = |ui: &mut egui::Ui, name: &str| {
            ui.add_space(10.0);
            ui.label(t(name.to_uppercase(), 11.5, 700.0, p.mute));
            ui.add_space(4.0);
        };
        let mut chosen = self.page;
        let current = self.page;
        let mut item = |ui: &mut egui::Ui, page: Page| {
            let (rect, hit) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), egui::Sense::click());
            let on = current == page;
            if on {
                ui.painter().rect_filled(rect, CornerRadius::same(8), p.raise);
                let bar = egui::Rect::from_min_size(egui::pos2(rect.left(), rect.top() + 6.0), Vec2::new(3.0, rect.height() - 12.0));
                ui.painter().rect_filled(bar, CornerRadius::same(2), p.gold);
            } else if hit.hovered() {
                ui.painter().rect_filled(rect, CornerRadius::same(8), p.hair);
            }
            let g = ui.painter().layout_job(t(page.name(), BODY, if on { 700.0 } else { 500.0 }, p.star));
            ui.painter().galley(egui::pos2(rect.left() + 14.0, (rect.center().y - g.size().y / 2.0).round()), g, p.star);
            if hit.clicked() {
                chosen = page;
            }
        };
        group(ui, "Settings");
        for page in SETTINGS {
            item(ui, page);
        }
        group(ui, "Tools");
        for tool in TOOLS {
            item(ui, tool.page);
        }
        if chosen != self.page {
            self.page = chosen;
            LAST_PAGE.store(chosen as u8, SeqCst);
        }
    }

    fn page_ui(&mut self, ui: &mut egui::Ui) {
        let p = look::palette();
        Self::section(ui, self.page.name());
        match self.page {
            Page::General => {
                Self::row(ui, "League", &mut |ui| self.league(ui));
                Self::row(ui, "Default minimum", &mut |ui| self.default_min(ui));
                Self::row(ui, "Stash scroll", &mut |ui| {
                    let mut on = self.config.stash_scroll;
                    ui.checkbox(&mut on, label("Ctrl and wheel"));
                    if on != self.config.stash_scroll {
                        self.config.stash_scroll = on;
                        self.apply();
                    }
                });
                Self::row(ui, "Map bulk", &mut |ui| {
                    let mut on = self.config.map_bulk;
                    ui.checkbox(&mut on, label("Bulk exchange for maps"));
                    if on != self.config.map_bulk {
                        self.config.map_bulk = on;
                        self.apply();
                    }
                });
                let ctx = ui.ctx().clone();
                let league = self.config.league.map(|l| l.as_str().to_owned());
                Self::row(ui, "Game data", &mut |ui| {
                    let busy = self.data.busy.load(SeqCst);
                    if ui.add_enabled(!busy, egui::Button::new(label("Update game data"))).clicked() {
                        self.data.start(&ctx, crate::worker::update_data);
                    }
                    ui.label(hint(self.data.said.lock().unwrap().as_str()));
                });
                Self::row(ui, "Prices", &mut |ui| {
                    let busy = self.prices.busy.load(SeqCst);
                    if ui.add_enabled(!busy, egui::Button::new(label("Update prices"))).clicked() {
                        let league = league.clone().or_else(crate::worker::league_override);
                        self.prices.start(&ctx, move || crate::worker::update_prices(league));
                    }
                    ui.label(hint(self.prices.said.lock().unwrap().as_str()));
                });
                ui.add_space(24.0);
                ui.label(t(NOTICE, 12.5, 500.0, p.mute));
            }
            Page::Appearance => {
                Self::row(ui, "Theme", &mut |ui| self.theme(ui));
                Self::row(ui, "Searched mods", &mut |ui| self.marker(ui));
            }
            Page::Hotkeys => self.hotkeys(ui),
            Page::Stash => self.searches(ui),
            Page::MapFilter => self.map_filter(ui),
        }
    }

    /// The map mods the player tagged: filter, retag, remove, add.
    fn map_filter(&mut self, ui: &mut egui::Ui) {
        let p = look::palette();
        let entries = self.mods.entries();
        ui.horizontal(|ui| {
            ui.add(field(&mut self.mod_search, 260.0, "Search the list", p.star));
            ui.label(hint(&format!("{} tagged", entries.len())));
        });
        ui.add_space(6.0);
        let needle = self.mod_search.trim().to_lowercase();
        let mut change: Option<(String, Option<MapTag>)> = None;
        for (text, tag) in entries {
            if !needle.is_empty() && !text.contains(&needle) {
                continue;
            }
            ui.horizontal(|ui| {
                for option in TAGS {
                    let on = tag == option;
                    let job = t(tag_name(option), 12.5, 700.0, if on { p.night } else { tag_colour(option) });
                    let button = egui::Button::new(job).min_size(Vec2::new(62.0, 22.0)).corner_radius(CornerRadius::same(11));
                    let button = if on { button.fill(tag_colour(option)) } else { button.fill(p.raise) };
                    if ui.add(button).clicked() && !on {
                        change = Some((text.clone(), Some(option)));
                    }
                }
                if ui.add(egui::Button::new(t("\u{00d7}", 17.0, 600.0, p.star)).min_size(Vec2::new(24.0, 24.0))).clicked() {
                    change = Some((text.clone(), None));
                }
                ui.add(egui::Label::new(label(&text)).truncate());
            });
        }
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add(field(&mut self.add_text, 200.0, "Mod wording, numbers as #", p.star));
            for option in TAGS {
                chip(ui, &mut self.add_tag, option, tag_name(option));
            }
            if ui.add_enabled(!self.add_text.trim().is_empty(), egui::Button::new(label("Add mod"))).clicked() {
                change = Some((std::mem::take(&mut self.add_text), Some(self.add_tag)));
            }
        });
        if let Some((text, tag)) = change {
            self.mods.set(&text, tag, self.shot.is_none());
        }
    }
}

/// The pages. The settings come first, then the tools: adding a tool is one line in `TOOLS`
/// and one arm in `page_ui`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    General,
    Appearance,
    Hotkeys,
    Stash,
    MapFilter,
}

struct Tool {
    page: Page,
}

const SETTINGS: [Page; 3] = [Page::General, Page::Appearance, Page::Hotkeys];
const TOOLS: [Tool; 2] = [Tool { page: Page::Stash }, Tool { page: Page::MapFilter }];

impl Page {
    const ALL: [Page; 5] = [Page::General, Page::Appearance, Page::Hotkeys, Page::Stash, Page::MapFilter];

    fn name(self) -> &'static str {
        match self {
            Page::General => "General",
            Page::Appearance => "Appearance",
            Page::Hotkeys => "Hotkeys",
            Page::Stash => "Stash searches",
            Page::MapFilter => "Map filter",
        }
    }

    fn of_name(name: &str) -> Option<Self> {
        let name = name.to_lowercase().replace(['-', '_'], " ");
        Self::ALL.into_iter().find(|page| {
            let full = page.name().to_lowercase();
            full == name || full.split(' ').next() == Some(name.as_str())
        })
    }
}

/// The page shown last, kept while the app runs.
static LAST_PAGE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// `--settings-page name`: the page the next window opens on.
pub fn set_page(name: &str) {
    if let Some(page) = Page::of_name(name) {
        LAST_PAGE.store(page as u8, SeqCst);
        WANTED_PAGE.store(page as u8, SeqCst);
    }
}

/// A page asked for while the window may already be open: it switches to it at its next frame.
static WANTED_PAGE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(u8::MAX);

/// The tallest the window grows, as a share of the monitor.
const MAX_SHARE: f32 = 0.85;
/// The least it shrinks to: the sidebar and the header always fit.
const MIN_HEIGHT: f32 = 380.0;

/// The height of a window whose page needs `content` points under a header and around a
/// scroll area of `chrome` points of its own: as tall as that, between the least and the share
/// of the monitor (`monitor` points tall) a window may take.
fn fitted_height(content: f32, chrome: f32, monitor: f32) -> f32 {
    let most = (monitor * MAX_SHARE).max(MIN_HEIGHT);
    (content + chrome).clamp(MIN_HEIGHT, most).ceil()
}

use crate::maps::Tag as MapTag;

/// How a tag is worded and coloured on this page.
fn tag_name(tag: MapTag) -> &'static str {
    match tag {
        MapTag::Good => "Good",
        MapTag::Warning => "Warning",
        MapTag::Danger => "Danger",
    }
}

fn tag_colour(tag: MapTag) -> Color32 {
    match tag {
        MapTag::Good => Color32::from_rgb(110, 200, 120),
        MapTag::Warning => Color32::from_rgb(240, 160, 70),
        MapTag::Danger => RED,
    }
}

const TAGS: [MapTag; 3] = [MapTag::Good, MapTag::Warning, MapTag::Danger];

/// The map mods store with where it is kept.
struct Mods {
    store: crate::maps::MapMods,
    path: std::path::PathBuf,
}

impl Mods {
    fn load(sample: bool) -> Self {
        // Where the store keeps its file (its own helper is private).
        let path = crate::config::data_dir().join("map-mods.json");
        let mut store = if sample { crate::maps::MapMods::default() } else { crate::maps::MapMods::load() };
        if sample {
            use crate::maps::Tag;
            for (text, tag) in [
                ("Monsters deal #% extra Physical Damage as Fire", Tag::Danger),
                ("Players cannot regenerate Life, Mana or Energy Shield", Tag::Danger),
                ("Monsters have #% increased Area of Effect", Tag::Danger),
                ("#% increased Quantity of Items found in this Area", Tag::Good),
                ("#% increased Pack Size", Tag::Good),
                ("Area has #% increased Monster Life", Tag::Good),
            ] {
                store.set(text, Some(tag));
            }
        }
        Self { store, path }
    }

    fn entries(&self) -> Vec<(String, MapTag)> {
        self.store.entries()
    }

    fn set(&mut self, text: &str, tag: Option<MapTag>, write: bool) {
        self.store.set(text, tag);
        if write && let Err(error) = self.store.save(&self.path) {
            println!("map filter not saved: {error}");
        }
    }
}

/// The look of this window, on its own ui: the style of the root, which paints the panel, is
/// left as it is.
fn style(ui: &mut egui::Ui) {
    let p = look::palette();
    let style = ui.style_mut();
    let v = &mut style.visuals;
    v.panel_fill = p.plate;
    v.override_text_color = Some(p.star);
    v.selection.bg_fill = p.gold;
    v.selection.stroke = Stroke::new(1.0, p.night);
    v.extreme_bg_color = p.night;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.hair);
    for (widget, fill) in [(&mut v.widgets.inactive, p.raise), (&mut v.widgets.hovered, p.hair), (&mut v.widgets.active, p.line)] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = Stroke::NONE;
        widget.corner_radius = CornerRadius::same(9);
        widget.fg_stroke = Stroke::new(1.0, p.star);
    }
    style.spacing.button_padding = Vec2::new(12.0, 5.0);
    style.text_styles.insert(egui::TextStyle::Body, FontId::new(BODY, FontFamily::Name("text-vf".into())));
    style.text_styles.insert(egui::TextStyle::Button, FontId::new(BODY, FontFamily::Name("text-vf".into())));
    style.spacing.item_spacing = Vec2::new(6.0, 6.0);
    // A thin bar that is always there, so that content below the fold is seen to exist.
    style.spacing.scroll = egui::style::ScrollStyle {
        floating: false,
        bar_width: 6.0,
        handle_min_length: 28.0,
        bar_inner_margin: 3.0,
        bar_outer_margin: 0.0,
        ..egui::style::ScrollStyle::solid()
    };
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, p.star);
}

impl Window {
    fn ui(&mut self, ui: &mut egui::Ui) {
        style(ui);
        let p = look::palette();
        let wanted = WANTED_PAGE.swap(u8::MAX, SeqCst);
        if let Some(page) = Page::ALL.get(wanted as usize) {
            self.page = *page;
        }
        // The key that opens the window closes it, when it is pressed in it and nothing waits
        // for keys or is being typed in.
        if self.capturing.is_none() && ui.ctx().memory(|memory| memory.focused()).is_none() {
            let opener = crate::current_hotkeys().and_then(|keys| keys.settings);
            let pressed = ui.input(|input| {
                input.events.iter().any(|event| {
                    let egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } = event else {
                        return false;
                    };
                    matches!(capture(Some(*key), modifiers.ctrl, modifiers.alt, modifiers.shift), Captured::Set(combo) if Some(combo) == opener)
                })
            });
            if pressed && self.shot.is_none() {
                self.close = true;
            }
        }
        if self.capturing.is_none() && self.shot.is_none() && ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.close = true;
        }
        let frame = Frame::new()
            .fill(p.plate)
            .stroke(Stroke::new(1.0, p.line))
            .corner_radius(CornerRadius::same(14))
            .inner_margin(Margin { left: 24, right: 24, top: 8, bottom: 20 });
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            self.header(ui);
            let height = ui.available_height();
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(Vec2::new(150.0, height), egui::Layout::top_down(Align::Min), |ui| {
                    ui.set_min_size(Vec2::new(150.0, height));
                    self.sidebar(ui);
                });
                ui.add_space(8.0);
                let width = ui.available_width();
                ui.allocate_ui_with_layout(Vec2::new(width, height), egui::Layout::top_down(Align::Min), |ui| {
                    ui.set_min_size(Vec2::new(width, height));
                    let out = egui::ScrollArea::vertical().id_salt(self.page as u8).auto_shrink([false, false]).show(ui, |ui| self.page_ui(ui));
                    // What the window is besides the scroll area, and what the page asks of it.
                    self.chrome = ui.ctx().content_rect().height() - out.inner_rect.height();
                    self.content = out.content_size.y;
                });
            });
        });
        self.fit(ui.ctx());
    }

    /// Sizes the window to its page: only when the height it should have changes.
    fn fit(&mut self, ctx: &egui::Context) {
        if self.content <= 0.0 {
            return;
        }
        let monitor = ctx.input(|input| input.viewport().monitor_size).map_or(HEIGHT, |size| size.y);
        let height = fitted_height(self.content, self.chrome, monitor);
        if (height - self.height).abs() >= 1.0 {
            self.height = height;
            let width = ctx.content_rect().width();
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(width, height)));
            ctx.request_repaint();
        }
    }

    /// The title bar the window does not have: drag it to move the window, the cross closes it.
    fn header(&mut self, ui: &mut egui::Ui) {
        let p = look::palette();
        let (bar, drag) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), egui::Sense::click_and_drag());
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        let orb = egui::Rect::from_min_size(egui::pos2(bar.left(), bar.center().y - 11.0), Vec2::splat(22.0));
        // Loaded once, kept in the context's memory (outside its lock while the texture is made).
        let slot = egui::Id::NULL.with("divine-orb");
        let mut orb_texture = ui.ctx().data(|data| data.get_temp::<egui::TextureHandle>(slot));
        if orb_texture.is_none()
            && let Ok(image) = crate::panel::icons::decode(include_bytes!("../assets/divine-source.png"))
        {
            let texture = ui.ctx().load_texture("divine-orb", image, egui::TextureOptions::LINEAR);
            ui.ctx().data_mut(|data| data.insert_temp(slot, texture.clone()));
            orb_texture = Some(texture);
        }
        if let Some(texture) = orb_texture {
            ui.painter().image(texture.id(), orb, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        let galley = ui.painter().layout_job(t("Settings", 22.0, 700.0, p.star));
        let at = egui::pos2(bar.left() + 30.0, (bar.center().y - galley.size().y / 2.0).round());
        ui.painter().galley(at, galley, p.star);
        let size = Vec2::splat(30.0);
        // The version and where the update stands, small, left of the cross; a click on the
        // action word does what it says.
        let mut right = bar.right() - size.x - 10.0;
        if let Some(label) = crate::update::action_label() {
            let g = ui.painter().layout_job(t(label, 11.5, 500.0, p.gold_hi));
            let rect = egui::Rect::from_min_size(egui::pos2(right - g.size().x, bar.center().y - g.size().y / 2.0), g.size());
            let hit = ui.interact(rect.expand(5.0), ui.scope_id().with("update"), egui::Sense::click());
            let tone = if hit.hovered() { p.star } else { p.gold_hi };
            ui.painter().galley(rect.min, g, tone);
            if hit.clicked() && self.shot.is_none() {
                crate::update::run_action();
            }
            right = rect.left() - 10.0;
        }
        let g = ui.painter().layout_job(t(&crate::update::status_text(), 11.5, 500.0, p.mute));
        ui.painter().galley(egui::pos2(right - g.size().x, bar.center().y - g.size().y / 2.0), g, p.mute);
        let cross = egui::Rect::from_min_size(egui::pos2(bar.right() - size.x, bar.center().y - size.y / 2.0), size);
        let hit = ui.interact(cross, ui.scope_id().with("close"), egui::Sense::click());
        if hit.hovered() {
            ui.painter().rect_filled(cross, CornerRadius::same(9), p.hair);
        }
        let g = ui.painter().layout_job(t("\u{00d7}", 22.0, 600.0, p.star));
        ui.painter().galley((cross.center() - g.size() / 2.0).round(), g, p.star);
        if hit.clicked() && self.shot.is_none() {
            self.close = true;
        }
    }
}

#[cfg(test)]
mod size_tests {
    use super::*;

    #[test]
    fn the_window_is_as_tall_as_its_page_up_to_most_of_the_monitor() {
        assert_eq!(fitted_height(500.0, 150.0, 1080.0), 650.0);
        // a long page scrolls: 85 % of the monitor
        assert_eq!(fitted_height(3000.0, 150.0, 1000.0), 850.0);
        // a short one keeps the sidebar whole
        assert_eq!(fitted_height(40.0, 150.0, 1080.0), MIN_HEIGHT);
    }

    #[test]
    fn the_menu_entries_name_pages_that_exist() {
        for item in crate::launcher::items() {
            if let crate::launcher::Target::Settings(Some(name)) = item.target {
                assert!(Page::of_name(name).is_some(), "{name}");
            }
        }
        assert_eq!(Page::of_name("Stash searches"), Some(Page::Stash));
    }
}
