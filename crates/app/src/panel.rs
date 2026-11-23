//! The price-check panel, drawn by hand from a `View` in the Forge look of
//! the design's sizes and colours, written as CSS. Coordinates are CSS
//! pixels inside the plate's padding box. A 1px border is one physical pixel, in the layout too,
//! as a browser has it at a fractional scale (0.8px at 125%): `hair` below.
//!
//! The plate is as tall as its content. `measure` runs the same layout as `show` without
//! drawing, so that the window can be sized before the frame that shows it.
//!
//! What can be clicked is a `Zone` of the layout: a rectangle and the `Action` a click in it is.
//! `show` reports the actions of the frame; what they do is `check::apply`'s business.
//!
//! A row that is searched has its two number fields at the right, each a `NumField` of the
//! layout. `show` reports a press in one and the wheel turned over one; who has the keyboard and
//! what a key does is not decided here: see `field` and the caller.
//!
//! Under the estimate of a piece of gear, the two ways of pricing it, as the segmented control
//! of the settings (`.seg`): each a zone. Under it, the chips of the item's own options
//! (`.opts`): each a zone, and the number of one that is on a `NumField`. Above the buttons, the three settings of every search
//! (`.defaults`), each with its pin. A right click on a chip that has several values is reported
//! too: the caller says which chip's menu is open, and the menu is drawn over everything else.
//! While it is open, its lines are the only zones. The minimum field of a searched mod whose
//! range is known has such a menu as well, of the rolls it can be set to: see `check::rolls`.
//!
//! Such a mod also has a slider, a `Track` of the layout, while the pointer is over its row, one
//! of its fields is being typed in, its menu is open or the slider is held: a rail under the
//! text of its first line, inside the row's own height, so that nothing moves when it comes and
//! goes. `show` reports the slider that is held from one frame to the next and the action its
//! release is; `slide` is that behaviour, pure. See `slider` for what a place on the rail is
//! worth.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, FontFamily, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind,
    TextFormat, epaint::text::VariationCoords, pos2, text::LayoutJob, vec2,
};
use zenith_core::{Item, Rarity};
#[path = "icons.rs"]
pub mod icons;

use zenith_trade::Listing;
use zenith_trade::client::Budget;

use crate::check::{Action, Again, Check, Chip, Choice, ItemOption, Mark, Outcome, Plan, SETTINGS, Setting, WAYS, Way, again, choices, cycle, rolls};
use crate::config::Config;
use crate::maps::{MapView, Tag};
use crate::look::{self, Marker};
use crate::launcher;
use crate::quote;
use crate::field::{self, Bound, Fields, Target, Typed};
use crate::slider::{self, Drag, Scale};
use crate::state::{self, View};

pub const WIDTH: f32 = 420.0;
/// box-shadow: 0 3px 0
pub const SHADOW: f32 = 3.0;

// Theme "Forge", the default: `.game[data-theme="forge"] .overlay`. The names are the
// names of the CSS variables.
/// The plate's colour as 0xRRGGBB, for the self-test's pixel check.
pub const PLATE_RGB: u32 = 0x2E2925;
/// What is fractured, in the tone the game writes a fractured mod in.
const FRACTURED: Color32 = Color32::from_rgb(0xA2, 0x91, 0x62);
/// The text of a fractured mod: the same tone, lighter, so that it reads on the plate.
const FRACTURED_TEXT: Color32 = Color32::from_rgb(0xCD, 0xBA, 0x85);
/// What was crafted at the bench, in the light blue players know it by, and the text of such a mod.
const CRAFTED: Color32 = Color32::from_rgb(0x8F, 0xA8, 0xFF);
const CRAFTED_TEXT: Color32 = Color32::from_rgb(0xB9, 0xC6, 0xFF);
/// What an enchant gave: a teal, apart from the blue of a crafted mod.
const ENCHANT: Color32 = Color32::from_rgb(0x4F, 0xC8, 0xC0);
const ENCHANT_TEXT: Color32 = Color32::from_rgb(0x8F, 0xE0, 0xDA);
/// What the Foulborn mechanic changed: a purple, apart from the gold of a fractured mod and the
/// blue of a crafted one. One per theme, picked to read on that theme's plate: the Eldritch
/// plate is itself violet, so its purple leans to magenta.
fn foulborn_tone() -> Color32 {
    match look::theme() {
        look::Theme::Eldritch => Color32::from_rgb(0xC0, 0x7C, 0xF2),
        look::Theme::Frost | look::Theme::Midnight => Color32::from_rgb(0xB0, 0x82, 0xF0),
        _ => Color32::from_rgb(0xAE, 0x7C, 0xE6),
    }
}
/// A tone toward white, as the text of a marked mod has it.
fn lighter(tone: Color32) -> Color32 {
    let mix = |c: u8| (c as f32 + 0.3 * (255.0 - c as f32)).round() as u8;
    Color32::from_rgb(mix(tone.r()), mix(tone.g()), mix(tone.b()))
}
/// The text of a Foulborn mod: the same purple, lighter.
fn foulborn_text() -> Color32 {
    let tone = foulborn_tone();
    let mix = |c: u8| (c as f32 + 0.45 * (255.0 - c as f32)).round() as u8;
    Color32::from_rgb(mix(tone.r()), mix(tone.g()), mix(tone.b()))
}
// The rarity marks. The first three are the common ones; the others have none.
const RARE: Color32 = Color32::from_rgb(0xE6, 0xDD, 0x7A);
const UNIQUE: Color32 = Color32::from_rgb(0xDE, 0x8A, 0x45);
const CURRENCY: Color32 = Color32::from_rgb(0xB9, 0xAD, 0x8C);
const MAGIC: Color32 = Color32::from_rgb(0x8F, 0x9B, 0xF0);
const GEM: Color32 = Color32::from_rgb(0x4F, 0xB8, 0xA8);

fn tone_plate() -> Color32 {
    look::palette().plate
}
fn tone_night() -> Color32 {
    look::palette().night
}
fn tone_raise() -> Color32 {
    look::palette().raise
}
fn tone_hair() -> Color32 {
    look::palette().hair
}
fn tone_line() -> Color32 {
    look::palette().line
}
fn tone_star() -> Color32 {
    look::palette().star
}
fn tone_gold_hi() -> Color32 {
    look::palette().gold_hi
}
fn tone_gold() -> Color32 {
    look::palette().gold
}
fn tone_mute() -> Color32 {
    look::palette().mute
}
fn tone_ember() -> Color32 {
    look::palette().ember
}

#[derive(Clone, Copy)]
pub enum F {
    T400,
    T500,
    T600,
    T700,
    D700,
    D800,
}

/// Manrope for rows and sentences, Sora for the name and the figures: the variable fonts of the
/// google/fonts repository. The weight is set
/// per span through the `wght` axis.
const FONTS: [(&str, &[u8]); 2] = [
    ("text-vf", include_bytes!("../assets/fonts/Manrope-VF.ttf")),
    ("display-vf", include_bytes!("../assets/fonts/Sora-VF.ttf")),
];

impl F {
    fn display(self) -> bool {
        matches!(self, F::D700 | F::D800)
    }

    fn weight(self) -> f32 {
        match self {
            F::T400 => 400.0,
            F::T500 => 500.0,
            F::T600 => 600.0,
            F::T700 | F::D700 => 700.0,
            F::D800 => 800.0,
        }
    }

    /// Advance of the font's tabular figures, in em. It is the same at every weight.
    fn tabular_cell(self) -> f32 {
        if self.display() { 0.676 } else { 0.62 }
    }

    fn format(self, size: f32, color: Color32, spacing: f32) -> TextFormat {
        let family = if self.display() { "display-vf" } else { "text-vf" };
        TextFormat {
            font_id: FontId::new(size, FontFamily::Name(family.into())),
            color,
            extra_letter_spacing: spacing,
            coords: VariationCoords::new([("wght", self.weight())]),
            ..Default::default()
        }
    }
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = egui::FontDefinitions::empty();
    for (name, bytes) in FONTS {
        let mut data = egui::FontData::from_static(bytes);
        data.tweak.hinting = Some(false); // the browser capture is unhinted
        defs.font_data.insert(name.to_owned(), Arc::new(data));
        defs.families
            .insert(FontFamily::Name(name.into()), vec![name.to_owned()]);
    }
    // egui's own text styles must resolve to something, even though no widget is used.
    defs.families
        .insert(FontFamily::Proportional, vec![FONTS[0].0.to_owned()]);
    defs.families
        .insert(FontFamily::Monospace, vec![FONTS[0].0.to_owned()]);
    ctx.set_fonts(defs);
}

/// Ascender and descender per em (hhea, which both fonts use): Manrope, then Sora.
const TEXT_METRICS: (f32, f32) = (1.066, 0.3);
const DISPLAY_METRICS: (f32, f32) = (0.97, 0.29);

struct Txt {
    job: LayoutJob,
    w: f32,
    base: f32,
}

struct Pen<'a> {
    /// Lays text out, drawn or not.
    ctx: &'a egui::Context,
    /// `None` to lay the panel out without drawing it.
    p: Option<&'a Painter>,
    o: egui::Vec2,
    /// One physical pixel, in points: what the browser draws for a 1px border.
    hair: f32,
    /// Physical pixels per point.
    ppp: f32,
}

impl Pen<'_> {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        pos2(x, y) + self.o
    }

    /// Where a browser puts the baseline inside a line box: half-leading plus the ascent. It
    /// rounds the ascent and the descent to physical pixels and rounds the half-leading down to
    /// one (measured at this screen's scale).
    fn baseline(&self, (ascender, descender): (f32, f32), size: f32, line_height: f32) -> f32 {
        let a = (ascender * size * self.ppp).round();
        let d = (descender * size * self.ppp).round();
        (((line_height * self.ppp - (a + d)) / 2.0).floor() + a) / self.ppp
    }

    /// Baseline of Manrope at `size` in a line box of `line_height`.
    fn bl(&self, size: f32, line_height: f32) -> f32 {
        self.baseline(TEXT_METRICS, size, line_height)
    }

    /// The same for Sora.
    fn bl_display(&self, size: f32, line_height: f32) -> f32 {
        self.baseline(DISPLAY_METRICS, size, line_height)
    }

    /// To the nearest physical pixel, a half going up: where a browser puts the edge of a box.
    /// (egui rounds boxes too, but in f32 a half lands on either side.)
    fn snap(&self, v: f32) -> f32 {
        (v * self.ppp + 0.001).round() / self.ppp
    }

    fn snap_x(&self, x: f32) -> f32 {
        self.snap(x + self.o.x) - self.o.x
    }

    fn snap_y(&self, y: f32) -> f32 {
        self.snap(y + self.o.y) - self.o.y
    }

    fn r(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        let (min, max) = (self.at(x, y), self.at(x + w, y + h));
        Rect::from_min_max(
            pos2(self.snap(min.x), self.snap(min.y)),
            pos2(self.snap(max.x), self.snap(max.y)),
        )
    }

    fn add(&self, shape: impl Into<Shape>) {
        if let Some(p) = self.p {
            p.add(shape);
        }
    }

    fn layout(&self, job: LayoutJob) -> Arc<egui::Galley> {
        self.ctx.fonts_mut(|fonts| fonts.layout_job(job))
    }

    fn fill(&self, x: f32, y: f32, w: f32, h: f32, radius: u8, c: Color32) {
        self.add(Shape::rect_filled(self.r(x, y, w, h), radius, c));
    }

    /// A filled circle of diameter `d` with its centre at `x`, `y`.
    /// A picture of the panel: the same texture, near-black and half clear, one step off in the
    /// eight directions under it, so that it keeps its edge on any ground.
    fn picture(&self, texture: eframe::egui::TextureId, rect: Rect) {
        let whole = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        let step = if rect.width() >= 30.0 { 1.5 } else { 1.0 };
        let shade = Color32::from_rgba_unmultiplied(6, 6, 10, 125);
        for (dx, dy) in [(-1.0, -1.0), (0.0, -1.0), (1.0, -1.0), (-1.0, 0.0), (1.0, 0.0), (-1.0, 1.0), (0.0, 1.0), (1.0, 1.0)] {
            self.add(Shape::image(texture, rect.translate(vec2(dx * step, dy * step)), whole, shade));
        }
        self.add(Shape::image(texture, rect, whole, Color32::WHITE));
    }

    fn dot(&self, x: f32, y: f32, d: f32, c: Color32) {
        self.add(Shape::circle_filled(self.at(x, y), d / 2.0, c));
    }

    /// Top of Manrope's content area above the baseline: what an inline box with a border wraps.
    fn ascent(&self, size: f32) -> f32 {
        (TEXT_METRICS.0 * size * self.ppp).round() / self.ppp
    }

    fn descent(&self, size: f32) -> f32 {
        (TEXT_METRICS.1 * size * self.ppp).round() / self.ppp
    }

    /// A box with a 1px border drawn inside it.
    fn boxed(&self, rect: Rect, radius: u8, fill: Color32, border: Color32) {
        self.add(egui::epaint::RectShape::new(
            rect,
            radius,
            fill,
            Stroke::new(self.hair, border),
            StrokeKind::Inside,
        ));
    }

    fn spans(&self, spans: &[(&str, F, Color32)], size: f32, spacing: f32) -> Txt {
        let mut job = LayoutJob::default();
        job.wrap.max_width = f32::INFINITY;
        job.round_output_to_gui = false;
        job.keep_trailing_whitespace = true;
        // font-variant-numeric: tabular-nums. egui 0.37 shapes with default OpenType features
        // only, so `tnum` cannot be asked for. Emulated: each figure is centred in the font's
        // tabular cell, as its own section so that it is not kerned. Both fonts' tabular figures
        // are their proportional ones moved in a wider or narrower cell, to within 0.01 em.
        let mut carry = 0.0; // right half of the previous figure's padding
        let mut gap = 0.0; // letter-spacing owed between two sections (egui adds it only inside one)
        let mut lead = 0.0; // every leading space handed to egui
        let mut inner_gaps = 0; // egui's own letter-spacing: between the characters of a section
        let mut push = |job: &mut LayoutJob, text: &str, leading: f32, format: TextFormat| {
            job.append(text, leading, format);
            lead += leading;
            inner_gaps += text.chars().count() - 1;
        };
        for (s, f, color) in spans {
            let format = f.format(size, *color, spacing);
            let cell = f.tabular_cell() * size;
            let mut run_start = 0;
            for (i, c) in s.char_indices() {
                if !c.is_ascii_digit() {
                    continue;
                }
                if run_start < i {
                    push(&mut job, &s[run_start..i], carry + gap, format.clone());
                    (carry, gap) = (0.0, spacing);
                }
                let pad = (cell - self.advance(c, &format)) / 2.0;
                push(&mut job, &s[i..i + 1], carry + gap + pad, format.clone());
                (carry, gap) = (pad, spacing);
                run_start = i + 1;
            }
            if run_start < s.len() {
                push(&mut job, &s[run_start..], carry + gap, format.clone());
                (carry, gap) = (0.0, spacing);
            }
        }
        let g = self.layout(job.clone());
        let (w, base) = match g.rows.first() {
            Some(row) => {
                // The width is the sum of the advances, as in a browser. A glyph's own `pos.x` is
                // a whole physical pixel (the rest is in its subpixel bitmap), so it cannot be
                // read off the last glyph.
                let glyphs = &row.row.glyphs;
                let advances: f32 = glyphs.iter().map(|gl| gl.advance_width).sum();
                (
                    lead + advances + spacing * inner_gaps as f32 + carry + gap,
                    row.pos.y + glyphs.first().map_or(0.0, |gl| gl.pos.y),
                )
            }
            None => (0.0, 0.0),
        };
        Txt { job, w, base }
    }

    /// Advance width of one character on its own.
    fn advance(&self, c: char, format: &TextFormat) -> f32 {
        let mut job = LayoutJob::default();
        job.wrap.max_width = f32::INFINITY;
        job.round_output_to_gui = false;
        job.append(c.encode_utf8(&mut [0; 4]), 0.0, format.clone());
        let g = self.layout(job);
        g.rows
            .first()
            .and_then(|row| row.row.glyphs.first())
            .map_or(0.0, |gl| gl.advance_width)
    }

    fn t(&self, s: &str, f: F, size: f32, color: Color32) -> Txt {
        self.spans(&[(s, f, color)], size, 0.0)
    }

    /// The text, cut short with an ellipsis when it is wider than `max`.
    fn fitted(&self, s: &str, f: F, size: f32, color: Color32, spacing: f32, max: f32) -> Txt {
        let whole = self.spans(&[(s, f, color)], size, spacing);
        if whole.w <= max {
            return whole;
        }
        let mut kept: Vec<char> = s.chars().collect();
        while kept.pop().is_some() {
            let cut = format!("{}\u{2026}", kept.iter().collect::<String>().trim_end());
            let t = self.spans(&[(&cut, f, color)], size, spacing);
            if t.w <= max {
                return t;
            }
        }
        whole
    }

    /// The sentence broken at its spaces into lines no wider than `max`.
    fn wrapped(&self, s: &str, f: F, size: f32, color: Color32, max: f32) -> Vec<Txt> {
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in s.split_whitespace() {
            let longer = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
            if !line.is_empty() && self.t(&longer, f, size, color).w > max {
                lines.push(self.fitted(&line, f, size, color, 0.0, max));
                line = word.to_owned();
            } else {
                line = longer;
            }
        }
        if !line.is_empty() {
            lines.push(self.fitted(&line, f, size, color, 0.0, max));
        }
        lines
    }

    /// Draw with the left edge at `x` and the baseline at `baseline`.
    fn draw(&self, t: &Txt, x: f32, baseline: f32) {
        let Some(p) = self.p else {
            return;
        };
        // egui puts a galley on a whole physical pixel; a browser starts a line of text anywhere.
        // The fraction goes into the layout as leading space, where egui honours it to a quarter
        // of a pixel with its subpixel glyph bitmaps.
        let px = (x + self.o.x) * self.ppp;
        let whole = px.floor();
        let mut job = t.job.clone();
        if let Some(first) = job.sections.first_mut() {
            first.leading_space += (px - whole) / self.ppp;
        }
        // The baseline goes on a whole pixel, like a box's edge (`t.base` is whole pixels already).
        let at = pos2(whole / self.ppp, self.snap(baseline + self.o.y) - t.base);
        p.galley(at, self.layout(job), Color32::PLACEHOLDER);
    }

    fn draw_right(&self, t: &Txt, right: f32, baseline: f32) {
        self.draw(t, right - t.w, baseline);
    }

    /// `text` with its currency words (`div`, `c`, `chaos`) as icons once those are there.
    fn rich(&self, text: &str, f: F, font: f32, color: Color32, icon: f32) -> Rich {
        let mut parts = Vec::new();
        let mut w = 0.0;
        let mut buffer = String::new();
        let flush = |buffer: &mut String, parts: &mut Vec<Part>, w: &mut f32| {
            if !buffer.is_empty() {
                let t = self.t(buffer, f, font, color);
                *w += t.w + if matches!(parts.last(), Some(Part::Icon(_))) { ICON_GAP } else { 0.0 };
                parts.push(Part::Text(t));
                buffer.clear();
            }
        };
        for word in text.split(' ') {
            let texture = matches!(word, "div" | "divine" | "c" | "chaos").then(|| icons::get(self.ctx, word)).flatten();
            match texture {
                Some(texture) => {
                    flush(&mut buffer, &mut parts, &mut w);
                    w += if parts.is_empty() { 0.0 } else { ICON_GAP };
                    parts.push(Part::Icon(texture));
                    w += icon;
                }
                None => {
                    if !buffer.is_empty() {
                        buffer.push(' ');
                    }
                    buffer.push_str(word);
                }
            }
        }
        flush(&mut buffer, &mut parts, &mut w);
        Rich { parts, w, icon, font }
    }

    /// One currency: its icon once there, else `fallback`, its name as text.
    fn unit(&self, id: &str, fallback: Txt, font: f32, icon: f32) -> Rich {
        match icons::get(self.ctx, id) {
            Some(texture) => Rich { parts: vec![Part::Icon(texture)], w: icon, icon, font },
            None => Rich { w: fallback.w, parts: vec![Part::Text(fallback)], icon, font },
        }
    }

    fn draw_rich(&self, r: &Rich, x: f32, baseline: f32) {
        let mut x = x;
        for (at, part) in r.parts.iter().enumerate() {
            match part {
                Part::Text(t) => {
                    if at > 0 && matches!(r.parts[at - 1], Part::Icon(_)) {
                        x += ICON_GAP;
                    }
                    self.draw(t, x, baseline);
                    x += t.w;
                }
                Part::Icon(texture) => {
                    if at > 0 {
                        x += ICON_GAP;
                    }
                    // Centred on the digits of the line.
                    let rect = Rect::from_center_size(self.at(x + r.icon / 2.0, baseline - 0.34 * r.font), vec2(r.icon, r.icon));
                    self.picture(*texture, rect);
                    x += r.icon;
                }
            }
        }
    }

    fn draw_rich_right(&self, r: &Rich, right: f32, baseline: f32) {
        self.draw_rich(r, right - r.w, baseline);
    }
}

/// Space between an icon and the text beside it.
const ICON_GAP: f32 = 3.0;

enum Part {
    Text(Txt),
    Icon(egui::TextureId),
}

/// A line of text some words of which are icons.
struct Rich {
    parts: Vec<Part>,
    w: f32,
    icon: f32,
    font: f32,
}

/// Something of the panel that can be clicked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zone {
    /// In the painter's coordinates.
    pub rect: Rect,
    /// What a click in it is.
    pub action: Action,
}

/// Whether `at` is in `rect`: its right and bottom edges belong to what comes next, so that two
/// rows one under the other share no point.
fn inside(rect: Rect, at: Pos2) -> bool {
    rect.min.x <= at.x && at.x < rect.max.x && rect.min.y <= at.y && at.y < rect.max.y
}

/// What a click at `at` is: the action of the zone under it, if any.
pub fn hit(zones: &[Zone], at: Pos2) -> Option<Action> {
    zones.iter().find(|zone| inside(zone.rect, at)).map(|zone| zone.action)
}

/// Whether `at` is on the "Any N" chip: the wheel over it steps N.
pub fn over_any(zones: &[Zone], at: Pos2) -> bool {
    zones.iter().any(|zone| zone.action == Action::ToggleAny && inside(zone.rect, at))
}

/// The notches a turn of the wheel over the "Any N" chip is: one each way, whatever the turn.
pub fn any_notches(lines: f32) -> i32 {
    if lines > 0.0 { 1 } else if lines < 0.0 { -1 } else { 0 }
}

/// The pin of a setting: a click on it pins the setting, or unpins it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PinZone {
    /// In the painter's coordinates.
    pub rect: Rect,
    pub setting: Setting,
}

/// A chip that has a menu: a right click anywhere on it opens the menu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuZone {
    /// In the painter's coordinates.
    pub rect: Rect,
    pub chip: Chip,
}

/// The chip at `at` that has a menu, if any.
pub fn chip_at(chips: &[MenuZone], at: Pos2) -> Option<Chip> {
    chips.iter().find(|zone| inside(zone.rect, at)).map(|zone| zone.chip)
}

/// What a right click is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// On this chip: its menu opens.
    Open(Chip),
    /// Where there is no menu to open: the one that is open closes.
    Elsewhere,
}

/// What a right click at `at` is, with the open menu at `menu` if one is open. Inside the menu
/// it is nothing.
pub fn context_at(chips: &[MenuZone], menu: Option<Rect>, at: Pos2) -> Option<Context> {
    if menu.is_some_and(|menu| inside(menu, at)) {
        return None;
    }
    Some(chip_at(chips, at).map_or(Context::Elsewhere, Context::Open))
}

/// What the panel shows that is no part of the check: which field has the caret, what is pinned,
/// the chip whose menu is open, and the slider that is held.
#[derive(Clone, Copy)]
pub struct Face<'a> {
    pub fields: &'a Fields,
    pub pins: Config,
    pub menu: Option<Chip>,
    pub drag: Option<Drag>,
}

/// A number field of the panel: one of the two of a searched row, or the number of a chip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumField {
    /// In the painter's coordinates.
    pub rect: Rect,
    pub target: Target,
}

/// The field at `at`, if any.
pub fn field_at(fields: &[NumField], at: Pos2) -> Option<Target> {
    fields.iter().find(|field| inside(field.rect, at)).map(|field| field.target)
}

/// Where the primary button went down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// In this field: it is to be typed in.
    Field(Target),
    /// In the menu that is open: on one of its lines, or beside them.
    Menu,
    /// Anywhere else.
    Elsewhere,
}

/// Where a press at `at` is.
pub fn press_at(fields: &[NumField], menu: Option<Rect>, at: Pos2) -> Press {
    match field_at(fields, at) {
        Some(target) => Press::Field(target),
        None if menu.is_some_and(|menu| inside(menu, at)) => Press::Menu,
        None => Press::Elsewhere,
    }
}

/// The slider of a searched mod, where the layout put it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Track {
    /// What answers the pointer, in the painter's coordinates: the rail and the room of its
    /// handle above and under it. No zone and no field shares a point with it.
    pub zone: Rect,
    /// The filter whose minimum it sets.
    pub filter: usize,
    pub scale: Scale,
    /// Where the middle of the handle is at the low end of the range and at the high end.
    pub left: f32,
    pub right: f32,
}

/// The track at `at`, if any.
pub fn track_at(tracks: &[Track], at: Pos2) -> Option<&Track> {
    tracks.iter().find(|track| inside(track.zone, at))
}

/// What the pointer did in a frame, as far as a slider is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Hand {
    /// The primary button went down in this frame, and where.
    pub pressed: Option<Pos2>,
    /// Where the pointer is now, or was last seen.
    pub at: Option<Pos2>,
    /// The primary button is down at the end of the frame.
    pub down: bool,
    /// A release of this frame ended a double click.
    pub double: bool,
}

/// One frame of the sliders. `held` is the slider that was held when the frame began. Returns
/// the slider held when it ends, and the action of the release, if the button was released.
///
/// A press on a track holds its slider at the value there. While it is held it follows the
/// pointer sideways wherever the pointer is, on its track or far above or under it. The release
/// is the minimum set to the value held; the release that ends a double click is the minimum
/// set to the high end. A press anywhere else holds nothing. A slider whose track is no longer
/// laid out (its row was switched off, the item was read again) is let go, and nothing is set.
pub fn slide(held: Option<Drag>, tracks: &[Track], hand: Hand) -> (Option<Drag>, Option<Action>) {
    let pressed = hand.pressed.and_then(|at| track_at(tracks, at).map(|track| Drag::press(track.filter, track.scale, (track.left, track.right), at.x)));
    let held = pressed.or(held).filter(|drag| tracks.iter().any(|track| track.filter == drag.filter()));
    let Some(drag) = held else {
        return (None, None);
    };
    let drag = hand.at.map_or(drag, |at| drag.follow(at.x));
    if hand.down { (Some(drag), None) } else { (None, Some(drag.release(hand.double))) }
}

/// So many lines of the wheel, as its event gives them: up is more. A wheel that counts in
/// points is taken to turn a line every 40 of them, and a page is a line.
fn lines(unit: egui::MouseWheelUnit, delta: egui::Vec2) -> f32 {
    match unit {
        egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => delta.y,
        egui::MouseWheelUnit::Point => delta.y / 40.0,
    }
}

/// How many pips the request budget is drawn with.
pub const PIPS: usize = 3;

/// The pips of the budget: one filled for each search left, up to all of them.
pub fn pips(left: u32) -> [bool; PIPS] {
    std::array::from_fn(|pip| (pip as u32) < left)
}

/// What a layout pass reports back.
pub struct Painted {
    /// The plate's outer height, border included, in points.
    pub height: f32,
    /// What can be clicked, in the order drawn. While a menu is open: its lines and nothing else.
    pub zones: Vec<Zone>,
    /// The number fields, in the order Tab goes through them. None while a menu is open.
    pub fields: Vec<NumField>,
    /// The pins of the settings. None while a menu is open.
    pub pins: Vec<PinZone>,
    /// The chips a right click opens a menu on.
    pub chips: Vec<MenuZone>,
    /// The sliders. None while a menu is open.
    pub tracks: Vec<Track>,
    /// Where the open menu is.
    pub menu: Option<Rect>,
}

/// What the panel reports of a frame.
pub struct Shown {
    /// What the player clicked, in order.
    pub actions: Vec<Action>,
    /// What could be clicked.
    pub zones: Vec<Zone>,
    /// Where the number fields were.
    pub fields: Vec<NumField>,
    /// The primary button went down in this frame, and where.
    pub press: Option<Press>,
    /// The wheel turned over a field in this frame, by so many lines: up is more.
    pub wheel: Option<(Target, f32)>,
    /// The same over the "Any N" chip.
    pub any_wheel: Option<f32>,
    /// Where the pins were, and the ones the player clicked.
    pub pins: Vec<PinZone>,
    pub pinned: Vec<Setting>,
    /// The chips that have a menu, and what a right click of this frame is.
    pub chips: Vec<MenuZone>,
    pub context: Option<Context>,
    /// Where the open menu was drawn. `None` although one is said to be open: its chip is gone.
    pub menu: Option<Rect>,
    /// Where the sliders were.
    pub tracks: Vec<Track>,
    /// The slider that is held once this frame is over.
    pub drag: Option<Drag>,
    /// A slider was released in this frame: what that is.
    pub slid: Option<Action>,
}

/// A budget for laying the panel out when the real one does not matter.
const SOME_BUDGET: Budget = Budget { left: PIPS as u32, wait_s: 0 };

/// The plate's outer height for `view`, in points, without drawing anything. Needs the fonts, so
/// not before the first frame.
pub fn measure(ctx: &egui::Context, view: &View, now: i64) -> f32 {
    // Neither a pin, nor a menu, nor a slider changes the height.
    let face = Face { fields: &Fields::new(), pins: Config::default(), menu: None, drag: None };
    lay(ctx, None, Pos2::ZERO, view, &face, now, 0.0, SOME_BUDGET, None).height
}

/// Paints the plate at the top-left corner of `ui` and reports what the player did in this frame.
/// `face` says which number field is being typed in, what is pinned and which menu is open.
/// `now`, in seconds since 1970, is what the age of a listing is counted from. `beat`, in
/// seconds, is where the bars of a view that is loading are in their pulse: it is for the caller
/// to repaint while they are to move. `budget` is how many searches may be sent now.
pub fn show(ui: &mut egui::Ui, view: &View, face: &Face<'_>, now: i64, beat: f32, budget: Budget) -> Shown {
    // Not clipped to the screen egui knows: when a view made the window taller in this very
    // pass, egui learns the new size only with the next one.
    let painter = ui.painter().with_clip_rect(Rect::EVERYTHING);
    let pointer = ui.input(|input| input.pointer.hover_pos());
    let Painted { zones, fields, pins, chips, tracks, menu, .. } =
        lay(ui.ctx(), Some(&painter), ui.max_rect().min, view, face, now, beat, budget, pointer);

    let mut actions = Vec::new();
    // The lines of a menu are not the zones that were there before it opened: a press on one
    // and a release on the other is no click.
    let kind = if menu.is_some() { "line" } else { "zone" };
    for (index, zone) in zones.iter().enumerate() {
        // egui's own notion of a click: pressed and released in the same zone, without a drag.
        // Not one egui gives its own keyboard focus to: Tab and Enter are for the fields.
        if ui.interact(zone.rect, ui.scope_id().with((kind, index)), egui::Sense::CLICK).clicked() {
            actions.push(zone.action);
        }
    }
    let mut pinned = Vec::new();
    for (index, pin) in pins.iter().enumerate() {
        if ui.interact(pin.rect, ui.scope_id().with(("pin", index)), egui::Sense::CLICK).clicked() {
            pinned.push(pin.setting);
        }
    }
    for track in &tracks {
        // A thing of its own for egui, which gives a press to the nearest thing that can be
        // clicked: a press on a track is for no zone beside it.
        ui.interact(track.zone, ui.scope_id().with(("track", track.filter)), egui::Sense::CLICK);
    }
    let over = pointer.and_then(|at| field_at(&fields, at));
    // The wheel over a track is the wheel over the minimum it sets.
    let over_track = pointer.and_then(|at| track_at(&tracks, at)).map(|track| Target::min(track.filter));
    if face.drag.is_some() || (over.is_none() && over_track.is_some()) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    } else if over.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    } else if pointer.is_some_and(|at| hit(&zones, at).is_some() || pins.iter().any(|pin| inside(pin.rect, at))) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let hand = ui.input(|input| {
        let pointer = &input.pointer;
        Hand {
            pressed: pointer.primary_pressed().then(|| pointer.press_origin().or(pointer.interact_pos())).flatten(),
            at: pointer.interact_pos(),
            down: pointer.primary_down(),
            double: pointer.button_double_clicked(egui::PointerButton::Primary),
        }
    });
    let (drag, slid) = slide(face.drag, &tracks, hand);
    let (pressed, turned, context) = ui.input(|input| {
        let pressed = input.pointer.primary_pressed().then(|| input.pointer.interact_pos()).flatten();
        // The secondary button pressed and released without a drag, as a browser's `contextmenu`.
        let context = input.pointer.secondary_clicked().then(|| input.pointer.interact_pos()).flatten();
        let turned: f32 = input
            .events
            .iter()
            .map(|event| match event {
                egui::Event::MouseWheel { unit, delta, .. } => lines(*unit, *delta),
                _ => 0.0,
            })
            .sum();
        (pressed, turned, context)
    });
    let press = pressed.map(|at| press_at(&fields, menu, at));
    // A click on the chip of the uniques opens its menu, as a right click does.
    let context = context.or_else(|| {
        let at = ui.input(|input| input.pointer.primary_clicked().then(|| input.pointer.interact_pos()).flatten())?;
        chip_at(&chips, at).filter(|chip| *chip == Chip::Unique).map(|_| at)
    });
    let context = context.and_then(|at| context_at(&chips, menu, at));
    // The wheel anywhere else does nothing.
    let wheel = over.or(over_track).filter(|_| turned != 0.0).map(|target| (target, turned));
    let any_wheel = pointer.filter(|at| over_any(&zones, *at)).filter(|_| turned != 0.0).map(|_| turned);
    Shown { actions, zones, fields, press, wheel, any_wheel, pins, pinned, chips, context, menu, tracks, drag, slid }
}

/// Sizes of one layout pass.
struct Frame {
    /// --gutter: everything starts 16px from the plate's edge
    left: f32,
    right: f32,
}

/// A chip that has a menu, where it was drawn: what its menu hangs from. In CSS pixels inside
/// the plate's padding box.
#[derive(Clone, Copy)]
struct Anchor {
    chip: Chip,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    /// The menu opens above the chip (`.pop-up`), not under it.
    upward: bool,
}

/// What a layout pass knows of the pointer, and where it notes what can be clicked.
struct Touch {
    /// Where the pointer is, in the painter's coordinates, when it is over the panel and no menu
    /// is open: under a menu nothing answers it.
    pointer: Option<Pos2>,
    zones: Vec<Zone>,
    /// The zones that are only there while a slider is shown, kept apart to go after all the
    /// others: the place of every other zone in the list, which is what egui knows it by from
    /// one frame to the next, is then the same with a slider and without.
    late: Vec<Zone>,
    fields: Vec<NumField>,
    pins: Vec<PinZone>,
    chips: Vec<MenuZone>,
    anchors: Vec<Anchor>,
    tracks: Vec<Track>,
    /// What the pointer reads when it rests on a mark: its rectangle and its sentence.
    tips: Vec<(Rect, String)>,
    /// The text of a mod cut short, shown whole over its rows while the pointer is on it.
    unfolds: Vec<Unfold>,
}

impl Touch {
    fn over(&self, rect: Rect) -> bool {
        self.pointer.is_some_and(|at| inside(rect, at))
    }

    fn zone(&mut self, rect: Rect, action: Action) {
        self.zones.push(Zone { rect, action });
    }

    /// A chip that has a menu, drawn in `rect`, which is `width` by `height` at `x`, `y`.
    fn chip(&mut self, pen: &Pen<'_>, chip: Chip, (x, y, width, height): (f32, f32, f32, f32), upward: bool) {
        self.chips.push(MenuZone { rect: pen.r(x, y, width, height), chip });
        self.anchors.push(Anchor { chip, x, y, width, height, upward });
    }
}

#[allow(clippy::too_many_arguments)]
fn lay(
    ctx: &egui::Context,
    p: Option<&Painter>,
    origin: Pos2,
    view: &View,
    face: &Face<'_>,
    now: i64,
    beat: f32,
    budget: Budget,
    pointer: Option<Pos2>,
) -> Painted {
    let ppp = ctx.pixels_per_point();
    let hair = (ppp.floor().max(1.0)) / ppp;
    let pen = Pen { ctx, p, o: origin.to_vec2() + vec2(hair, hair), hair, ppp };
    let frame = Frame { left: 16.0, right: WIDTH - 2.0 * hair - 16.0 };
    let fields = face.fields;
    // An unidentified unique waiting for its pick shows the picker, not a menu of names.
    let picking = view.check().is_some_and(picking);
    let menu_chip = face.menu.or_else(|| view.check().filter(|check| check.candidates_open && !picking).map(|_| Chip::Unique));
    let mut touch = Touch {
        // Nor while a slider is held: the pointer is that slider's, wherever it goes.
        pointer: pointer.filter(|_| menu_chip.is_none() && face.drag.is_none()),
        zones: Vec::new(),
        late: Vec::new(),
        fields: Vec::new(),
        pins: Vec::new(),
        chips: Vec::new(),
        anchors: Vec::new(),
        tracks: Vec::new(),
        tips: Vec::new(),
        unfolds: Vec::new(),
    };

    // The plate's height is known only once the rows are laid out: reserve its slots.
    let slots = p.map(|p| (p.add(Shape::Noop), p.add(Shape::Noop), p.add(Shape::Noop), p.add(Shape::Noop)));
    // The quick menu with its saved searches open: two plates, the gap between them clear.
    let mut split = None;

    let mut y = match view {
        View::Message(text) => sentence(&pen, &frame, text, 12.0) + 2.0,
        View::Maps(maps) => map_body(&pen, &frame, maps, &mut touch),
        View::Launcher => launcher_body(&pen, &frame, false, &mut touch),
        View::Searches(titles) => {
            let bottom = launcher_body(&pen, &frame, true, &mut touch) + 14.0;
            split = Some((bottom, bottom + PLATE_GAP));
            searches_body(&pen, &frame, bottom + PLATE_GAP, titles, &mut touch)
        }
        View::Item(check) => {
            let Outcome { league, listings, loading, problem, .. } = &check.result;
            let mut y = head(&pen, &frame, &check.item, league.as_deref(), state::rate_text(check.result.quote.as_ref()).as_deref(), (check.plan == Plan::Stack).then(|| quote::name_of(&check.item)));
            let searchable = check.searchable();
            if picking {
                y = picker(&pen, &frame, y, check, &mut touch);
            } else if check.plan == Plan::Stack {
                // Priced from the cached list alone: nothing is searched, nothing to edit.
                y = stack_body(&pen, &frame, y, check, now);
            } else if check.in_bulk() {
                // A map on the bulk exchange: its chips (Bulk turns back to listings), then the
                // offers, or why there are none.
                y = option_chips(&pen, &frame, y, check, fields, &mut touch);
                if *loading {
                    y = sentence(&pen, &frame, "Asking the bulk exchange", y + 6.0) + 2.0;
                } else if let Some(problem) = problem.as_ref().filter(|_| check.result.bulk.is_empty()) {
                    y = sentence(&pen, &frame, problem, y + 6.0) + 2.0;
                }
                for bulk in &check.result.bulk {
                    y = bulk_offers(&pen, &frame, y, bulk, check.result.quote.as_ref());
                }
            } else if listings.is_empty() && !loading {
                // Nothing was priced: the reason takes the place of the price.
                if let Some(problem) = problem {
                    y = sentence(&pen, &frame, problem, y + 6.0) + 2.0;
                }
                // What can be searched again keeps its two ways and its chips: the search may be
                // the one to change.
                y = ways(&pen, &frame, y, check, &mut touch);
                let chips = option_chips(&pen, &frame, y, check, fields, &mut touch);
                let chipped = chips > y;
                y = chips;
                if !check.rows.is_empty() {
                    // Under the chips their own padding is the gap; before the settings and
                    // their rule, the rows keep theirs.
                    y = mod_rows(&pen, &frame, if chipped { y } else { y + 4.0 }, check, face, &mut touch);
                    if !searchable {
                        y -= 8.0;
                    }
                }
            } else {
                y = figure(&pen, &frame, y, check, beat, now);
                y = ninja_line(&pen, &frame, y, check, now);
                y = ways(&pen, &frame, y, check, &mut touch);
                y = option_chips(&pen, &frame, y, check, fields, &mut touch);
                y = mod_rows(&pen, &frame, y, check, face, &mut touch);
                // What comes under the listings: the bars, the reason the list stops short, the
                // footer and the plate's border. A plate that would be too tall lists fewer: the
                // two ways, the chips and the rows above are never what is cut.
                // The listings a pure tier sets aside are told of under the table.
                let kept = check.kept_listings();
                let aside = check.other_tier_count();
                let aside_text = format!("{aside} hidden (other tier)");
                let aside_room = if aside > 0 { 2.0 + sentence_height(&pen, &frame, &aside_text) + 2.0 } else { 0.0 };
                let reason = aside_room + problem.as_ref().map_or(0.0, |problem| 2.0 + sentence_height(&pen, &frame, problem) + 2.0);
                let under = if *loading { 2.0 * ARRIVING } else { 0.0 } + TABLE_FOOT + reason + footer(searchable) + 2.0 * hair;
                let fit = fitting(MAX_HEIGHT - (y + TABLE_HEAD) - under, kept.len());
                // Listings that no longer answer the filters step back.
                let shown = if check.stale { STALE } else { 1.0 };
                hidden_switch(&pen, &frame, y, check, &mut touch);
                y = table(&pen, &frame, y, &kept[..fit], check.item.corrupted, check.item.rarity == Rarity::Gem, *loading, now, beat, shown);
                if aside > 0 {
                    y = sentence(&pen, &frame, &aside_text, y + 2.0) + 2.0;
                }
                if let Some(problem) = problem {
                    y = sentence(&pen, &frame, problem, y + 2.0) + 2.0;
                }
            }
            y
        }
    };

    // ---- footer (`footer` in all) -----------------------------------------------------------
    // The quick menu and the saved searches are small floating panels: no footer, no "Close".
    let floating = matches!(view, View::Launcher | View::Searches(_));
    if floating {
        y += 14.0;
    } else {
    match view.check().filter(|check| check.searchable() && !picking) {
        // `.rule`, then `.defaults`: the settings of the search, each with its pin.
        Some(check) => {
            pen.fill(frame.left, y, frame.right - frame.left, 1.0, 0, tone_hair());
            y += 1.0 + SETTINGS_PAD;
            settings(&pen, &frame, y, check, face.pins, &mut touch);
            y += TOKEN + SETTINGS_PAD;
        }
        // `.foot` with the compact plate's padding-top.
        None => y += 8.0,
    }
    // While a unique is to be picked, only "Close".
    actions(&pen, &frame, y, view.check().filter(|_| !picking), budget, &mut touch);
    y += 30.0 + 14.0;
    }
    touch.zones.append(&mut touch.late);

    // ---- the plate itself, behind everything ------------------------------------------
    // Its bottom edge goes on a whole physical pixel, so that the window can be exactly as tall.
    let height = ((y + 2.0 * hair) * ppp - 0.001).ceil() / ppp;

    // ---- the open menu, over everything ------------------------------------------------
    let mut open = None;
    if let (Some(chip), Some(check)) = (menu_chip, view.check())
        && let Some(anchor) = touch.anchors.iter().find(|anchor| anchor.chip == chip)
    {
        let within = (WIDTH - 2.0 * hair, height - 2.0 * hair);
        let (rect, lines) = menu(&pen, anchor, &choices(check, chip), within, pointer);
        // Nothing under an open menu can be clicked or typed in: its lines only.
        touch.zones = lines;
        touch.fields.clear();
        touch.pins.clear();
        touch.tracks.clear();
        open = Some(rect);
    }
    if let (Some(chip), View::Maps(maps)) = (menu_chip, view)
        && let Some(anchor) = touch.anchors.iter().find(|anchor| anchor.chip == chip)
        && let Chip::MapMod(index) = chip
    {
        let tag = maps.mods.get(index).and_then(|row| row.tag);
        let lines = [
            Choice { label: "Danger", action: Action::MapTag(index, Some(Tag::Danger)), current: tag == Some(Tag::Danger) },
            Choice { label: "Warning", action: Action::MapTag(index, Some(Tag::Warning)), current: tag == Some(Tag::Warning) },
            Choice { label: "Good", action: Action::MapTag(index, Some(Tag::Good)), current: tag == Some(Tag::Good) },
            Choice { label: "Clear", action: Action::MapTag(index, None), current: tag.is_none() },
        ];
        let within = (WIDTH - 2.0 * hair, height - 2.0 * hair);
        let (rect, zones) = menu(&pen, anchor, &lines, within, pointer);
        touch.zones = zones;
        touch.fields.clear();
        touch.pins.clear();
        touch.tracks.clear();
        open = Some(rect);
    }
    // A danger on the map: a red bar on the plate's edge, an orange one for warnings only.
    if let View::Maps(maps) = view
        && let Some(tone) = maps_alarm(maps)
    {
        pen.fill(0.0, 16.0, 4.0, (height - 2.0 * hair - 32.0).max(0.0), 2, tone);
    }
    if let Some(unfold) = touch.unfolds.iter().find(|unfold| touch.over(unfold.rect)) {
        unfolded(&pen, unfold);
    }
    if let Some((rect, text)) = touch.tips.iter().find(|(rect, _)| touch.over(*rect)).cloned() {
        tooltip(&pen, rect, &text);
    }
    let plates = match split {
        Some((end, start)) => [
            Some(Rect::from_min_size(origin, vec2(WIDTH, end))),
            Some(Rect::from_min_size(origin + vec2(0.0, start), vec2(WIDTH, height - start))),
        ],
        None => [Some(Rect::from_min_size(origin, vec2(WIDTH, height))), None],
    };
    if let (Some(p), Some((shadow_a, plate_a, shadow_b, plate_b))) = (p, slots) {
        for (plate, (shadow_slot, plate_slot)) in plates.into_iter().zip([(shadow_a, plate_a), (shadow_b, plate_b)]) {
            let Some(plate) = plate else { continue };
            p.set(
                shadow_slot,
                Shape::rect_filled(
                    plate.translate(vec2(0.0, SHADOW)),
                    14,
                    Color32::from_rgba_unmultiplied(4, 7, 16, 179),
                ),
            );
            p.set(
                plate_slot,
                egui::epaint::RectShape::new(plate, 14, tone_plate(), Stroke::new(hair, tone_hair()), StrokeKind::Inside),
            );
        }
    }
    Painted { height, zones: touch.zones, fields: touch.fields, pins: touch.pins, chips: touch.chips, tracks: touch.tracks, menu: open }
}

/// An unidentified unique whose pick is still to come: the picker takes the place of the price.
fn picking(check: &Check) -> bool {
    check.picking()
}

/// The candidates as tiles: their picture over their name, four to a row.
fn picker(pen: &Pen<'_>, frame: &Frame, y: f32, check: &Check, touch: &mut Touch) -> f32 {
    const GAP: f32 = 4.0;
    const MAX_TILE: f32 = 132.0;
    let columns = check.candidates.len().clamp(1, 4);
    // Few candidates get a larger picture.
    let picture: f32 = if columns <= 3 { 84.0 } else { 64.0 };
    const LINE: f32 = 14.0;
    // A unique found in Standard only says so under its name.
    let legacy = |name: &str| zenith_data::is_legacy_unique(name);
    let tag = if check.candidates.iter().any(|name| legacy(name)) { LINE } else { 0.0 };
    let tile_h = 8.0 + picture + 2.0 + 2.0 * LINE + tag + 8.0;
    let width = frame.right - frame.left;
    let tile_w = ((width - GAP * (columns as f32 - 1.0)) / columns as f32).floor().min(MAX_TILE);
    let top = y + 6.0;
    for (index, name) in check.candidates.iter().enumerate() {
        let (column, row) = (index % columns, index / columns);
        // Each row is centred, the last one too when it is short.
        let in_row = (check.candidates.len() - row * columns).min(columns) as f32;
        let start = frame.left + (width - in_row * tile_w - GAP * (in_row - 1.0)) / 2.0;
        let (x, y) = (start + column as f32 * (tile_w + GAP), top + row as f32 * (tile_h + GAP));
        let rect = pen.r(x, y, tile_w, tile_h);
        let over = touch.over(rect);
        if over {
            pen.fill(x, y, tile_w, tile_h, 12, tone_raise());
        }
        let square = Rect::from_min_size(pen.at(x + (tile_w - picture) / 2.0, y + 8.0), vec2(picture, picture));
        let texture = check.candidate_icons.get(index).and_then(|url| url.as_deref()).and_then(|url| icons::get_url(pen.ctx, name, url));
        match texture {
            Some(texture) => pen.picture(texture, square),
            None => {
                // A soft round plate with the name's first letter.
                pen.add(Shape::circle_filled(square.center(), picture / 2.0 - 6.0, Color32::from_white_alpha(14)));
                let initial = pen.t(&name.chars().next().map(String::from).unwrap_or_default(), F::T600, 26.0, tone_mute());
                let at = square.center() - pen.o;
                pen.draw(&initial, at.x - initial.w / 2.0, at.y - 16.0 + pen.bl(26.0, 32.0));
            }
        }
        // The name over two lines at most, the last cut short.
        let colour = if over { tone_gold_hi() } else { tone_star() };
        let room = tile_w - 8.0;
        let mut rest = name.as_str();
        let mut cut = false;
        let mut drawn = 0;
        for line in 0..2 {
            if rest.is_empty() {
                break;
            }
            let last = line == 1;
            let mut taken = String::new();
            let mut words = rest.split_whitespace().peekable();
            while let Some(word) = words.peek() {
                let longer = if taken.is_empty() { (*word).to_owned() } else { format!("{taken} {word}") };
                if !taken.is_empty() && pen.t(&longer, F::T500, 11.0, colour).w > room {
                    break;
                }
                taken = longer;
                words.next();
            }
            let remainder = words.collect::<Vec<_>>().join(" ");
            // The last line holds the whole remainder, cut with an ellipsis; an only word too
            // wide is cut as well.
            let shown = if last { format!("{taken} {remainder}").trim().to_owned() } else { taken };
            let text = pen.fitted(&shown, F::T500, 11.0, colour, 0.0, room);
            cut |= text.w < pen.t(&shown, F::T500, 11.0, colour).w;
            pen.draw(&text, x + (tile_w - text.w) / 2.0, y + 8.0 + picture + 2.0 + line as f32 * LINE + pen.bl(11.0, LINE));
            drawn = line + 1;
            rest = if last { "" } else { name[name.len() - remainder.len()..].trim_start() };
            if !last && remainder.is_empty() {
                break;
            }
        }
        // Right under the name.
        if legacy(name) {
            let text = pen.t("Standard only", F::T700, 10.5, tone_ember());
            pen.draw(&text, x + (tile_w - text.w) / 2.0, y + 8.0 + picture + 2.0 + drawn as f32 * LINE + pen.bl(10.5, LINE));
        }
        if cut {
            touch.tips.push((rect, name.clone()));
        }
        touch.zone(rect, Action::PickUnique(Some(index)));
    }
    let rows = check.candidates.len().div_ceil(columns) as f32;
    top + rows * (tile_h + GAP) - GAP + 6.0
}

fn tone_danger() -> Color32 {
    Color32::from_rgb(226, 84, 84)
}

fn tone_warning() -> Color32 {
    Color32::from_rgb(240, 160, 64)
}

/// The colour of the worst tag of a map: red for a danger, orange for a warning, nothing else.
fn maps_alarm(maps: &MapView) -> Option<Color32> {
    if maps.danger_count() > 0 {
        Some(tone_danger())
    } else if maps.warning_count() > 0 {
        Some(tone_warning())
    } else {
        None
    }
}

fn tone_good() -> Color32 {
    Color32::from_rgb(104, 198, 128)
}

/// The mark of a tag in a 14px square at `x`, `y`: a drawn warning triangle with its bar and dot
/// for a danger, a tick for a good one.
fn tag_mark(pen: &Pen<'_>, x: f32, y: f32, tag: Tag) {
    match tag {
        Tag::Danger => {
            let tone = tone_danger();
            pen.add(Shape::convex_polygon(vec![pen.at(x + 7.0, y), pen.at(x + 14.0, y + 12.5), pen.at(x, y + 12.5)], tone, Stroke::NONE));
            let dark = tone_plate();
            pen.fill(x + 6.2, y + 4.4, 1.6, 4.0, 0, dark);
            pen.fill(x + 6.2, y + 9.4, 1.6, 1.6, 0, dark);
        }
        Tag::Warning => {
            let tone = tone_warning();
            pen.add(Shape::circle_filled(pen.at(x + 7.0, y + 6.5), 6.5, tone));
            let dark = tone_plate();
            pen.fill(x + 6.2, y + 3.0, 1.6, 4.2, 0, dark);
            pen.fill(x + 6.2, y + 8.2, 1.6, 1.6, 0, dark);
        }
        Tag::Good => {
            pen.add(Shape::line(vec![pen.at(x + 1.0, y + 7.0), pen.at(x + 5.0, y + 11.0), pen.at(x + 13.0, y + 2.0)], Stroke::new(2.0, tone_good())));
        }
    }
}

/// The view of a map: its name and tier, how it rewards, then each of its mods with the tag the
/// player gave it. A left click on a mod cycles its tag, a right click opens the menu of its tag. Returns the y under it.
fn map_body(pen: &Pen<'_>, frame: &Frame, maps: &MapView, touch: &mut Touch) -> f32 {
    let width = frame.right - frame.left;
    let mut y = 18.0;
    let tier = maps.tier.map(|tier| pen.t(&format!("T{tier}"), F::T600, 14.0, tone_gold()));
    let room = width - tier.as_ref().map_or(0.0, |tier| tier.w + 12.0);
    let name = pen.fitted(&maps.name, F::D700, 17.0, tone_star(), 0.0, room);
    pen.draw(&name, frame.left, y + pen.bl_display(17.0, 24.0));
    if let Some(tier) = &tier {
        pen.draw_right(tier, frame.right, y + pen.bl(14.0, 24.0));
    }
    y += 28.0;
    if let Some(line) = maps.danger_line() {
        let (mark, tone) = if maps.danger_count() > 0 { (Tag::Danger, tone_danger()) } else { (Tag::Warning, tone_warning()) };
        tag_mark(pen, frame.left, y + 3.0, mark);
        pen.draw(&pen.t(&line, F::T600, 13.0, tone), frame.left + 22.0, y + pen.bl(13.0, 20.0));
        y += 24.0;
    }
    if !maps.properties.is_empty() {
        y += 6.0;
        for (label, value) in &maps.properties {
            let base = y + pen.bl(13.0, 22.0);
            pen.draw(&pen.t(label, F::T400, 13.0, tone_mute()), frame.left, base);
            pen.draw_right(&pen.t(value, F::T600, 13.0, tone_star()), frame.right, base);
            y += 22.0;
        }
        y += 8.0;
    }
    if !maps.mods.is_empty() {
        pen.fill(frame.left, y, width, 1.0, 0, tone_hair());
        y += 9.0;
    }
    let text_x = frame.left + 22.0;
    for (index, row) in maps.mods.iter().enumerate() {
        let tone = match row.tag {
            Some(Tag::Danger) => tone_danger(),
            Some(Tag::Warning) => tone_warning(),
            Some(Tag::Good) => tone_good(),
            None => tone_star(),
        };
        let lines = pen.wrapped(&row.text, F::T400, 13.0, tone, frame.right - text_x);
        let height = 20.0 * lines.len().max(1) as f32 + 6.0;
        let zone = (frame.left - 8.0, y, width + 16.0, height);
        if let Some(tag) = row.tag {
            let wash = match tag {
                Tag::Danger => tone_danger(),
                Tag::Warning => tone_warning(),
                Tag::Good => tone_good(),
            };
            pen.fill(zone.0, zone.1, zone.2, zone.3, 6, Color32::from_rgba_unmultiplied(wash.r(), wash.g(), wash.b(), 26));
            tag_mark(pen, frame.left, y + 4.0 + 3.0, tag);
        }
        for (n, line) in lines.iter().enumerate() {
            pen.draw(line, text_x, y + 3.0 + 20.0 * n as f32 + pen.bl(13.0, 20.0));
        }
        // A left click cycles none, good, warning, danger; a right click opens the menu.
        let next = match row.tag {
            None => Some(Tag::Good),
            Some(Tag::Good) => Some(Tag::Warning),
            Some(Tag::Warning) => Some(Tag::Danger),
            Some(Tag::Danger) => None,
        };
        touch.zone(pen.r(zone.0, zone.1, zone.2, zone.3), Action::MapTag(index, next));
        touch.chip(pen, Chip::MapMod(index), zone, false);
        y += height;
    }
    y
}

/// The quick menu: one row of rounded buttons, an icon and a label each.
/// The clear gap between the quick menu and its saved searches, open under it.
const PLATE_GAP: f32 = 10.0;

/// The row of tools of the quick menu. `searches_open`: the button of the saved searches is lit,
/// as its widget is open under the row.
fn launcher_body(pen: &Pen<'_>, frame: &Frame, searches_open: bool, touch: &mut Touch) -> f32 {
    const PAD: f32 = 14.0;
    const ICON: f32 = 16.0;
    const HEIGHT: f32 = 34.0;
    let y = 14.0;
    let labels: Vec<Txt> = launcher::items().iter().map(|item| pen.t(item.label, F::T600, 13.0, tone_star())).collect();
    let widths: Vec<f32> = labels.iter().map(|label| PAD + ICON + 8.0 + label.w + PAD).collect();
    for (index, ((x, w), label)) in launcher::row(&widths, frame.left, frame.right, 8.0).into_iter().zip(&labels).enumerate() {
        let rect = pen.r(x, y, w, HEIGHT);
        let lit = searches_open && launcher::items()[index].target == launcher::Target::Searches;
        let over = touch.over(rect);
        let (fill, border) = match (over, lit) {
            (_, true) => (tone_raise(), tone_gold()),
            (true, false) => (tone_raise(), tone_line()),
            (false, false) => (Color32::TRANSPARENT, tone_hair()),
        };
        pen.boxed(rect, 17, fill, border);
        let tone = if over || lit { tone_gold_hi() } else { tone_gold() };
        launcher_icon(pen, launcher::items()[index].icon, x + PAD + ICON / 2.0, y + HEIGHT / 2.0, tone);
        pen.draw(label, x + PAD + ICON + 8.0, y + 7.0 + pen.bl(13.0, 20.0));
        touch.zone(rect, Action::Launch(index));
    }
    y + HEIGHT
}

/// A 16px picture, centred on `(cx, cy)`, drawn with a few strokes.
fn launcher_icon(pen: &Pen<'_>, icon: launcher::Icon, cx: f32, cy: f32, tone: Color32) {
    let stroke = Stroke::new(1.6, tone);
    let line = |a: (f32, f32), b: (f32, f32)| pen.add(Shape::line_segment([pen.at(cx + a.0, cy + a.1), pen.at(cx + b.0, cy + b.1)], stroke));
    match icon {
        launcher::Icon::Search => {
            pen.add(Shape::circle_stroke(pen.at(cx - 1.5, cy - 1.5), 5.0, stroke));
            line((2.2, 2.2), (7.0, 7.0));
        }
        launcher::Icon::Filter => {
            for (dy, knob) in [(-5.0, -2.5), (0.0, 3.0), (5.0, -1.0)] {
                line((-7.0, dy), (7.0, dy));
                pen.dot(cx + knob, cy + dy, 4.5, tone);
            }
        }
        launcher::Icon::Gear => {
            for turn in 0..8 {
                let a = turn as f32 * std::f32::consts::FRAC_PI_4;
                line((a.cos() * 5.4, a.sin() * 5.4), (a.cos() * 7.6, a.sin() * 7.6));
            }
            pen.add(Shape::circle_stroke(pen.at(cx, cy), 4.6, stroke));
            pen.dot(cx, cy, 3.0, tone);
        }
    }
}

/// The saved searches: a title line with the "edit" link, then a grid of buttons.
fn searches_body(pen: &Pen<'_>, frame: &Frame, top: f32, titles: &[String], touch: &mut Touch) -> f32 {
    const CELL: f32 = 32.0;
    const GAP: f32 = 6.0;
    let width = frame.right - frame.left;
    let mut y = top + 14.0;
    pen.draw(&pen.t("Stash searches", F::T600, 13.0, tone_mute()), frame.left, y + pen.bl(13.0, 20.0));
    let edit = pen.t("edit", F::T500, 12.0, tone_gold_hi());
    let link = pen.r(frame.right - edit.w - 16.0, y - 4.0, edit.w + 16.0, 28.0);
    let over = touch.over(link);
    pen.draw(&edit, frame.right - edit.w, y + pen.bl(12.0, 20.0));
    if over {
        pen.fill(frame.right - edit.w, y + 19.0, edit.w, 1.0, 0, tone_gold_hi());
    }
    touch.zone(link, Action::EditSearches);
    y += 28.0;
    if titles.is_empty() {
        return sentence(pen, frame, "No saved searches yet.", y + 2.0);
    }
    let shown = titles.len().min(launcher::MAX_SEARCHES);
    for (index, cell) in launcher::grid(shown, frame.left, y, width, CELL, GAP).into_iter().enumerate() {
        let rect = pen.r(cell.min.x, cell.min.y, cell.width(), cell.height());
        let over = touch.over(rect);
        let (fill, border) = if over { (tone_raise(), tone_line()) } else { (Color32::TRANSPARENT, tone_hair()) };
        pen.boxed(rect, 16, fill, border);
        let label = pen.fitted(titles[index].as_str(), F::T500, 13.0, if over { tone_gold_hi() } else { tone_star() }, 0.0, cell.width() - 24.0);
        pen.draw(&label, cell.min.x + (cell.width() - label.w) / 2.0, cell.min.y + 6.0 + pen.bl(13.0, 20.0));
        touch.zone(rect, Action::RunSearch(index));
    }
    y + launcher::grid_height(shown, CELL, GAP)
}

/// An action button box: a raised square with an 8px radius. The outlined main button keeps its
/// copper border and is filled once lit.
fn action_box(pen: &Pen<'_>, rect: Rect, fill: Color32, border: Color32) {
    if fill == Color32::TRANSPARENT && border != tone_gold() {
        pen.boxed(rect, 8, tone_raise(), tone_raise());
    } else {
        pen.boxed(rect, 8, fill, border);
    }
}

/// `.btn`: 30px tall, a 1px border, its label in the middle (12px from the border when the button
/// is as wide as its label asks). Returns where it is.
fn button(pen: &Pen<'_>, x: f32, y: f32, w: f32, label: &Txt, fill: Color32, border: Color32) -> Rect {
    let rect = pen.r(x, y, w, 30.0);
    action_box(pen, rect, fill, border);
    pen.draw(label, x + (w - label.w) / 2.0, y + 5.0 + pen.bl(13.0, 20.0));
    rect
}

/// `.budget i`: a pip is this wide, the next one starts this much further.
const PIP: f32 = 5.0;
const PIP_STEP: f32 = PIP + 3.0;
/// `.budget`: its padding on each side, and its whole width.
const PIPS_PAD: f32 = 2.0;
const PIPS_WIDTH: f32 = 2.0 * PIPS_PAD + PIP_STEP * (PIPS as f32 - 1.0) + PIP;
/// `.actions { gap }`
const ACTIONS_GAP: f32 = 6.0;

/// `.actions`: "Search again" with the request budget beside it and "Open trade site" for an item
/// that is searched, and "Close" at the right for every view. The buttons start at `y`.
fn actions(pen: &Pen<'_>, frame: &Frame, y: f32, check: Option<&Check>, budget: Budget, touch: &mut Touch) {
    let hair = pen.hair;
    let pad = hair + 12.0; // `.btn`: border, then padding 0 12px
    // `.btn:hover`
    let lifted = |over: bool| if over { (tone_raise(), tone_line()) } else { (Color32::TRANSPARENT, tone_hair()) };

    if let Some(check) = check.filter(|check| check.searchable()) {
        let mut x = frame.left;
        // `.btn.main`: outlined in copper; filled when the filters changed, and under the pointer.
        // As wide as "Search again" whatever it says, so that nothing beside it moves.
        let state = again(check, budget);
        let width = 2.0 * pad + pen.t("Search again", F::T600, 13.0, tone_star()).w;
        let over = state == Again::Ready && touch.over(pen.r(x, y, width, 30.0));
        let (label, text, fill, border) = match state {
            Again::Ready if check.stale || over => ("Search again".to_owned(), tone_night(), tone_gold(), tone_gold()),
            Again::Ready => ("Search again".to_owned(), tone_gold_hi(), Color32::TRANSPARENT, tone_gold()),
            Again::Wait(seconds) => (format!("Wait {seconds} s"), tone_mute(), Color32::TRANSPARENT, tone_hair()),
            Again::Off => ("Search again".to_owned(), tone_mute(), Color32::TRANSPARENT, tone_hair()),
        };
        let rect = button(pen, x, y, width, &pen.t(&label, F::T600, 13.0, text), fill, border);
        if state == Again::Ready {
            touch.zone(rect, Action::SearchAgain);
        }
        x += width + ACTIONS_GAP;

        // `.budget`: a pip per search left, filled; hollow once it is spent.
        for (pip, left) in pips(budget.left).into_iter().enumerate() {
            let (cx, cy) = (x + PIPS_PAD + PIP / 2.0 + PIP_STEP * pip as f32, y + 15.0);
            if left {
                pen.dot(cx, cy, PIP, tone_mute());
            } else {
                pen.add(Shape::circle_stroke(pen.at(cx, cy), (PIP - hair) / 2.0, Stroke::new(hair, tone_mute())));
            }
        }
        x += PIPS_WIDTH + ACTIONS_GAP;

        // The page of the last search: nothing to open before the site has answered one.
        let open = check.trade_url().is_some();
        let label = pen.t("Open trade site", F::T500, 13.0, if open { tone_star() } else { tone_mute() });
        let width = 2.0 * pad + label.w;
        let (fill, border) = lifted(open && touch.over(pen.r(x, y, width, 30.0)));
        let rect = button(pen, x, y, width, &label, fill, border);
        if open {
            touch.zone(rect, Action::OpenTradeSite);
        }
    }

    // `.btn.push`: "Close" and its key, against the right edge.
    let close = pen.t("Close", F::T500, 13.0, tone_star());
    let esc = pen.t("Esc", F::T500, 12.0, tone_mute());
    let key_w = hair + 6.0 + esc.w + 6.0 + hair;
    let w = pad + close.w + 6.0 + key_w + pad;
    let x = frame.right - w;
    let rect = pen.r(x, y, w, 30.0);
    let (fill, border) = lifted(touch.over(rect));
    action_box(pen, rect, fill, border);
    let btn_base = y + 5.0 + pen.bl(13.0, 20.0);
    pen.draw(&close, x + pad, btn_base);
    // the key: an inline box, its border around the text's content area
    let kx = x + pad + close.w + 6.0;
    let key_h = pen.ascent(12.0) + pen.descent(12.0) + 2.0 * hair;
    pen.boxed(pen.r(kx, btn_base - pen.ascent(12.0) - hair, key_w, key_h), 4, Color32::TRANSPARENT, tone_hair());
    pen.draw(&esc, kx + hair + 6.0, btn_base);
    touch.zone(rect, Action::Close);
}

fn rarity_colour(rarity: &Rarity) -> Color32 {
    match rarity {
        Rarity::Unique => UNIQUE,
        Rarity::Rare => RARE,
        Rarity::Magic => MAGIC,
        Rarity::Gem => GEM,
        Rarity::Normal => tone_star(),
        Rarity::Currency | Rarity::DivinationCard | Rarity::Other(_) => CURRENCY,
        // No rarity at all: the mark is there, in the quiet colour, and no word follows it.
        Rarity::None => tone_mute(),
    }
}

/// The rarity and the league, the name, then the base with what else is known of the item.
/// Returns the y under it.
fn head(pen: &Pen<'_>, frame: &Frame, item: &Item, league: Option<&str>, rate: Option<&str>, icon: Option<&str>) -> f32 {
    let (left, right) = (frame.left, frame.right);
    let colour = rarity_colour(&item.rarity);
    let head_base = 14.0 + pen.bl(12.0, 20.0);
    {
        // The rarity: an 8px square turned 45 degrees, inline before the word and sitting on its
        // baseline (`.rarity::before`, margin 0 9px 0 2px). A browser turns it about the centre
        // of its box, and puts that box on whole physical pixels.
        let centre = pen.at(pen.snap_x(left + 2.0) + 4.0, pen.snap_y(head_base - 8.0) + 4.0);
        let r = 4.0 * std::f32::consts::SQRT_2;
        let pts = [vec2(0.0, -r), vec2(r, 0.0), vec2(0.0, r), vec2(-r, 0.0)];
        pen.add(Shape::convex_polygon(pts.map(|d| centre + d).to_vec(), colour, Stroke::NONE));
    }
    let label_x = left + 2.0 + 8.0 + 9.0;
    let league = league.map(|league| pen.fitted(league, F::T400, 12.0, tone_mute(), 0.0, 180.0));
    // The divine rate sits at the left of the league, small and quiet.
    let rate = rate.map(|rate| {
        let rich = pen.rich(rate, F::T400, 11.0, tone_mute(), 12.0);
        if rich.w <= 120.0 { rich } else { pen.unit("", pen.fitted(rate, F::T400, 11.0, tone_mute(), 0.0, 120.0), 11.0, 12.0) }
    });
    let league_w = league.as_ref().map_or(0.0, |t| t.w + 10.0) + rate.as_ref().map_or(0.0, |t| t.w + 10.0);
    // An item that has no rarity has no word here.
    let label = state::rarity_label(&item.rarity);
    if !label.is_empty() {
        pen.draw(&pen.fitted(label, F::T600, 12.0, colour, 0.0, right - league_w - label_x), label_x, head_base);
    }
    let mut edge = right;
    if let Some(league) = &league {
        pen.draw_right(league, edge, head_base);
        edge -= league.w + 10.0;
    }
    if let Some(rate) = &rate {
        pen.draw_rich_right(rate, edge, head_base);
    }

    // An item without a name line is called by its base.
    let title = item.name.as_deref().unwrap_or(&item.base);
    // A stackable's own icon stands big at the left of its name once it is there.
    let mut name_x = left;
    if let Some(texture) = icon.and_then(|key| icons::get(pen.ctx, key)) {
        let rect = Rect::from_min_size(pen.at(left, 33.0), vec2(STACK_ICON, STACK_ICON));
        pen.picture(texture, rect);
        name_x += STACK_ICON + 8.0;
    }
    let name = pen.fitted(title, F::D700, 22.0, tone_star(), -0.11, right - name_x);
    pen.draw(&name, name_x, 34.0 + pen.bl_display(22.0, 28.0)); // `.overlay h2 { margin: 0 }` outranks the margin-top of `.name`

    let mut about: Vec<(String, Color32)> = Vec::new();
    if item.name.is_some() {
        about.push((item.base.clone(), tone_star()));
    }
    if let Some((held, _)) = item.stack.filter(|(held, _)| *held > 1) {
        about.push((format!("Stack of {held}"), tone_star()));
    }
    if let Some(level) = item.item_level {
        about.push((format!("Item level {level}"), tone_mute()));
    }
    if item.fractured {
        about.push(("Fractured".into(), FRACTURED));
    }
    if item.corrupted {
        about.push(("Corrupted".into(), tone_ember()));
    }
    if about.is_empty() {
        return 62.0 + 2.0;
    }
    // `.base`: one line, its parts 14px apart. The first one gives way when they do not all fit.
    let base_y = 62.0 + pen.bl(13.0, 20.0);
    let rest: Vec<Txt> = about[1..].iter().map(|(s, c)| pen.t(s, F::T400, 13.0, *c)).collect();
    let rest_w: f32 = rest.iter().map(|t| t.w + 14.0).sum();
    let first = pen.fitted(&about[0].0, F::T400, 13.0, about[0].1, 0.0, (right - left - rest_w).max(60.0));
    let mut x = left;
    for t in std::iter::once(&first).chain(&rest) {
        pen.draw(t, x, base_y);
        x += t.w + 14.0;
    }
    62.0 + 20.0 + 2.0
}

/// The side of a stackable's own icon, in the head.
const STACK_ICON: f32 = 30.0;

/// The tallest the plate gets, in CSS pixels: about 900 on a screen at 100%.
const MAX_HEIGHT: f32 = 900.0;
/// The listings a plate shows however tall its rows make it.
const LEAST_LISTINGS: usize = 3;
/// `.lrow`
const LISTING: f32 = 23.0;
/// `.lrow.arriving`
const ARRIVING: f32 = 15.0;
/// The rule, the padding under it and the line of column names.
const TABLE_HEAD: f32 = 1.0 + 6.0 + 18.0;
/// `.listings { padding-bottom }`
const TABLE_FOOT: f32 = 6.0;
/// `.token { height }`
const TOKEN: f32 = 26.0;
/// `.defaults { padding-top, padding-bottom }`
const SETTINGS_PAD: f32 = 10.0;

/// The footer of a view: for an item that is searched, the rule, the settings between their
/// paddings, the buttons and the plate's padding under them; for any other view its padding,
/// the button, the plate's padding.
fn footer(searchable: bool) -> f32 {
    if searchable { 1.0 + SETTINGS_PAD + TOKEN + SETTINGS_PAD + 30.0 + 14.0 } else { 8.0 + 30.0 + 14.0 }
}

/// How many of `count` listings go in `room` CSS pixels; never fewer than `LEAST_LISTINGS` of
/// them, so that a long list of mods still leaves prices to read.
fn fitting(room: f32, count: usize) -> usize {
    let fit = (room / LISTING).floor().max(0.0) as usize;
    fit.max(LEAST_LISTINGS).min(count)
}

/// `.stat-row { min-height }`
const ROW: f32 = 30.0;
/// `.tiers { width }`
const TIERS: f32 = 46.0;
/// `.stat-name`: font-size, line-height.
const ROW_TEXT: (f32, f32) = (13.5, 17.0);
/// `.tier`: font-size, line-height.
const TIER_TEXT: (f32, f32) = (11.5, 15.0);
/// The mark of a value given as at 20% quality: its font-size, and the gap before it.
const Q20_TEXT: f32 = 10.5;
const Q20_GAP: f32 = 6.0;
/// Between the text of a mod no stat matches and its mark.
const UNMATCHED_GAP: f32 = 5.0;

/// What the listings keep of their colour once the filters have changed
/// (`.plate[data-stale="true"] #listings { opacity }`).
const STALE: f32 = 0.45;

/// `.stat-row { gap }`
const ROW_GAP: f32 = 6.0;
/// `.num`: width and height, border included.
const NUM: (f32, f32) = (44.0, 24.0);
/// `.num`: font-size, and the line-height it inherits.
const NUM_TEXT: (f32, f32) = (13.0, 20.0);
/// `.num { padding }` on each side, and what is left of it around a text too long for the field.
const NUM_PAD: f32 = 7.0;
const NUM_PAD_TIGHT: f32 = 3.0;

/// The slider of a row is inside the row: the text of the row's first line is this much higher
/// while it shows, and what is under this much of the row's height is the slider's.
const SLIDER_LIFT: f32 = 5.0;
const SLIDER_TOP: f32 = 17.0;
/// Its rail is as thick as the roll bar of a tier mark, and level with it.
const RAIL: f32 = 3.0;
/// Its handle: a disc this wide, with this much of the row's ground around it.
const HANDLE: f32 = 10.0;
const HANDLE_RING: f32 = 1.5;
/// The tick at the item's own roll: its width and its height.
const TICK: (f32, f32) = (2.0, 9.0);

/// Where the roll bar of a tier mark starts, in the row that starts at `y`: 1 above the bottom
/// of the mark (`.tier::before { bottom }`), which is 20 tall in the middle of the row.
fn roll_bar_top(y: f32) -> f32 {
    y + (ROW - (TIER_TEXT.1 + 5.0)) / 2.0 + TIER_TEXT.1 + 5.0 - 1.0 - RAIL
}

/// Where the minimum field of a searched row starts; the maximum field ends at the gutter.
fn fields_x(frame: &Frame) -> f32 {
    frame.right - 2.0 * NUM.0 - ROW_GAP
}

/// Where the text of a row ends: before the fields of a row that is searched, at the gutter
/// otherwise. A text that would go further is cut with an ellipsis.
fn text_end(frame: &Frame, on: bool) -> f32 {
    if on { fields_x(frame) - ROW_GAP } else { frame.right }
}

/// `.stats`: one row per property and per line of a mod. A row that is part of the search has the
/// edge bar, the text's colour and, at the right of its first line, its two number fields; the
/// others are quiet. A fractured mod and a crafted one stand out: see `Special`. Where the
/// implicit mods end and the others begin, a hairline crosses the rows, as a line does in the
/// game's own text; it takes no room. A row that has a filter is a switch: a click anywhere on
/// it, from the plate's
/// edge to the end of its text, turns the filter on or off, and it lifts under the pointer. The
/// fields are no part of the switch.
///
/// A searched mod whose range is known shows its slider while the row is engaged: the pointer
/// is over the row, fields included, one of its fields is being typed in, the menu of its rolls
/// is open, or the slider is held. The slider is under the text of the first line, which moves
/// up to make room for it; the row is as tall as ever. It is no part of the switch either: the
/// switch is then what is above it, what is beside it under the tier mark, and the other lines.
/// Returns the y under the block, `y` itself when there is no row.
/// Whether a row is on the panel (the Hidden switch shows the others, dimmed): none on the base view (mods, pseudo and properties are of the
/// item, not of its base), but a fractured item's fractured mods, which the base search keeps.
fn shown(check: &Check, row: &state::Row) -> bool {
    check.reveal || !check.hides(row)
}

fn mod_rows(pen: &Pen<'_>, frame: &Frame, mut y: f32, check: &Check, face: &Face<'_>, touch: &mut Touch) -> f32 {
    if !check.rows.iter().any(|row| shown(check, row)) {
        return y;
    }
    let fields = face.fields;
    y += 2.0;
    // `.tiers` and its margin, then the row's gap.
    let text_x = frame.left + TIERS + 2.0 + ROW_GAP;
    let (size, line_height) = ROW_TEXT;
    let mut after_implicit = false;
    for row in check.rows.iter().filter(|row| shown(check, row)) {
        let mark = check.mark(row);
        let on = mark == Mark::Searched;
        let excluded = row.filter.is_some_and(|filter| check.excluded(filter));
        let special = Special::of(row);
        let colour = if excluded { tone_ember() } else { row_tone(on, special) };
        let end = text_end(frame, on && crate::field::has_fields(check, row));
        let room = end - text_x;
        // The whole mod is one switch, however many lines it has.
        let block = ROW * row.lines.len() as f32;
        let held = face.drag.filter(|drag| Some(drag.filter()) == row.filter);
        let typing = fields.active().and_then(|editor| editor.target().filter());
        let engaged = touch.over(pen.r(0.0, y, frame.right + frame.left, block))
            || held.is_some()
            || row.filter.is_some_and(|filter| typing == Some(filter) || face.menu == Some(Chip::Roll(filter)));
        let scale = slider::scale_of(check, row, engaged);
        let switch = row.filter.map(|filter| (pen.r(0.0, y, end, block), filter));
        // A slider that is held keeps its row lifted wherever the pointer went.
        let over = switch.is_some_and(|(rect, _)| touch.over(rect)) || held.is_some();
        if let Some((rect, filter)) = switch {
            let toggle = Action::ToggleFilter(filter);
            if scale.is_some() {
                touch.zone(pen.r(0.0, y, end, SLIDER_TOP), toggle);
                touch.late.push(Zone { rect: pen.r(0.0, y + SLIDER_TOP, text_x, ROW - SLIDER_TOP), action: toggle });
                if block > ROW {
                    touch.late.push(Zone { rect: pen.r(0.0, y + ROW, end, block - ROW), action: toggle });
                }
            } else {
                touch.zone(rect, toggle);
            }
        }
        if over {
            // Clear of the edge bar on the left; as far out on the right.
            pen.fill(frame.left - 4.0, y, frame.right - frame.left + 8.0, block, 8, tone_raise());
        }
        if on && look::marker() == Marker::Band {
            pen.boxed(pen.r(frame.left - 4.0, y, frame.right - frame.left + 8.0, block), 9, tone_raise(), tone_hair());
        }
        // The first mod after the implicit ones: the hairline, over the lift of either row.
        if after_implicit && row.kind.is_some() && !row.implicit {
            pen.fill(frame.left, y, frame.right - frame.left, 1.0, 0, tone_hair());
        }
        after_implicit = row.implicit;
        if let (true, true, true, Some(filter)) = (on, check.leads(row), crate::field::has_fields(check, row), row.filter) {
            // While its slider is held, the minimum shows the value held.
            num_field(pen, fields_x(frame), y, check, fields, Target::min(filter), held.map(|drag| drag.value()), touch);
            num_field(pen, frame.right - NUM.0, y, check, fields, Target::max(filter), None, touch);
            // The minimum of a mod whose range is known: a right click on it lists the rolls it
            // can be set to, under the field.
            if rolls(check, filter).is_some() {
                touch.chip(pen, Chip::Roll(filter), (fields_x(frame), y + (ROW - NUM.1) / 2.0, NUM.0, NUM.1), false);
            }
            if let Some(scale) = scale {
                let zone = pen.r(text_x, y + SLIDER_TOP, room, ROW - SLIDER_TOP);
                // The minimum as it stands: the value held, what is being typed, or the filter's.
                let typed = fields.active().filter(|editor| editor.target() == Target::min(filter)).map(|editor| match editor.typed() {
                    Typed::Number(value) => Some(value),
                    Typed::Empty | Typed::NotANumber => None,
                });
                let min = held.map(|drag| drag.value()).or_else(|| typed.unwrap_or_else(|| check.min_of(row)));
                let look = Slider {
                    // The row's own tone for a mod that is marked, the copper accent otherwise;
                    // lighter while the pointer is on the slider.
                    tone: match (special, held.is_some() || touch.over(zone)) {
                        (Some(special), false) => special.tone(),
                        (Some(special), true) => special.text(),
                        (None, false) => tone_gold(),
                        (None, true) => tone_gold_hi(),
                    },
                    tick: colour,
                    ground: if over { tone_raise() } else { tone_plate() },
                };
                let (left, right) = slider_track(pen, (text_x, end), y, scale, min, row.value, &look);
                touch.tracks.push(Track { zone, filter, scale, left: left + pen.o.x, right: right + pen.o.x });
            }
        }
        // A right click anywhere on the row of a stat opens its menu: the exclusion is there.
        // The minimum field's own chip, pushed before, answers first where they meet.
        if let Some(filter) = row.filter.filter(|filter| check.can_exclude(*filter)) {
            touch.chip(pen, Chip::Roll(filter), (text_x, y, room, block), false);
        }
        // `.star-btn::before`, and its `:hover`: lit when searched, a hint of it when it could be.
        let marker = look::marker();
        // In the mod's own tone when it has one (Foulborn, enchant, influence...), gold otherwise.
        let bar = match (marker, on, over) {
            (Marker::Bar, true, false) => Some(special.map_or_else(tone_gold, Special::tone)),
            (Marker::Bar, true, true) => Some(special.map_or_else(tone_gold_hi, |special| lighter(special.tone()))),
            (Marker::Bar, false, true) => Some(tone_line()),
            _ => None,
        }
        .filter(|_| !excluded);
        let weight = if on && marker == Marker::Plain { F::T600 } else { F::T400 };
        let (row_top, mut cut) = (y, false);
        for (index, line) in row.lines.iter().enumerate() {
            if let Some(bar) = bar.filter(|_| index == 0) {
                // 3 wide, in the middle of the 16px to the left of the row: one bar down all the
                // lines of the mod, which is one switch; 18 tall on a single line.
                pen.fill((16.0 - 3.0) / 2.0, y + (ROW - 18.0) / 2.0, 3.0, block - (ROW - 18.0), 2, bar);
            }
            // The mods of a unique have no tier, nor have those of an item searched by its very
            // base: where the mark would be, the bar of the roll alone, for a mod the item gives
            // the range of. A fractured mod and a crafted one have their pill there whatever
            // they are on.
            let untiered = matches!(check.plan, Plan::Unique | Plan::Exact { .. });
            // The mark sits in the middle of all the lines of the mod.
            let mark_y = y + (block - ROW) / 2.0;
            if index == 0 && marker == Marker::Badge && special.is_none() {
                // The tier itself is the badge: filled when the mod is searched.
                let top = mark_y + (ROW - 17.0) / 2.0;
                let (fill, edge, ink) = if on { (tone_gold(), tone_gold(), tone_night()) } else { (Color32::TRANSPARENT, tone_line(), tone_mute()) };
                pen.boxed(pen.r(frame.left, top, 30.0, 17.0), 9, fill, edge);
                let label = row.tier.map_or_else(String::new, |tier| format!("T{tier}"));
                let text = pen.t(&label, if on { F::T700 } else { F::T600 }, TIER_TEXT.0, ink);
                pen.draw(&text, frame.left + (30.0 - text.w) / 2.0, top + (17.0 - TIER_TEXT.1) / 2.0 + pen.bl(TIER_TEXT.0, TIER_TEXT.1));
                if row.filter.is_some_and(|filter| check.is_pure(filter)) {
                    pure_mark(pen, frame.left, mark_y);
                }
            } else if index == 0 && (row.tier.is_some() || untiered || special.is_some()) {
                tier_mark(pen, touch, frame.left, mark_y, row.tier, row.roll, on, special);
                if row.filter.is_some_and(|filter| check.is_pure(filter)) {
                    pure_mark(pen, frame.left, mark_y);
                }
            }
            let lift = if index == 0 && scale.is_some() { SLIDER_LIFT } else { 0.0 };
            let base = y + (ROW - line_height) / 2.0 - lift + pen.bl(size, line_height);
            // A mod no stat matches ends with its mark: after its last line.
            let unmatched = mark == Mark::Unmatched && index + 1 == row.lines.len();
            match (&row.figure, unmatched) {
                // `Energy shield <b>498</b>`: a few words and a number, which always fit. A value
                // that is the one at 20% quality and not the item's own says so, quietly.
                (Some(figure), _) => {
                    let name = format!("{line} ");
                    let text = pen.spans(&[(&name, F::T400, colour), (figure, F::T700, colour)], size, 0.0);
                    pen.draw(&text, text_x, base);
                    if row.q20 {
                        pen.draw(&pen.t("Q20", F::T600, Q20_TEXT, tone_mute()), text_x + text.w + Q20_GAP, base);
                    }
                }
                (None, true) => {
                    let mark = pen.t("?", F::T700, TIER_TEXT.0, tone_ember());
                    let fit = room - UNMATCHED_GAP - mark.w;
                    let text = pen.fitted(line, F::T400, size, colour, 0.0, fit);
                    pen.draw(&text, text_x, base);
                    pen.draw(&mark, text_x + text.w + UNMATCHED_GAP, base);
                    cut |= pen.t(line, F::T400, size, colour).w > fit;
                }
                (None, false) => {
                    let text = pen.fitted(line, weight, size, colour, 0.0, room);
                    pen.draw(&text, text_x, base);
                    cut |= pen.t(line, weight, size, colour).w > room;
                }
            }
            if excluded {
                // Struck through: the whole of the text the line shows.
                let width = match &row.figure {
                    Some(figure) => pen.spans(&[(&format!("{line} "), F::T400, colour), (figure, F::T700, colour)], size, 0.0).w,
                    None => pen.fitted(line, weight, size, colour, 0.0, room).w,
                };
                let at = y + ROW / 2.0 - lift + 0.5;
                pen.add(Shape::line_segment([pen.at(text_x, at), pen.at(text_x + width, at)], Stroke::new(1.3, colour)));
            }
            // A row a display rule hides is only here because the Hidden switch is on: dimmed.
            if check.hides(row) {
                pen.fill(frame.left - 4.0, y, frame.right - frame.left + 8.0, block, 8, tone_plate().gamma_multiply(0.6));
            }
            y += ROW;
        }
        // Cut short: the whole mod over its rows while the pointer is on its text.
        if cut {
            let rect = pen.r(text_x, row_top, room, block);
            touch.unfolds.push(Unfold { rect, lines: row.lines.clone(), weight, size, colour });
        }
    }
    y + 8.0
}

/// The colours of a slider.
struct Slider {
    /// The filled part of the rail and the handle.
    tone: Color32,
    /// The tick at the item's own roll.
    tick: Color32,
    /// What the row is drawn on: the ring around the handle is of it.
    ground: Color32,
}

/// The slider of a row that starts at `y`: a rail from `from` to `to`, level with the roll bar
/// of the tier mark, filled from its low end to where the minimum `min` sits in `scale`, with a
/// round handle there and a thin tick where the item's own roll `roll` sits. The handle stays
/// inside the rail: its middle goes from half its width after `from` to half its width before
/// `to`. Returns those two places.
fn slider_track(pen: &Pen<'_>, (from, to): (f32, f32), y: f32, scale: Scale, min: Option<f64>, roll: Option<f64>, look: &Slider) -> (f32, f32) {
    let (left, right) = (from + HANDLE / 2.0, to - HANDLE / 2.0);
    let top = roll_bar_top(y);
    let middle = top + RAIL / 2.0;
    let at = scale.x_of(min, left, right);
    pen.fill(from, top, to - from, RAIL, 2, tone_line());
    // The ends of the range, dimmed, under the rail's two ends: the handle is drawn over them.
    let (low, high) = scale.bounds_text();
    let base = top + RAIL + 7.0;
    pen.draw(&pen.t(&low, F::T400, 9.0, tone_mute().gamma_multiply(0.9)), from, base);
    pen.draw_right(&pen.t(&high, F::T400, 9.0, tone_mute().gamma_multiply(0.9)), to, base);
    pen.fill(from, top, at - from, RAIL, 2, look.tone);
    if let Some(roll) = roll {
        let tick = scale.x_of(Some(roll), left, right);
        pen.fill(tick - TICK.0 / 2.0, middle - TICK.1 / 2.0, TICK.0, TICK.1, 0, look.tick);
    }
    pen.dot(at, middle, HANDLE + 2.0 * HANDLE_RING, look.ground);
    pen.dot(at, middle, HANDLE, look.tone);
    (left, right)
}

/// How a number field is drawn: `.num` for the two of a row, `.opt-num` for the one of a chip.
struct Well {
    width: f32,
    height: f32,
    radius: u8,
    /// font-size, and the line-height the text is centred with.
    text: (f32, f32),
    /// The padding on each side of the text, and what is left of it around a text too long.
    pad: (f32, f32),
    /// `text-align: center`, not `right`.
    centred: bool,
}

/// `.num`
const NUM_WELL: Well = Well { width: NUM.0, height: NUM.1, radius: 8, text: NUM_TEXT, pad: (NUM_PAD, NUM_PAD_TIGHT), centred: false };
/// `.opt[data-on="true"] .opt-num` (outside the first theme, whose figures are narrower).
const CHIP_WELL: Well = Well { width: 30.0, height: 18.0, radius: 9, text: (12.0, 20.0), pad: (0.0, 0.0), centred: true };

/// `.num`: one of the two fields of a searched row, in the middle of the row that starts at `y`.
#[allow(clippy::too_many_arguments)]
fn num_field(pen: &Pen<'_>, x: f32, y: f32, check: &Check, fields: &Fields, target: Target, held: Option<f64>, touch: &mut Touch) {
    well(pen, &NUM_WELL, x, y + (ROW - NUM.1) / 2.0, check, fields, target, held, touch);
}

/// The width of a number field: its usual one, more for a text that needs it, never past `most`.
fn field_width(usual: f32, needed: f32, most: f32) -> f32 {
    needed.min(most.max(usual)).max(usual)
}

/// A number field with its top-left corner at `x`, `top`. It shows its number, or for a bound
/// that is none "min" or "max" quietly. The field being typed in has the copper border of
/// `:focus`, its text as typed, the selection and the caret; a field that is flagged has the red
/// one. A field whose slider is held shows the value `held` in place of its bound, with the same
/// copper border: it is what the release will give it.
#[allow(clippy::too_many_arguments)]
fn well(pen: &Pen<'_>, look: &Well, x: f32, top: f32, check: &Check, fields: &Fields, target: Target, held: Option<f64>, touch: &mut Touch) {
    let hair = pen.hair;
    let height = look.height;
    let editor = fields.active().filter(|editor| editor.target() == target && held.is_none());
    let text = match (held, editor) {
        (Some(held), _) => field::written(held),
        (None, Some(editor)) => editor.text().to_owned(),
        (None, None) => fields.text(check, target),
    };
    let whole = pen.t(&text, F::T600, look.text.0, tone_star());
    // A right-aligned field grows to the left to fit its text, up to about six characters.
    let (width, x) = if look.centred {
        (look.width, x)
    } else {
        let room = 2.0 * (hair + look.pad.0);
        let widest = pen.t("888888", F::T600, look.text.0, tone_star()).w + room;
        let width = field_width(look.width, whole.w + room, widest);
        (width, x + look.width - width)
    };
    let rect = pen.r(x, top, width, height);
    touch.fields.push(NumField { rect, target });
    let border = match editor {
        Some(_) => tone_gold(),
        None if held.is_some() => tone_gold(),
        None if field::invalid(check, target) => tone_ember(),
        None => tone_hair(),
    };
    pen.boxed(rect, look.radius, tone_night(), border);

    let (size, line_height) = look.text;
    let base = top + (height - line_height) / 2.0 + pen.bl(size, line_height);
    // Inside the padding; a text too long for it gets most of the padding.
    let pad = if whole.w > width - 2.0 * (hair + look.pad.0) { look.pad.1 } else { look.pad.0 };
    let (left, right) = (x + hair + pad, x + width - hair - pad);
    // Where a text that fits starts: against the right padding, or in the middle.
    let rest = if look.centred { (right - left - whole.w) / 2.0 } else { right - left - whole.w };
    // What still does not fit is cut at the padding (the caret may stand a pixel into it).
    let clipped = pen.p.map(|p| p.with_clip_rect(pen.r(left - 1.0, top, right - left + 2.0 + hair, height)));
    let pen = &Pen { ctx: pen.ctx, p: clipped.as_ref(), o: pen.o, hair, ppp: pen.ppp };

    if text.is_empty()
        && let Some(bound) = target.bound()
    {
        // `.num::placeholder`
        let hint = if bound == Bound::Min { "min" } else { "max" };
        pen.draw_right(&pen.t(hint, F::T400, size, tone_mute()), right, base);
    }
    let Some(editor) = editor else {
        // Too long: from its start, as a field that is not typed in shows it.
        if !text.is_empty() {
            pen.draw(&whole, left + rest.max(0.0), base);
        }
        return;
    };
    // Where the text is cut before each character, from its left end.
    let edge = |at: usize| pen.t(&text[..at], F::T600, size, tone_star()).w;
    let caret = edge(editor.caret());
    // Too long: moved so that the caret is in sight.
    let text_x = (left + rest).max(left - caret);
    let (up, down) = (pen.ascent(size), pen.descent(size));
    let (start, end) = editor.selection().unwrap_or((0, 0));
    if start != end {
        // `::selection`
        pen.fill(text_x + edge(start), base - up, edge(end) - edge(start), up + down, 2, tone_gold());
    }
    let parts = [(&text[..start], F::T600, tone_star()), (&text[start..end], F::T600, tone_night()), (&text[end..], F::T600, tone_star())];
    pen.draw(&pen.spans(&parts, size, 0.0), text_x, base);
    pen.fill(text_x + caret, base - up, hair, up + down, 0, tone_star());
}

/// `.seg { padding, gap }`: around its buttons, and between two of them.
const SEG_PAD: f32 = 2.0;
const SEG_GAP: f32 = 2.0;
/// `.seg button { height, padding }`, and its font-size with the line-height it inherits.
const SEG_BUTTON: f32 = 26.0;
const SEG_BUTTON_PAD: f32 = 12.0;
const SEG_TEXT: (f32, f32) = (13.0, 20.0);
/// What keeps the control from the estimate above it, and what is under it: with the chips' own
/// padding it is as far from them as two rows of chips are from each other.
const SEG_ABOVE: f32 = 4.0;
const SEG_UNDER: f32 = 2.0;

/// `.seg`: the two ways of pricing a piece of gear, as the segmented control of the
/// settings: a round track with a hairline border, holding a button for each way. The way the
/// item is priced has the raised fill, a thin copper border inside it and the heavier label
/// (`[aria-checked="true"]`); the other is quiet, and its label lights under the pointer. A click
/// on a button is that way. Returns the y under the control, `y` itself for an item that has no
/// way to choose.
fn ways(pen: &Pen<'_>, frame: &Frame, y: f32, check: &Check, touch: &mut Touch) -> f32 {
    if !check.has_ways() {
        return y;
    }
    let hair = pen.hair;
    let (size, line_height) = SEG_TEXT;
    let top = y + SEG_ABOVE;
    let weight = |way: Way| if check.way == way { F::T600 } else { F::T400 };
    // A button is as wide as its label asks, at the weight it has.
    let widths = WAYS.map(|way| SEG_BUTTON_PAD + pen.t(check.way_label(way), weight(way), size, tone_star()).w + SEG_BUTTON_PAD);
    let inner = widths.iter().sum::<f32>() + SEG_GAP * (WAYS.len() - 1) as f32;
    let edge = hair + SEG_PAD;
    let height = edge + SEG_BUTTON + edge;
    pen.boxed(pen.r(frame.left, top, edge + inner + edge, height), 16, Color32::TRANSPARENT, tone_hair());

    let mut x = frame.left + edge;
    for (way, width) in WAYS.into_iter().zip(widths) {
        let rect = pen.r(x, top + edge, width, SEG_BUTTON);
        let chosen = check.way == way;
        if chosen {
            // `box-shadow: inset 0 0 0 1px`: a CSS pixel, which a browser does not bring down to
            // a physical one as it does a border.
            pen.add(egui::epaint::RectShape::new(rect, 13, tone_raise(), Stroke::new(1.0, tone_gold()), StrokeKind::Inside));
        }
        let colour = if chosen || touch.over(rect) { tone_star() } else { tone_mute() };
        let label = pen.t(check.way_label(way), weight(way), size, colour);
        pen.draw(&label, x + SEG_BUTTON_PAD, top + edge + (SEG_BUTTON - line_height) / 2.0 + pen.bl(size, line_height));
        touch.zone(rect, Action::SetWay(way));
        x += width + SEG_GAP;
    }
    top + height + SEG_UNDER
}

/// `.opt { height }`, and `.opts { gap }` between two chips and between two rows of them.
const OPT: f32 = 24.0;
const OPT_GAP: f32 = 6.0;
/// `.opt-btn { padding }` on each side of a chip's label, and after it before a number.
const OPT_PAD: f32 = 10.0;
const OPT_PAD_NUM: f32 = 4.0;
/// `.opt-num` of a chip that is off: its width and the margin after it. On, the number is in
/// `CHIP_WELL`, with this margin after it.
const OPT_NUM: (f32, f32) = (20.0, 9.0);
const OPT_WELL_MARGIN: f32 = 2.0;
/// `.opt { font-size }` and `.opt-btn { line-height }`.
const OPT_TEXT: (f32, f32) = (12.0, 22.0);

/// `.opts`: the chips of the item's own options, in rows that wrap at the gutter. A chip that is
/// part of the search is lit: the copper border and the text's colour. A click anywhere on a chip
/// is its switch, but in the number of a chip that is on, which is a field. A right click on the
/// corrupted chip opens the menu of its three values. Returns the y under the block, `y` itself
/// when the item has no chip.
fn option_chips(pen: &Pen<'_>, frame: &Frame, y: f32, check: &Check, fields: &Fields, touch: &mut Touch) -> f32 {
    let chips = check.option_chips();
    if chips.is_empty() {
        return y;
    }
    let hair = pen.hair;
    let (size, line_height) = OPT_TEXT;
    let (mut x, mut top) = (frame.left, y + 4.0);
    for chip in &chips {
        // What follows the label: the number and its margin, or the padding.
        let after = match (chip.number, chip.on) {
            (Some(_), true) => OPT_PAD_NUM + CHIP_WELL.width + OPT_WELL_MARGIN,
            (Some(_), false) => OPT_PAD_NUM + OPT_NUM.0 + OPT_NUM.1,
            (None, _) => OPT_PAD + if chip.option == ItemOption::Unique { OPT_CARET + 4.0 } else { 0.0 },
        };
        // A name too long for a whole row is cut.
        // An influence's chip leads with its symbol, once it is there.
        let symbol = (chip.option == ItemOption::Influence).then(|| icons::symbol(pen.ctx, &chip.label)).flatten();
        let lead = if symbol.is_some() { SYMBOL_CHIP + 4.0 } else { 0.0 };
        let room = frame.right - frame.left - 2.0 * hair - OPT_PAD - after - lead;
        let label_w = pen.fitted(&chip.label, F::T400, size, tone_mute(), 0.0, room).w;
        let width = hair + OPT_PAD + lead + label_w + after + hair;
        if x > frame.left && x + width > frame.right + 0.01 {
            (x, top) = (frame.left, top + OPT + OPT_GAP);
        }
        let rect = pen.r(x, top, width, OPT);
        let over = touch.over(rect);
        // `.opt`, `.opt:hover`, `.opt[data-on="true"]` and its `:hover`.
        let (border, colour) = match (chip.on, over) {
            (true, false) => (tone_gold(), tone_star()),
            (true, true) => (tone_gold_hi(), tone_star()),
            (false, true) => (tone_line(), tone_star()),
            (false, false) => (tone_hair(), tone_mute()),
        };
        pen.boxed(rect, 12, Color32::TRANSPARENT, border);
        let base = top + (OPT - line_height) / 2.0 + pen.bl(size, line_height);
        if let Some(texture) = symbol {
            pen.picture(texture, Rect::from_min_size(pen.at(x + hair + OPT_PAD, top + (OPT - SYMBOL_CHIP) / 2.0), vec2(SYMBOL_CHIP, SYMBOL_CHIP)));
        }
        pen.draw(&pen.fitted(&chip.label, F::T400, size, colour, 0.0, room), x + hair + OPT_PAD + lead, base);
        let number_x = x + hair + OPT_PAD + lead + label_w + OPT_PAD_NUM;
        // Which unique: a click, as a right click, opens its menu.
        if chip.option == ItemOption::Unique {
            // The caret of a menu: a small triangle pointing down, at the right.
            let (cx, cy) = (x + width - hair - OPT_PAD - OPT_CARET / 2.0, top + OPT / 2.0);
            let corners = [pen.at(cx - OPT_CARET / 2.0, cy - 2.0), pen.at(cx + OPT_CARET / 2.0, cy - 2.0), pen.at(cx, cy + 2.5)];
            pen.add(Shape::convex_polygon(corners.to_vec(), colour, Stroke::NONE));
            touch.chip(pen, Chip::Unique, (x, top, width, OPT), false);
            x += width + OPT_GAP;
            continue;
        }
        let target = if chip.option == ItemOption::ItemLevel { Target::ItemLevel } else { Target::Quality };
        match (chip.number, chip.on) {
            // The number in its well: a field. The switch is the label before it.
            (Some(_), true) => {
                touch.zone(pen.r(x, top, number_x - x, OPT), chip.click);
                well(pen, &CHIP_WELL, number_x, top + (OPT - CHIP_WELL.height) / 2.0, check, fields, target, None, touch);
            }
            // Only a label yet: the whole chip is the switch.
            (Some(number), false) => {
                touch.zone(rect, chip.click);
                let number = pen.t(&number.to_string(), F::T400, size, colour);
                pen.draw(&number, number_x + (OPT_NUM.0 - number.w) / 2.0, base);
            }
            (None, _) => touch.zone(rect, chip.click),
        }
        if chip.option == ItemOption::Corrupted {
            touch.chip(pen, Chip::Corrupted, (x, top, width, OPT), false);
        }
        x += width + OPT_GAP;
    }
    top + OPT + 6.0
}

/// The width of the caret on the chip of the uniques.
const OPT_CARET: f32 = 8.0;

/// `.token`: its padding before the pin and after the value, and its font-size and the
/// line-height of `.value`.
const TOKEN_PAD: (f32, f32) = (3.0, 10.0);
const TOKEN_TEXT: (f32, f32) = (12.5, 24.0);
/// `.pin { width, height }`
const PIN: f32 = 22.0;

/// `.defaults`: the three settings of every search, each a chip with its pin at the left and its
/// value. A click on the value goes to the next one, a right click anywhere on the chip opens the
/// menu of all of them, above the chip, and a click on the pin pins the value or unpins it. The
/// chips start at `y`.
fn settings(pen: &Pen<'_>, frame: &Frame, y: f32, check: &Check, pins: Config, touch: &mut Touch) {
    let hair = pen.hair;
    let (size, line_height) = TOKEN_TEXT;
    let mut x = frame.left;
    for setting in SETTINGS {
        let pinned = pins.pinned(setting);
        let label = check.setting_label(setting);
        let label_w = pen.t(label, F::T400, size, tone_star()).w;
        let width = hair + TOKEN_PAD.0 + PIN + label_w + TOKEN_PAD.1 + hair;
        pen.boxed(pen.r(x, y, width, TOKEN), 7, Color32::TRANSPARENT, tone_hair());

        // `.pin`, in the middle of the chip's height; `.pin:hover` is a disc behind it.
        let (pin_x, pin_y) = (x + hair + TOKEN_PAD.0, y + (TOKEN - PIN) / 2.0);
        let pin_rect = pen.r(pin_x, pin_y, PIN, PIN);
        if touch.over(pin_rect) {
            pen.fill(pin_x, pin_y, PIN, PIN, 11, tone_raise());
        }
        // `.pin svg`: 13 by 14, in the middle of the button.
        pushpin(pen, pin_x + (PIN - 13.0) / 2.0, pin_y + (PIN - 14.0) / 2.0, pinned);
        touch.pins.push(PinZone { rect: pin_rect, setting });

        // `.value`, and its `:hover`. A value that is not pinned is quiet.
        let value_x = pin_x + PIN;
        let value = pen.r(value_x, y, x + width - value_x, TOKEN);
        let colour = match (touch.over(value), pinned) {
            (true, _) => tone_gold_hi(),
            (false, true) => tone_star(),
            (false, false) => tone_mute(),
        };
        pen.draw(&pen.t(label, F::T400, size, colour), value_x, y + (TOKEN - line_height) / 2.0 + pen.bl(size, line_height));
        touch.zone(value, cycle(setting));
        touch.chip(pen, Chip::Setting(setting), (x, y, width, TOKEN), true);
        x += width + 6.0;
    }
}

/// `.pin svg`: a pushpin 13 by 14 with its top-left corner at `x`, `y`, leaning to the right.
/// Pinned, it is filled copper; not pinned, it is an outline.
fn pushpin(pen: &Pen<'_>, x: f32, y: f32, pinned: bool) {
    // `rotate(32 6.5 6)`, in the drawing's own units, which are CSS pixels.
    let (sin, cos) = 32f32.to_radians().sin_cos();
    let at = |(px, py): (f32, f32)| {
        let (dx, dy) = (px - 6.5, py - 6.0);
        pen.at(x + 6.5 + dx * cos - dy * sin, y + 6.0 + dx * sin + dy * cos)
    };
    let colour = if pinned { tone_gold() } else { tone_mute() };
    // The head, the neck, the skirt and the base, as the `polygon` has them.
    let outline = [
        (4.2, 0.8), (8.8, 0.8), (8.8, 2.2), (8.1, 2.2), (8.1, 5.6), (10.4, 7.9),
        (10.4, 8.9), (2.6, 8.9), (2.6, 7.9), (4.9, 5.6), (4.9, 2.2), (4.2, 2.2),
    ];
    if pinned {
        // The shape is not convex: filled as four pieces that are, each a little into the next.
        let pieces: [[(f32, f32); 4]; 4] = [
            [(4.2, 0.8), (8.8, 0.8), (8.8, 2.2), (4.2, 2.2)],
            [(4.9, 1.8), (8.1, 1.8), (8.1, 5.9), (4.9, 5.9)],
            [(4.9, 5.6), (8.1, 5.6), (10.4, 7.9), (2.6, 7.9)],
            [(2.6, 7.6), (10.4, 7.6), (10.4, 8.9), (2.6, 8.9)],
        ];
        for piece in pieces {
            pen.add(Shape::convex_polygon(piece.map(at).to_vec(), colour, Stroke::NONE));
        }
    }
    // `stroke-width: 1.2` for the shape, `1.3` for the needle (`line`), whose round ends reach
    // half its width past its two points.
    pen.add(Shape::closed_line(outline.map(at).to_vec(), Stroke::new(1.2, colour)));
    pen.add(Shape::line_segment([at((6.5, 9.0 - 0.3)), at((6.5, 13.4 + 0.65))], Stroke::new(1.3, colour)));
}

/// `.pop`: its padding, and how far it hangs from its chip.
const POP_PAD: f32 = 5.0;
const POP_GAP: f32 = 7.0;
/// `.pop-line`: its height, its padding at the left and at the right, and the gap after its mark.
const POP_LINE: f32 = 26.0;
const POP_LINE_PAD: (f32, f32) = (7.0, 14.0);
const POP_LINE_GAP: f32 = 8.0;
/// `.pop-line i`: the room of the mark, which is 11 across when it is the current value and 9
/// with a margin of 1 otherwise.
const POP_MARK: f32 = 11.0;
/// The least a menu keeps from the plate's edge when it has to be moved to stay inside it.
const POP_EDGE: f32 = 6.0;

/// `.pop.menu`: the menu of a chip, every value it can take on a line of its own, the one it has
/// marked with a filled point. It hangs under its chip, or above it for a setting (`.pop-up`), at
/// least as wide as the chip. The window is the plate and nothing more, so a menu that would
/// leave the plate, `within` wide and tall, goes to the other side of its chip or is moved in.
/// Returns where it is and its lines as zones.
fn menu(pen: &Pen<'_>, anchor: &Anchor, lines: &[Choice], within: (f32, f32), pointer: Option<Pos2>) -> (Rect, Vec<Zone>) {
    let hair = pen.hair;
    let texts: Vec<Txt> = lines.iter().map(|line| pen.t(line.label, F::T400, 13.0, if line.current { tone_star() } else { tone_mute() })).collect();
    let widest = texts.iter().map(|text| text.w).fold(0.0, f32::max);
    let line_w = (POP_LINE_PAD.0 + POP_MARK + POP_LINE_GAP + widest + POP_LINE_PAD.1).max(anchor.width - 2.0 * hair - 2.0 * (hair + POP_PAD));
    let width = hair + POP_PAD + line_w + POP_PAD + hair;
    let height = hair + POP_PAD + POP_LINE * lines.len() as f32 + POP_PAD + hair;
    // `left: -1px` from the chip's padding box; `top` or `bottom: calc(100% + 7px)`.
    let x = (anchor.x + hair - 1.0).min(within.0 - POP_EDGE - width).max(POP_EDGE);
    let above = anchor.y + hair - POP_GAP - height;
    let below = anchor.y + anchor.height - hair + POP_GAP;
    let fits = |top: f32| top >= POP_EDGE && top + height + SHADOW <= within.1 - POP_EDGE;
    let y = match (anchor.upward, fits(above), fits(below)) {
        (true, true, _) | (false, true, false) => above,
        (false, _, true) | (true, false, true) => below,
        // Neither side has the room: as far in as it goes.
        _ => (if anchor.upward { above } else { below }).min(within.1 - POP_EDGE - SHADOW - height).max(POP_EDGE),
    };
    let rect = pen.r(x, y, width, height);
    // `box-shadow: 0 3px 0`, then the box.
    pen.add(Shape::rect_filled(rect.translate(vec2(0.0, SHADOW)), 12, Color32::from_rgba_unmultiplied(4, 7, 16, 179)));
    pen.boxed(rect, 12, tone_plate(), tone_line());

    let mut zones = Vec::new();
    let line_x = x + hair + POP_PAD;
    for (index, (line, text)) in lines.iter().zip(&texts).enumerate() {
        let line_y = y + hair + POP_PAD + POP_LINE * index as f32;
        let zone = pen.r(line_x, line_y, line_w, POP_LINE);
        if pointer.is_some_and(|at| inside(zone, at)) {
            // `.pop-line:hover`
            pen.fill(line_x, line_y, line_w, POP_LINE, 8, tone_raise());
        }
        let (cx, cy) = (line_x + POP_LINE_PAD.0 + POP_MARK / 2.0, line_y + POP_LINE / 2.0);
        if line.current {
            pen.dot(cx, cy, POP_MARK, tone_gold());
        } else {
            // 9 across with a border of 1.5.
            pen.add(Shape::circle_stroke(pen.at(cx, cy), (9.0 - 1.5) / 2.0, Stroke::new(1.5, tone_mute())));
        }
        pen.draw(text, line_x + POP_LINE_PAD.0 + POP_MARK + POP_LINE_GAP, line_y + (POP_LINE - 20.0) / 2.0 + pen.bl(13.0, 20.0));
        zones.push(Zone { rect: zone, action: line.action });
    }
    (rect, zones)
}

/// A mod that is not like the others and is marked so that it cannot be missed: its tier mark
/// is a filled pill in its tone, its roll bar is in that tone, and its text in a lighter one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Special {
    /// The item can no longer lose it.
    Fractured,
    /// It was put on at the crafting bench.
    Crafted,
    /// The Foulborn mechanic changed it.
    Foulborn,
    /// An implicit the item got by being corrupted.
    Corrupted,
    /// An enchant of the item.
    Enchant,
    /// A mod a unique rolls from a pool of its own, as Watcher's Eye does.
    Variant,
    /// The implicit of a vestigial item.
    Vestigial,
    /// An influence, an Eldritch implicit, the synthesis, an essence...: see `Origin`.
    Origin(zenith_core::Origin),
    /// Any of the four conquerors' citadels, on the base view of a map.
    Conquerors,
    /// A map the Originator's memories influence.
    Originator,
}

/// The tone of an origin's pill, each apart from the marks above and from the others. Bright
/// enough to carry the dark letter on every theme's plate.
/// The violet of a vestigial implicit, as the game writes it.
const VESTIGIAL: Color32 = Color32::from_rgb(0xA6, 0x8C, 0xF2);

/// The tone of any conqueror: theirs, mixed.
const CONQUERORS: Color32 = Color32::from_rgb(0xD8, 0xC6, 0x9A);

/// The conquerors, in the order their symbols fan out.
const CONQUEROR_ORIGINS: [zenith_core::Origin; 4] =
    [zenith_core::Origin::Crusader, zenith_core::Origin::Redeemer, zenith_core::Origin::Hunter, zenith_core::Origin::Warlord];

/// The tone of a map the Originator's memories influence.
const ORIGINATOR: Color32 = Color32::from_rgb(0xB7, 0x8C, 0xFF);

/// What the implicit of a map tells of it: who occupies it, whose citadel it holds, whose
/// influence it bears, each in the tone of that influence.
fn map_implicit(row: &crate::check::Row) -> Option<Special> {
    use zenith_core::Origin as O;
    if !row.implicit {
        return None;
    }
    let text = row.lines.join(" ");
    if text.contains("Originator's Memories") {
        return Some(Special::Originator);
    }
    if text == "Any conqueror" {
        return Some(Special::Conquerors);
    }
    let origin = [
        ("Map is occupied by", O::Elder),
        ("Any Elder guardian", O::Elder),
        ("Baran's Citadel", O::Crusader),
        ("Veritania's Citadel", O::Redeemer),
        ("Al-Hezmin's Citadel", O::Hunter),
        ("Drox's Citadel", O::Warlord),
        ("influenced by The Shaper", O::Shaper),
        ("influenced by The Elder", O::Elder),
    ]
    .into_iter()
    .find(|(words, _)| text.contains(words))?;
    Some(Special::Origin(origin.1))
}

fn origin_tone(origin: zenith_core::Origin) -> Color32 {
    use zenith_core::Origin as O;
    let (r, g, b) = match origin {
        O::Shaper => (0x3F, 0x8C, 0xFF),
        O::Elder => (0xBC, 0xBC, 0xD6),
        O::Crusader => (0xE8, 0xE2, 0xC4),
        O::Redeemer => (0x5F, 0xD0, 0xFF),
        O::Hunter => (0x6B, 0xCB, 0x4A),
        O::Warlord => (0xD9, 0x4F, 0x6E),
        O::Exarch => (0xFF, 0x7A, 0x3D),
        O::Eater => (0x3F, 0xD6, 0xA0),
        O::Synthesised => (0xF2, 0x8E, 0xD0),
        O::Veiled => (0x8E, 0x93, 0xAA),
        O::Unveiled => (0xC9, 0xA7, 0x7C),
        O::Essence => (0xF4, 0xF4, 0xF4),
        O::Delve => (0xD9, 0xA4, 0x41),
    };
    Color32::from_rgb(r, g, b)
}

/// The two letters of an origin's pill.
fn origin_letter(origin: zenith_core::Origin) -> &'static str {
    use zenith_core::Origin as O;
    match origin {
        O::Shaper => "Sh",
        O::Elder => "El",
        O::Crusader => "Cr",
        O::Redeemer => "Re",
        O::Hunter => "Hu",
        O::Warlord => "Wa",
        O::Exarch => "Ex",
        O::Eater => "Ea",
        O::Synthesised => "Sy",
        O::Veiled => "Vl",
        O::Unveiled => "Un",
        O::Essence => "Es",
        O::Delve => "De",
    }
}

/// The letter of the pill of a row, when it has one.
#[cfg(test)]
pub(crate) fn special_letter(row: &crate::check::Row) -> Option<&'static str> {
    Special::of(row).map(Special::letter)
}

impl Special {
    /// What a row is marked as. A mod that is both is first of all fractured.
    fn of(row: &crate::check::Row) -> Option<Self> {
        match (row.fractured, row.crafted, row.foulborn, row.corruption) {
            (true, ..) => Some(Self::Fractured),
            (false, true, ..) => Some(Self::Crafted),
            (false, false, true, _) => Some(Self::Foulborn),
            (false, false, false, true) => Some(Self::Corrupted),
            (false, false, false, false) if row.kind.as_deref() == Some("Enchant") => Some(Self::Enchant),
            (false, false, false, false) if row.kind.as_deref().is_some_and(|kind| kind.starts_with("Vestigial")) => Some(Self::Vestigial),
            (false, false, false, false) if row.variant => Some(Self::Variant),
            (false, false, false, false) => row.origin.map(Self::Origin).or_else(|| map_implicit(row)),
        }
    }

    /// The pill and the filled part of the roll bar.
    fn tone(self) -> Color32 {
        match self {
            Self::Fractured => FRACTURED,
            Self::Crafted => CRAFTED,
            Self::Foulborn => foulborn_tone(),
            Self::Corrupted => tone_ember(),
            Self::Enchant => ENCHANT,
            Self::Variant => UNIQUE,
            Self::Vestigial => VESTIGIAL,
            Self::Origin(origin) => origin_tone(origin),
            Self::Conquerors => CONQUERORS,
            Self::Originator => ORIGINATOR,
        }
    }

    /// The text of the mod while it is searched.
    fn text(self) -> Color32 {
        match self {
            Self::Fractured => FRACTURED_TEXT,
            Self::Crafted => CRAFTED_TEXT,
            Self::Foulborn => foulborn_text(),
            Self::Corrupted => tone_ember(),
            Self::Enchant => ENCHANT_TEXT,
            Self::Variant => lighter(UNIQUE),
            Self::Vestigial => lighter(VESTIGIAL),
            Self::Origin(origin) => lighter(origin_tone(origin)),
            Self::Conquerors => lighter(CONQUERORS),
            Self::Originator => lighter(ORIGINATOR),
        }
    }

    /// What the pill holds when the mod has no tier, as a crafted mod never has.
    fn letter(self) -> &'static str {
        match self {
            Self::Fractured => "F",
            Self::Crafted => "C",
            Self::Foulborn => "FB",
            Self::Corrupted => "V",
            Self::Enchant => "E",
            Self::Variant => "U",
            Self::Vestigial => "Vg",
            Self::Origin(origin) => origin_letter(origin),
            Self::Conquerors => "Co",
            Self::Originator => "Or",
        }
    }
}

/// How much of its way from the plate a text keeps when its row is not searched: what takes the
/// text's colour to the quiet one, near enough.
const QUIET: f32 = 0.58;

/// A tone as the row of a mod that is not searched has it: dimmed toward the plate as the usual
/// text is when it turns quiet.
fn quiet(tone: Color32) -> Color32 {
    let mix = |tone: u8, plate: u8| (plate as f32 + QUIET * (tone as f32 - plate as f32)).round() as u8;
    Color32::from_rgb(mix(tone.r(), tone_plate().r()), mix(tone.g(), tone_plate().g()), mix(tone.b(), tone_plate().b()))
}

/// The colour of a row's text: the text's when it is searched and the quiet one when it is not,
/// and for a mod that is marked its own tone, dimmed when it is not searched.
fn row_tone(on: bool, special: Option<Special>) -> Color32 {
    match (special, on) {
        (Some(special), true) => special.text(),
        (Some(special), false) => quiet(special.text()),
        (None, true) => tone_star(),
        (None, false) => tone_mute(),
    }
}

/// The colour of a tier mark and of the filled part of its roll bar: the text's when the row is
/// searched and the quiet one when it is not, and for a mod that is marked its tone either way.
fn tier_tone(on: bool, special: Option<Special>) -> Color32 {
    match (special, on) {
        (Some(special), _) => special.tone(),
        (None, true) => tone_star(),
        (None, false) => tone_mute(),
    }
}

/// The pill of a marked mod: the padding on each side of what it holds, and its least width.
const PILL_PAD: f32 = 5.0;
const PILL_MIN: f32 = 20.0;
/// The side of a symbol in a pill, and in a chip.
const SYMBOL: f32 = 22.0;
/// The side of a symbol in a chip, and the height of a pill that holds a symbol.
const SYMBOL_CHIP: f32 = 18.0;
const SYMBOL_PILL: f32 = 24.0;

/// What a pill that holds a symbol is filled with: the plate's dark with a third of the tone in it,
/// so that the symbol reads on it whatever its own colours.
fn symbol_fill(tone: Color32) -> Color32 {
    let night = tone_night();
    let mix = |tone: u8, night: u8| (night as f32 + 0.3 * (tone as f32 - night as f32)).round() as u8;
    Color32::from_rgb(mix(tone.r(), night.r()), mix(tone.g(), night.g()), mix(tone.b(), night.b()))
}

/// `.tier`: the tier, and under it a 3px bar filled to where the roll sits in its range. The
/// mark is 20px tall, in the middle of the row that starts at `y`. Without a tier the bar is
/// where it would be under one. For a mod that is marked, the tier is in a filled pill as tall
/// as the tier's line, so that the bar keeps its place under it; without a tier the pill holds
/// the letter of what the mod is.
/// The small "pure" under a tier mark: the stat is searched as a genuine tier.
fn pure_mark(pen: &Pen<'_>, x: f32, y: f32) {
    let text = pen.t("pure", F::T600, 8.5, tone_gold());
    pen.draw(&text, x + (TIERS - text.w).min(30.0) / 2.0, y + ROW - 8.5 + pen.bl(8.5, 9.0) - 1.0);
}

#[allow(clippy::too_many_arguments)]
fn tier_mark(pen: &Pen<'_>, touch: &mut Touch, x: f32, y: f32, tier: Option<u32>, roll: Option<f32>, on: bool, special: Option<Special>) {
    let (size, line_height) = TIER_TEXT;
    let top = y + (ROW - (line_height + 5.0)) / 2.0;
    let (weight, colour) = (if on { F::T700 } else { F::T600 }, tier_tone(on, special));
    let label = tier.map(|tier| format!("T{tier}"));
    match (special, label) {
        (Some(special), label) => {
            // A Foulborn mod without a tier holds the Breach logo, else a small rift.
            if special == Special::Foulborn && label.is_none() {
                let symbol = icons::symbol(pen.ctx, "Breach");
                let (width, tall) = if symbol.is_some() { (SYMBOL + 6.0, SYMBOL_PILL) } else { (PILL_MIN, line_height) };
                let pill_top = if symbol.is_some() { y + (ROW - SYMBOL_PILL) / 2.0 - 2.0 } else { top };
                touch.tips.push((pen.r(x, pill_top, width, tall), "Foulborn mod".into()));
                if let Some(texture) = symbol {
                    pen.fill(x, pill_top, width, tall, 10, symbol_fill(colour));
                    pen.picture(texture, Rect::from_center_size(pen.at(x + width / 2.0, pill_top + tall / 2.0), vec2(SYMBOL, SYMBOL)));
                } else {
                    pen.fill(x, top, PILL_MIN, line_height, 8, colour);
                    rift(pen, x + PILL_MIN / 2.0, top + line_height / 2.0, tone_night());
                }
                if let Some(roll) = roll {
                    let bar_y = if symbol.is_some() { symbol_bar_top(y) } else { roll_bar_top(y) };
                    pen.fill(x, bar_y, TIERS, RAIL, 2, tone_line());
                    pen.fill(x, bar_y, (roll.clamp(0.0, 1.0) * TIERS).max(3.0), RAIL, 2, colour);
                }
                return;
            }
            // Any conqueror: their four symbols fanned out, once they are all there.
            if special == Special::Conquerors {
                let symbols: Vec<_> = CONQUEROR_ORIGINS.iter().filter_map(|origin| icons::symbol(pen.ctx, origin.label())).collect();
                if symbols.len() == CONQUEROR_ORIGINS.len() {
                    let (icon, step, pad) = (17.0, 8.0, 3.0);
                    let width = icon + step * (symbols.len() - 1) as f32 + 2.0 * pad;
                    let pill_top = y + (ROW - SYMBOL_PILL) / 2.0 - 2.0;
                    pen.fill(x, pill_top, width, SYMBOL_PILL, 10, symbol_fill(colour));
                    touch.tips.push((pen.r(x, pill_top, width, SYMBOL_PILL), special_tip(special).into()));
                    for (index, texture) in symbols.into_iter().enumerate() {
                        let at = pen.at(x + pad + step * index as f32, pill_top + (SYMBOL_PILL - icon) / 2.0);
                        pen.picture(texture, Rect::from_min_size(at, vec2(icon, icon)));
                    }
                    return;
                }
            }
            // An influence's pill holds its symbol (and the tier after it) once the symbol is there.
            if let Special::Origin(origin) = special
                && let Some(texture) = icons::symbol(pen.ctx, origin.label())
            {
                let text = label.as_ref().map(|label| pen.spans(&[(label, F::T700, colour)], size, 0.01 * size));
                let pad = 3.0;
                let width = (SYMBOL + 2.0 * pad + text.as_ref().map_or(0.0, |text| text.w + 2.0)).max(SYMBOL + 6.0);
                let pill_top = y + (ROW - SYMBOL_PILL) / 2.0 - 2.0;
                pen.fill(x, pill_top, width, SYMBOL_PILL, 10, symbol_fill(colour));
                touch.tips.push((pen.r(x, pill_top, width, SYMBOL_PILL), origin_tip(origin).into()));
                let icon_x = if text.is_some() { x + pad } else { x + (width - SYMBOL) / 2.0 };
                pen.picture(texture, Rect::from_min_size(pen.at(icon_x, pill_top + (SYMBOL_PILL - SYMBOL) / 2.0), vec2(SYMBOL, SYMBOL)));
                if let Some(text) = &text {
                    pen.draw(text, icon_x + SYMBOL + 2.0, pill_top + (SYMBOL_PILL - line_height) / 2.0 + pen.bl(size, line_height));
                }
                if let Some(roll) = roll {
                    let bar_y = symbol_bar_top(y);
                    pen.fill(x, bar_y, TIERS, RAIL, 2, tone_line());
                    pen.fill(x, bar_y, (roll.clamp(0.0, 1.0) * TIERS).max(3.0), RAIL, 2, colour);
                }
                return;
            }
            let label = label.unwrap_or_else(|| special.letter().to_owned());
            // letter-spacing: .01em
            let text = pen.spans(&[(&label, F::T700, tone_night())], size, 0.01 * size);
            let width = (text.w + 2.0 * PILL_PAD).max(PILL_MIN);
            pen.fill(x, top, width, line_height, 8, colour);
            touch.tips.push((pen.r(x, top, width, line_height), special_tip(special).into()));
            pen.draw(&text, x + (width - text.w) / 2.0, top + pen.bl(size, line_height));
        }
        (None, Some(label)) => {
            // letter-spacing: .01em
            let text = pen.spans(&[(&label, weight, colour)], size, 0.01 * size);
            pen.draw(&text, x, top + pen.bl(size, line_height));
        }
        (None, None) => {}
    }
    if let Some(roll) = roll {
        // `::before` and `::after`: bottom 1px, height 3px.
        let bar_y = roll_bar_top(y);
        pen.fill(x, bar_y, TIERS, RAIL, 2, tone_line());
        pen.fill(x, bar_y, (roll.clamp(0.0, 1.0) * TIERS).max(3.0), RAIL, 2, colour);
    }
}

/// Under a pill that holds a symbol, which is taller than the tier's line.
fn symbol_bar_top(y: f32) -> f32 {
    y + ROW - 1.0 - RAIL
}

/// What an origin's pill says it is.
fn origin_tip(origin: zenith_core::Origin) -> &'static str {
    use zenith_core::Origin as O;
    match origin {
        O::Shaper => "Shaper mod",
        O::Elder => "Elder mod",
        O::Crusader => "Crusader mod",
        O::Redeemer => "Redeemer mod",
        O::Hunter => "Hunter mod",
        O::Warlord => "Warlord mod",
        O::Exarch => "Searing Exarch implicit",
        O::Eater => "Eater of Worlds implicit",
        O::Synthesised => "Synthesised implicit",
        O::Veiled => "Veiled mod (not yet unveiled)",
        O::Unveiled => "Unveiled mod (Catarina)",
        O::Essence => "Essence mod",
        O::Delve => "Delve mod",
    }
}

/// What a lettered pill says it is.
fn special_tip(special: Special) -> &'static str {
    match special {
        Special::Fractured => "Fractured mod",
        Special::Crafted => "Crafted mod",
        Special::Foulborn => "Foulborn mod",
        Special::Corrupted => "Corruption implicit",
        Special::Enchant => "Enchant",
        Special::Variant => "Unique pool",
        Special::Vestigial => "Vestigial",
        Special::Origin(origin) => origin_tip(origin),
        Special::Conquerors => "Any of the four conquerors",
        Special::Originator => "Originator",
    }
}

/// The text of a mod that was cut short.
struct Unfold {
    rect: Rect,
    lines: Vec<String>,
    weight: F,
    size: f32,
    colour: Color32,
}

/// The whole text of a mod, wrapped to the width its row gives it, on a plate over the row and
/// the ones under it.
fn unfolded(pen: &Pen<'_>, unfold: &Unfold) {
    const LINE: f32 = 19.0;
    let (x, y, w) = (unfold.rect.min.x - pen.o.x, unfold.rect.min.y - pen.o.y, unfold.rect.width());
    let lines: Vec<Txt> = unfold.lines.iter().flat_map(|line| pen.wrapped(line, unfold.weight, unfold.size, unfold.colour, w)).collect();
    let h = LINE * lines.len() as f32 + 10.0;
    pen.boxed(pen.r(x - 6.0, y + 2.0, w + 12.0, h), 6, tone_raise(), tone_line());
    for (index, line) in lines.iter().enumerate() {
        pen.draw(line, x, y + 7.0 + LINE * index as f32 + pen.bl(unfold.size, LINE));
    }
}

/// A small plate with `text`, above the mark at `rect` (under it when there is no room above).
fn tooltip(pen: &Pen<'_>, rect: Rect, text: &str) {
    // A text wider than the panel goes on as many lines as it needs.
    let lines = pen.wrapped(text, F::T600, 11.5, tone_star(), WIDTH - 8.0 - 16.0);
    let widest = lines.iter().map(|line| line.w).fold(0.0, f32::max);
    let (w, h) = (widest + 16.0, 15.0 * lines.len() as f32 + 7.0);
    let x = (rect.min.x - pen.o.x + rect.width() / 2.0 - w / 2.0).clamp(4.0, (WIDTH - w - 4.0).max(4.0));
    let above = rect.min.y - pen.o.y - h - 4.0;
    let y = if above >= 2.0 { above } else { rect.max.y - pen.o.y + 4.0 };
    pen.boxed(pen.r(x, y, w, h), 6, tone_night(), tone_line());
    for (index, line) in lines.iter().enumerate() {
        pen.draw(line, x + 8.0, y + 3.5 + 15.0 * index as f32 + pen.bl(11.5, 15.0));
    }
}

/// An original rift mark, centred on (`cx`, `cy`): a small eye-shaped ellipse with a pupil and
/// a few curved tendrils reaching out of it.
fn rift(pen: &Pen<'_>, cx: f32, cy: f32, colour: Color32) {
    let stroke = Stroke::new(1.1, colour);
    let ring: Vec<_> = (0..16)
        .map(|i| {
            let a = i as f32 / 16.0 * std::f32::consts::TAU;
            pen.at(cx + 5.0 * a.cos(), cy + 2.6 * a.sin())
        })
        .collect();
    pen.add(Shape::closed_line(ring, stroke));
    pen.add(Shape::circle_filled(pen.at(cx, cy), 1.3, colour));
    for (side, lift) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let from = pen.at(cx + side * 4.2, cy + lift * 1.6);
        let ctrl = pen.at(cx + side * 6.2, cy + lift * 3.0);
        let to = pen.at(cx + side * 6.6, cy + lift * 5.2);
        pen.add(Shape::QuadraticBezier(egui::epaint::QuadraticBezierShape::from_points_stroke([from, ctrl, to], false, Color32::TRANSPARENT, stroke)));
    }
}

/// The height `sentence` gives the text.
fn sentence_height(pen: &Pen<'_>, frame: &Frame, text: &str) -> f32 {
    20.0 * pen.wrapped(text, F::T400, 13.0, tone_star(), frame.right - frame.left).len() as f32
}

/// A sentence in lines of 20px starting at `y`. Returns the y under the last one.
fn sentence(pen: &Pen<'_>, frame: &Frame, text: &str, y: f32) -> f32 {
    let lines = pen.wrapped(text, F::T400, 13.0, tone_star(), frame.right - frame.left);
    for (i, line) in lines.iter().enumerate() {
        pen.draw(line, frame.left, y + 20.0 * i as f32 + pen.bl(13.0, 20.0));
    }
    y + 20.0 * lines.len() as f32
}

/// One pulse of the bars, in seconds.
const PULSE: f32 = 1.2;
/// What a bar keeps of its colour at the faintest point of its pulse.
const PULSE_LOW: f32 = 0.35;
/// How far each row of bars is behind the one above it, as a share of a pulse.
const PULSE_LAG: f32 = 0.18;

/// How much of its colour a bar of row `row` shows `beat` seconds into the pulsing: eased from
/// all of it down to `PULSE_LOW` and back, once per `PULSE`. The first row starts at all of it.
fn pulse(beat: f32, row: usize) -> f32 {
    let turn = (beat / PULSE - PULSE_LAG * row as f32) * std::f32::consts::TAU;
    PULSE_LOW + (1.0 - PULSE_LOW) * (0.5 + 0.5 * turn.cos())
}

/// A quiet bar where something is still to come (`.arriving .bar`), showing `shown` of its
/// colour: its pulse.
fn waiting(pen: &Pen<'_>, x: f32, y: f32, w: f32, h: f32, alpha: f32, shown: f32) {
    pen.fill(x, y, w, h, 3, tone_line().gamma_multiply(alpha * shown));
}

/// The estimate: "about 3.1 div", and under it how many listings it comes from. Once the filters
/// have changed, that line says so instead (`.estimate .stale`), and with no filter left on the
/// figure itself steps back (`.plate[data-empty="true"]`).
fn figure(pen: &Pen<'_>, frame: &Frame, y: f32, check: &Check, beat: f32, now: i64) -> f32 {
    let Outcome { estimate, total, note, quote, .. } = &check.result;
    // With a pure tier on, the estimate is the one of the listings that stay.
    let kept = check.kept_listings();
    let aside_estimate;
    let estimate = if kept.len() == check.result.listings.len() {
        estimate
    } else {
        aside_estimate = state::estimate_of(&kept);
        &aside_estimate
    };
    // The cached price, while the listings are on their way.
    let cached = quote.as_ref().filter(|quote| quote.each.is_some());
    // A unique is searched by its name: with none of its mods on, it is not without a filter.
    let empty = check.has_ways() && check.stale && !check.filters.is_empty() && !check.filters.iter().any(|filter| filter.on);
    let left = frame.left;
    let fig_base = y + 2.0 + pen.bl_display(48.0, 50.0);
    let about = pen.t("about", F::T400, 14.0, tone_mute());
    pen.draw(&about, left, fig_base);
    let mut x = left + about.w + 8.0;
    match estimate {
        Some(estimate) => {
            let (figure, unit) = if empty { (tone_line(), tone_line()) } else { (tone_gold_hi(), tone_gold()) };
            let amount = state::estimate_text(estimate.amount);
            let big = pen.spans(&[(&amount, F::D800, figure)], 42.0, -0.84);
            pen.draw(&big, x, fig_base);
            x += big.w + 8.0;
            let unit = pen.unit(&estimate.currency, pen.t(state::currency_label(&estimate.currency), F::D700, 22.0, unit), 42.0, 36.0);
            pen.draw_rich(&unit, x, fig_base);
            // The same price in the other currency, at the right, when it fits.
            let divine = quote.as_ref().and_then(|quote| quote.divine);
            if let Some(other) = quote::other_currency(estimate.amount, &estimate.currency, divine) {
                let other = pen.t(&other, F::T400, 14.0, tone_mute());
                if x + unit.w + 12.0 < frame.right - other.w {
                    pen.draw_right(&other, frame.right, fig_base);
                }
            }
        }
        // Not yet the listings' estimate: the cached price stands in its place, paler.
        None if cached.is_some() => {
            let quote = cached.expect("checked on the line above");
            let (amount, unit) = quote::range_text(quote.each.unwrap_or_default(), quote.divine);
            let big = pen.spans(&[(&amount, F::D800, tone_gold_hi().gamma_multiply(CACHED))], 42.0, -0.84);
            pen.draw(&big, x, fig_base);
            let unit = pen.unit(unit, pen.t(unit, F::D700, 22.0, tone_gold().gamma_multiply(CACHED)), 42.0, 36.0);
            pen.draw_rich(&unit, x + big.w + 8.0, fig_base);
        }
        None => waiting(pen, x, fig_base - 10.0, 72.0, 8.0, 0.62, pulse(beat, 0)),
    }
    let count_y = y + 2.0 + 50.0;
    let line = |text: &str, colour: Color32| pen.draw(&pen.t(text, F::T400, 13.0, colour), left, count_y + pen.bl(13.0, 20.0));
    match (check.stale, total, note) {
        // The search failed and the cached price is all there is: it stays, marked, above the error.
        (false, ..) if cached.is_some() && state::cached_stands(&check.result) => {
            let (source, red) = quote::source_line(cached.expect("checked on the line above"), now);
            line(&format!("cached: {source}"), if red { tone_ember() } else { tone_mute() });
        }
        (true, ..) => line(if empty { "No filter on" } else { "Filters changed" }, tone_gold_hi()),
        (false, Some(total), _) => {
            let ignored = estimate.as_ref().map_or(0, |estimate| estimate.ignored);
            line(&state::matched_line(*total, kept.len(), ignored), tone_mute());
        }
        // What is being waited for, when it is not the search itself.
        (false, None, Some(note)) => line(note, tone_mute()),
        // The cached price is marked as such, with its age.
        (false, None, None) if cached.is_some() => {
            let (source, red) = quote::source_line(cached.expect("checked on the line above"), now);
            line(&format!("cached: {source}"), if red { tone_ember() } else { tone_mute() });
        }
        (false, None, None) => waiting(pen, left, count_y + 8.0, 92.0, 5.0, 0.5, pulse(beat, 1)),
    }
    y + 2.0 + 50.0 + 20.0 + 6.0
}

/// A price from the list that is no stack: under the estimate, the item's picture, an arrow, what
/// the list says it costs, and at the right the change over the last seven days with a tiny
/// line of them. Under it, the list's age and what the week is. Returns the y under it.
fn ninja_line(pen: &Pen<'_>, frame: &Frame, y: f32, check: &Check, now: i64) -> f32 {
    let Some(quote) = check.result.quote.as_ref().filter(|quote| quote.each.is_some()) else {
        return y;
    };
    let (amount, unit) = quote::range_text(quote.each.unwrap_or_default(), quote.divine);
    let amount = amount.replace('-', "\u{2013}");
    let top = y + 4.0;
    let cy = top + 13.0;
    let base = top + pen.bl(15.0, 26.0);
    let mut x = frame.left;
    // The item's own picture, from the listings, once it is there.
    let name = quote::name_of(&check.item);
    let texture = check.result.listings.iter().find_map(|listing| listing.icon.as_deref()).and_then(|url| icons::get_url(pen.ctx, name, url));
    if let Some(texture) = texture {
        pen.picture(texture, Rect::from_min_size(pen.at(x, cy - 12.0), vec2(24.0, 24.0)));
        x += 24.0 + 8.0;
        let stroke = Stroke::new(1.3, tone_mute());
        let (a, b) = (pen.at(x, cy), pen.at(x + 12.0, cy));
        pen.add(Shape::line_segment([a, b], stroke));
        pen.add(Shape::line(vec![b + vec2(-4.0, -3.5), b, b + vec2(-4.0, 3.5)], stroke));
        x += 12.0 + 8.0;
    }
    let text = pen.t(&format!("{amount} \u{00D7}"), F::T700, 15.0, tone_gold_hi());
    pen.draw(&text, x, base);
    x += text.w + 5.0;
    let icon = pen.unit(unit, pen.t(unit, F::T700, 15.0, tone_gold()), 15.0, 20.0);
    pen.draw_rich(&icon, x, base);

    let change = state::trend_change(&quote.trend);
    if let Some(change) = change {
        let colour = if change < 0.0 { tone_ember() } else if change > 0.0 { NINJA_UP } else { tone_mute() };
        let label = pen.t(&state::change_text(change), F::T600, 13.0, colour);
        pen.draw_right(&label, frame.right, top + pen.bl(13.0, 26.0));
        let ax = frame.right - label.w - 8.0;
        let tip = if change < 0.0 { 3.0 } else { -3.0 };
        if change != 0.0 {
            pen.add(Shape::convex_polygon(vec![pen.at(ax - 3.5, cy - tip), pen.at(ax + 3.5, cy - tip), pen.at(ax, cy + tip)], colour, Stroke::NONE));
        }
        // The week: lowest to highest over 16 px, a day every 7.
        let known = quote.trend.iter().flatten().copied();
        let (low, high) = (known.clone().fold(f64::INFINITY, f64::min), known.fold(f64::NEG_INFINITY, f64::max));
        let x0 = ax - 10.0 - 6.0 * 7.0;
        let points: Vec<Pos2> = quote
            .trend
            .iter()
            .enumerate()
            .filter_map(|(day, value)| {
                let up = if high > low { (((*value)? - low) / (high - low)) as f32 } else { 0.5 };
                Some(pen.at(x0 + 7.0 * day as f32, cy + 8.0 - 16.0 * up))
            })
            .collect();
        if points.len() >= 2 {
            pen.add(Shape::line(points, Stroke::new(1.2, tone_gold())));
        }
    }
    let low = top + 26.0 + 1.0;
    let lb = low + pen.bl(11.0, 14.0);
    let (source, red) = quote::source_line(quote, now);
    pen.draw(&pen.t(&format!("cached: {source}"), F::T400, 11.0, if red { tone_ember() } else { tone_mute() }), frame.left, lb);
    if change.is_some() {
        pen.draw_right(&pen.t("last 7 days", F::T400, 11.0, tone_mute()), frame.right, lb);
    }
    low + 14.0 + 4.0
}

/// The green of a price that went up.
const NINJA_UP: Color32 = Color32::from_rgb(0x7B, 0xC0, 0x8A);

/// The switch for the rows a display rule hides, at the right of the listings' header: only there
/// when some are hidden.
fn hidden_switch(pen: &Pen<'_>, frame: &Frame, y: f32, check: &Check, touch: &mut Touch) {
    let count = check.hidden_count();
    if count == 0 {
        return;
    }
    let label = pen.t(&state::hidden_label(count), F::T400, 12.0, if check.reveal { tone_star() } else { tone_mute() });
    let cy = y + 1.0 + 6.0 - 1.0 + 10.0;
    let width = label.w + 8.0 + 22.0;
    let x = frame.right - width;
    pen.draw(&label, x, y + 6.0 + pen.bl(12.0, 20.0));
    let track = x + label.w + 8.0;
    pen.fill(track, cy - 6.0, 22.0, 12.0, 6, if check.reveal { tone_gold() } else { tone_line() });
    pen.dot(track + if check.reveal { 16.0 } else { 6.0 }, cy, 8.0, if check.reveal { tone_night() } else { tone_mute() });
    touch.zone(pen.r(x - 4.0, cy - 10.0, width + 4.0, 20.0), Action::ToggleHidden);
}

/// How much of its colour a cached price keeps: it is not the listings' estimate yet.
const CACHED: f32 = 0.62;

/// `.rate`: one line of the currency screen, its label at the left and its value at the right.
fn rate_row(pen: &Pen<'_>, frame: &Frame, y: f32, label: &str, amount: &str, unit: &str) -> f32 {
    let base = y + pen.bl(14.0, 24.0);
    pen.draw(&pen.t(label, F::T400, 14.0, tone_mute()), frame.left, base);
    let unit = pen.unit(unit, pen.t(unit, F::T400, 14.0, tone_mute()), 14.0, 15.0);
    let amount = pen.t(amount, F::T600, 14.0, tone_star());
    pen.draw_rich_right(&unit, frame.right, base);
    pen.draw_right(&amount, frame.right - unit.w - 4.0, base);
    y + 24.0
}

/// A stackable, from the cached price list alone (the currency screen): the price
/// of the stack, or of one when there is no stack, the source and its age, the price of one and
/// the other currency, the seven days, and what a divine buys when it buys more than one.
/// Without a price, the reason in its place.
fn stack_body(pen: &Pen<'_>, frame: &Frame, y: f32, check: &Check, now: i64) -> f32 {
    let Outcome { quote, problem, .. } = &check.result;
    let (left, right) = (frame.left, frame.right);
    let Some((quote, each)) = quote.as_ref().and_then(|quote| Some((quote, quote.each?.0))) else {
        let reason = problem.as_deref().unwrap_or(quote::NO_PRICE_DATA);
        return sentence(pen, frame, reason, y + 6.0) + 2.0;
    };
    let divine = quote.divine;
    let total = quote::stack_total(each, check.item.stack);

    // `.figure` at 40px: the stack's price, in divine from one divine up.
    let (amount, unit) = quote::amount_text(total.unwrap_or(each), divine);
    let fig_base = y + 2.0 + pen.bl_display(40.0, 44.0);
    let about = pen.t(if total.is_some() { "stack" } else { "each" }, F::T400, 14.0, tone_mute());
    pen.draw(&about, left, fig_base);
    let big = pen.spans(&[(&amount, F::D800, tone_gold_hi())], 40.0, -0.8);
    let x = left + about.w + 8.0;
    pen.draw(&big, x, fig_base);
    pen.draw_rich(&pen.unit(unit, pen.t(unit, F::D700, 22.0, tone_gold()), 22.0, 22.0), x + big.w + 8.0, fig_base);
    let (source, red) = quote::source_line(quote, now);
    let from_base = y + 2.0 + 44.0 + pen.bl(13.0, 20.0);
    pen.draw(&pen.t(&source, F::T400, 13.0, if red { tone_ember() } else { tone_mute() }), left, from_base);
    let mut y = y + 2.0 + 44.0 + 20.0 + 6.0;

    // `.rates`: the price of one, and the stack in the other currency.
    let (each_text, each_unit) = quote::amount_text(each, divine);
    if let Some(total) = total {
        y = rate_row(pen, frame, y, "Each", &each_text, each_unit);
        if let Some(rate) = divine.filter(|rate| *rate > 0.0) {
            y = if unit == "div" {
                rate_row(pen, frame, y, "Stack in chaos", &state::estimate_text(total), "chaos")
            } else {
                rate_row(pen, frame, y, "Stack in divine", &state::estimate_text(total / rate), "div")
            };
        }
    } else if let Some(rate) = divine.filter(|rate| *rate > 0.0) {
        let (label, amount, unit) = if unit == "div" {
            ("In chaos", state::estimate_text(each), "chaos")
        } else {
            ("In divine", state::estimate_text(each / rate), "div")
        };
        y = rate_row(pen, frame, y, label, &amount, unit);
    }
    y += 8.0;

    let points: Vec<f64> = quote.trend.iter().flatten().copied().collect();
    if points.len() >= 2 {
        pen.fill(left, y, right - left, 1.0, 0, tone_hair());
        y = trend(pen, frame, y + 1.0, &quote.trend);
    }
    if let Some(count) = quote::divine_buys(each, divine) {
        pen.fill(left, y, right - left, 1.0, 0, tone_hair());
        y += 1.0 + 10.0;
        let base = y + pen.bl(14.0, 24.0);
        let words = pen.t("1 div buys", F::T400, 14.0, tone_star());
        pen.draw(&words, left, base);
        pen.draw(&pen.t(&count, F::T700, 14.0, tone_gold_hi()), left + words.w + 8.0, base);
        let rate = format!("1 div = {} chaos", state::estimate_text(divine.unwrap_or_default()));
        pen.draw_rich_right(&pen.rich(&rate, F::T400, 12.0, tone_mute(), 13.0), right, y + pen.bl(12.0, 24.0));
        y += 24.0 + 10.0;
    }
    for bulk in &check.result.bulk {
        y = bulk_offers(pen, frame, y, bulk, check.result.quote.as_ref());
    }
    y
}

/// The price list's price of one item in the currency of `bulk`, when it knows one.
fn market_price(bulk: &zenith_trade::Bulk, quote: Option<&quote::Quote>) -> Option<f64> {
    let (cheapest, _) = quote?.each?;
    match bulk.have.as_str() {
        "chaos" => Some(cheapest),
        "divine" => quote?.divine.filter(|rate| *rate > 0.0).map(|rate| cheapest / rate),
        _ => None,
    }
}

/// `Live offers`: what sellers ask on the bulk exchange, up to twenty compact rows in price order
/// (the ratio, the stock, the seller) with the price list's own price among them, then the age
/// of the answer.
fn bulk_offers(pen: &Pen<'_>, frame: &Frame, y: f32, bulk: &zenith_trade::Bulk, quote: Option<&quote::Quote>) -> f32 {
    use zenith_trade::Row;
    let (left, right) = (frame.left, frame.right);
    pen.fill(left, y, right - left, 1.0, 0, tone_hair());
    let y = y + 1.0 + 10.0;
    let base = y + pen.bl(12.0, 20.0);
    pen.draw(&pen.t("Live offers", F::T400, 12.0, tone_mute()), left, base);
    pen.draw_right(&pen.t("now", F::T400, 12.0, tone_mute()), right, base);
    let mut y = y + 20.0;
    if let Some(live) = state::live_text(bulk) {
        let base = y + pen.bl(14.0, 24.0);
        pen.draw_rich(&pen.rich(&live, F::T700, 14.0, tone_gold_hi(), 15.0), left, base);
        y += 24.0;
    }
    const ROW: f32 = 18.0;
    for row in bulk.rows(market_price(bulk, quote)) {
        let base = y + pen.bl(12.0, ROW);
        match row {
            Row::Offer(offer) => {
                pen.draw_rich(&pen.rich(&state::ratio_text(offer.each(), &bulk.have), F::T600, 12.0, tone_star(), 13.0), left, base);
                let seller = pen.t(&offer.seller, F::T400, 12.0, tone_mute());
                pen.draw_right(&seller, right, base);
                let stock = pen.t(&format!("{} in stock", offer.stock), F::T400, 12.0, tone_mute());
                pen.draw_right(&stock, right - seller.w - 12.0, base);
            }
            Row::Market(each) => {
                // The price list's line: gold, on a faint band, named by its source.
                pen.fill(left - 4.0, y, right - left + 8.0, ROW, 0, tone_hair());
                pen.draw_rich(&pen.rich(&state::ratio_text(each, &bulk.have), F::T700, 12.0, tone_gold_hi(), 13.0), left, base);
                pen.draw_right(&pen.t(quote::SOURCE_NAME, F::T700, 12.0, tone_gold()), right, base);
            }
        }
        y += ROW;
    }
    y + 8.0
}

/// `.trend-wrap`: the seven days as a thin line over three guides, the lowest and the highest
/// amount written under "7 days" and at the left of the guides. Returns the y under it.
fn trend(pen: &Pen<'_>, frame: &Frame, y: f32, values: &[Option<f64>]) -> f32 {
    let (left, right) = (frame.left, frame.right);
    let known = || values.iter().flatten().copied();
    let low = known().fold(f64::INFINITY, f64::min);
    let high = known().fold(f64::NEG_INFINITY, f64::max);

    // `.trend-cap`
    let y = y + 10.0;
    let cap = y + pen.bl(12.0, 17.0);
    pen.draw(&pen.t("7 days", F::T400, 12.0, tone_mute()), left, cap);
    let range = format!("{} to {}", state::price_text(low), state::price_text(high));
    pen.draw_right(&pen.t(&range, F::T400, 12.0, tone_mute()), right, cap);

    // `.trend`: 78 tall; guides at 12, 36 and 60, the amounts written at their left.
    let top = y + 17.0 + 2.0;
    let x0 = left + 26.0;
    for (guide, value) in [(12.0, high), (36.0, (low + high) / 2.0), (60.0, low)] {
        pen.fill(x0, top + guide, right - x0, 1.0, 0, tone_hair());
        pen.draw(&pen.t(&state::estimate_text(value), F::T400, 12.0, tone_mute()), left, top + guide + 4.0);
    }
    // A flat week sits on the middle guide.
    let height = |value: f64| if high > low { top + 60.0 - ((value - low) / (high - low)) as f32 * 48.0 } else { top + 36.0 };
    let step = (right - x0) / (values.len().max(2) - 1) as f32;
    let at: Vec<Option<Pos2>> =
        values.iter().enumerate().map(|(day, value)| value.map(|value| pen.at(x0 + step * day as f32, height(value)))).collect();
    // Days with no data break the line.
    for run in at.split(Option::is_none) {
        if run.len() >= 2 {
            pen.add(Shape::line(run.iter().flatten().copied().collect(), Stroke::new(1.5, tone_gold())));
        }
    }
    let last = at.iter().rposition(Option::is_some);
    for (day, point) in at.iter().enumerate() {
        let Some(point) = point else { continue };
        if Some(day) == last {
            pen.add(Shape::circle_filled(*point, 3.0, tone_gold_hi()));
        } else {
            pen.add(Shape::circle_filled(*point, 3.0, tone_plate()));
            pen.add(Shape::circle_stroke(*point, 3.0, Stroke::new(1.5, tone_gold())));
        }
    }
    top + 78.0 + 8.0
}

/// The mark of a listing whose corruption differs from the checked item (the search normally asks
/// for the same state, so the mark only shows on the odd one out): a circle this wide, this far
/// after the price.
const MARK: f32 = 6.0;
const MARK_GAP: f32 = 5.0;

/// The listings: price, listed since, seller. Under them, while more are on their way, two rows
/// of bars. `shown` is how much of their colour the listings and the bars keep.
#[allow(clippy::too_many_arguments)]
fn table(
    pen: &Pen<'_>,
    frame: &Frame,
    mut y: f32,
    listings: &[Listing],
    item_corrupted: bool,
    gem: bool,
    loading: bool,
    now: i64,
    beat: f32,
    shown: f32,
) -> f32 {
    let (edge, right) = (frame.left, frame.right);
    let dim = |colour: Color32| if shown < 1.0 { colour.gamma_multiply(shown) } else { colour };
    pen.fill(edge, y, right - edge, 1.0, 0, tone_hair());
    y += 1.0 + 6.0;
    // A gem has its level and quality between the price and the date.
    let c_level = edge + 92.0;
    let c_quality = c_level + 36.0;
    let c_since = if gem { c_quality + 46.0 } else { edge + 92.0 };
    let c_seller = c_since + 70.0;
    let hb = y - 1.0 + pen.bl(12.0, 20.0);
    pen.draw(&pen.t("Price", F::T400, 12.0, tone_mute()), edge, hb);
    if gem {
        pen.draw(&pen.t("Level", F::T400, 12.0, tone_mute()), c_level, hb);
        pen.draw(&pen.t("Quality", F::T400, 12.0, tone_mute()), c_quality, hb);
    }
    pen.draw(&pen.t("Listed", F::T400, 12.0, tone_mute()), c_since, hb);
    pen.draw(&pen.t("Seller", F::T400, 12.0, tone_mute()), c_seller, hb);
    y += 18.0;
    let cheap = state::suspicious_of(listings);
    for (at, listing) in listings.iter().enumerate() {
        let b = y + 1.5 + pen.bl(13.0, 20.0);
        let amount = format!("{} ", state::price_text(listing.amount));
        let price = pen.t(&amount, F::T700, 13.0, dim(tone_star()));
        let odd = listing.corrupted != item_corrupted || cheap.get(at).copied().unwrap_or(false);
        // The unit of a marked listing leaves room for the mark before the next column.
        let room = if odd { c_since - 4.0 - MARK - MARK_GAP - (edge + price.w) } else { 44.0 };
        let name = pen.fitted(state::currency_label(&listing.currency), F::T400, 13.0, dim(tone_mute()), 0.0, room.clamp(0.0, 44.0));
        let unit = pen.unit(&listing.currency, name, 13.0, 14.0);
        pen.draw(&price, edge, b);
        pen.draw_rich(&unit, edge + price.w, b);
        if odd {
            // Level with the middle of the small letters (Manrope's x-height is 0.54 em).
            pen.dot(edge + price.w + unit.w + MARK_GAP + MARK / 2.0, b - 0.27 * 13.0, MARK, dim(tone_ember()));
        }
        if gem {
            let (level, quality) = (listing.gem_level.map(|n| n.to_string()), listing.quality.map(|n| format!("{n}%")));
            pen.draw(&pen.t(level.as_deref().unwrap_or("-"), F::T400, 13.0, dim(tone_star())), c_level, b);
            pen.draw(&pen.t(quality.as_deref().unwrap_or("-"), F::T400, 13.0, dim(tone_star())), c_quality, b);
        }
        let since = state::listed_since(&listing.indexed, now);
        pen.draw(&pen.t(&since, F::T400, 13.0, dim(tone_mute())), c_since, b);
        let seller = state::seller_name(&listing.seller);
        pen.draw(&pen.fitted(seller, F::T400, 13.0, dim(tone_star()), 0.0, right - c_seller), c_seller, b);
        y += LISTING;
    }
    if loading {
        // After the two bars of the figure: each row a little behind the one above it.
        for (row, (widths, alpha)) in [([46.0, 34.0, 78.0], 0.62), ([38.0, 40.0, 60.0], 0.5)].into_iter().enumerate() {
            let shown = shown * pulse(beat, 2 + row);
            waiting(pen, edge, y + 5.0, widths[0], 5.0, alpha, shown);
            waiting(pen, c_since, y + 5.0, widths[1], 5.0, alpha, shown);
            waiting(pen, c_seller, y + 5.0, widths[2], 5.0, alpha, shown);
            y += ARRIVING;
        }
    }
    y + TABLE_FOOT
}
