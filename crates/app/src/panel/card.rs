//! The item of a listing drawn as the game draws it: the name plate with the
//! rarity's header art and the influence symbols at its ends, the properties, the requirements,
//! the sockets (small circles in a frame), the item level, then the blocks of mods, each block
//! after the rarity's separator. The mods are the game's normal look (the site's lines, no
//! headers) until Alt is held (`alt_held`), then the advanced one (grouped per mod, prefixes
//! first, with headers and roll ranges). The look follows the item tooltip of the PoE Kit site (its colours are the game's
//! text colours); the text is the game's Fontin SmallCaps once it is fetched (see fontin.rs), else the panel's
//! own font set in small capitals.
//!
//! The art is the game's (see THIRD_PARTY_NOTICES), in `assets/game-ui/`, built into the program.

use std::sync::atomic::{AtomicBool, Ordering::SeqCst};

use eframe::egui::{Color32, Rect, Shape, TextureId, TextureOptions, pos2, text::LayoutJob, vec2};
use zenith_trade::client::{Affix, ListedItem, ListedMod, ListedModKind, ListedRarity};

use super::{F, Pen, SHADOW, Txt};

/// The card's width, in points.
pub const CARD_W: f32 = 360.0;
/// Between the plate and the card.
pub const CARD_GAP: f32 = 8.0;

const SIZE: f32 = 14.0;
const LINE: f32 = 18.0;
/// Small capitals: a lower-case letter is a capital this much smaller.
const SMALL: f32 = 0.8;
const HEAD_SIZE: f32 = 11.5;
const HEAD_LINE: f32 = 15.0;
const PAD_X: f32 = 12.0;
const MOD_GAP: f32 = 4.0;
/// The separator art is 221 by 8.
const SEP: (f32, f32) = (221.0, 8.0);
const SEP_MARGIN: f32 = 3.0;
/// The sockets' frame: its height, the circle's diameter, the link's length, the gap between groups.
const SOCKET_BOX: f32 = 22.0;
const SOCKET_D: f32 = 11.0;
const SOCKET_LINK: f32 = 5.0;
const SOCKET_GAP: f32 = 8.0;
const SOCKET_PAD: f32 = 9.0;

/// The advanced look is forced on (a render, `--alt`), whatever the keyboard says.
pub static FORCE_ADVANCED: AtomicBool = AtomicBool::new(false);

/// Whether the player holds Alt. The game has the focus, not this program, so the key is polled
/// from Windows rather than read from egui.
pub fn alt_held() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU};
    FORCE_ADVANCED.load(SeqCst) || unsafe { GetAsyncKeyState(i32::from(VK_MENU.0)) } < 0
}

const LABEL: Color32 = Color32::from_rgb(0x7f, 0x7f, 0x7f);
const VALUE: Color32 = Color32::WHITE;
const MOD: Color32 = Color32::from_rgb(0x88, 0x88, 0xff);
const CRAFTED: Color32 = Color32::from_rgb(0xb4, 0xb4, 0xff);
const FRACTURED: Color32 = Color32::from_rgb(0xa2, 0x91, 0x62);
const HEAD: Color32 = Color32::from_rgb(0x77, 0x77, 0x7d);
const RED: Color32 = Color32::from_rgb(0xd2, 0x00, 0x00);

/// The name's colour, the frame's, and the plate's tint: the game's.
fn colours(rarity: ListedRarity) -> (Color32, Color32, Color32) {
    let rgb = |v: u32| Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
    let (name, edge, band) = match rarity {
        ListedRarity::Normal | ListedRarity::Other => (0xc8c8c8, 0x6b6b6b, 0x2b2b2d),
        ListedRarity::Magic => (0x8888ff, 0x4e4e9c, 0x1c1c3a),
        ListedRarity::Rare => (0xffff77, 0x8c7a32, 0x2f2810),
        ListedRarity::Unique => (0xaf6025, 0x7a4a22, 0x2c1a0c),
        ListedRarity::Gem => (0x1ba29b, 0x1d6e6a, 0x0c2624),
        ListedRarity::Currency => (0xaa9e82, 0x6c6350, 0x24211a),
    };
    (rgb(name), rgb(edge), rgb(band))
}

/// The game's art, built in: (name, bytes).
const ART: [(&str, &[u8]); 30] = [
    ("WhiteLeft", include_bytes!("../../assets/game-ui/ItemsHeaderWhiteLeft.png")),
    ("WhiteMiddle", include_bytes!("../../assets/game-ui/ItemsHeaderWhiteMiddle.png")),
    ("WhiteRight", include_bytes!("../../assets/game-ui/ItemsHeaderWhiteRight.png")),
    ("MagicLeft", include_bytes!("../../assets/game-ui/ItemsHeaderMagicLeft.png")),
    ("MagicMiddle", include_bytes!("../../assets/game-ui/ItemsHeaderMagicMiddle.png")),
    ("MagicRight", include_bytes!("../../assets/game-ui/ItemsHeaderMagicRight.png")),
    ("RareSingleLineLeft", include_bytes!("../../assets/game-ui/ItemsHeaderRareSingleLineLeft.png")),
    ("RareSingleLineMiddle", include_bytes!("../../assets/game-ui/ItemsHeaderRareSingleLineMiddle.png")),
    ("RareSingleLineRight", include_bytes!("../../assets/game-ui/ItemsHeaderRareSingleLineRight.png")),
    ("RareLeft", include_bytes!("../../assets/game-ui/ItemsHeaderRareLeft.png")),
    ("RareMiddle", include_bytes!("../../assets/game-ui/ItemsHeaderRareMiddle.png")),
    ("RareRight", include_bytes!("../../assets/game-ui/ItemsHeaderRareRight.png")),
    ("UniqueSingleLineLeft", include_bytes!("../../assets/game-ui/ItemsHeaderUniqueSingleLineLeft.png")),
    ("UniqueSingleLineMiddle", include_bytes!("../../assets/game-ui/ItemsHeaderUniqueSingleLineMiddle.png")),
    ("UniqueSingleLineRight", include_bytes!("../../assets/game-ui/ItemsHeaderUniqueSingleLineRight.png")),
    ("UniqueLeft", include_bytes!("../../assets/game-ui/ItemsHeaderUniqueLeft.png")),
    ("UniqueMiddle", include_bytes!("../../assets/game-ui/ItemsHeaderUniqueMiddle.png")),
    ("UniqueRight", include_bytes!("../../assets/game-ui/ItemsHeaderUniqueRight.png")),
    ("sep-normal", include_bytes!("../../assets/game-ui/seperator-normal.png")),
    ("sep-magic", include_bytes!("../../assets/game-ui/seperator-magic.png")),
    ("sep-rare", include_bytes!("../../assets/game-ui/seperator-rare.png")),
    ("sep-unique", include_bytes!("../../assets/game-ui/seperator-unique.png")),
    ("sep-gem", include_bytes!("../../assets/game-ui/seperator-gem.png")),
    ("sep-currency", include_bytes!("../../assets/game-ui/seperator-currency.png")),
    ("shaper", include_bytes!("../../assets/game-ui/shaper-symbol.png")),
    ("elder", include_bytes!("../../assets/game-ui/elder-symbol.png")),
    ("crusader", include_bytes!("../../assets/game-ui/crusader-symbol.png")),
    ("hunter", include_bytes!("../../assets/game-ui/hunter-symbol.png")),
    ("redeemer", include_bytes!("../../assets/game-ui/redeemer-symbol.png")),
    ("warlord", include_bytes!("../../assets/game-ui/warlord-symbol.png")),
];
const MORE_ART: [(&str, &[u8]); 2] = [
    ("fractured", include_bytes!("../../assets/game-ui/fractured-symbol.png")),
    ("synthesised", include_bytes!("../../assets/game-ui/synthetic-symbol.png")),
];

/// One piece of art as a texture, decoded the first time it is asked for.
fn art(pen: &Pen<'_>, name: &str) -> Option<TextureId> {
    pen.p?;
    let id = eframe::egui::Id::unique(("zenith-game-ui", name));
    if let Some(handle) = pen.ctx.data(|data| data.get_temp::<eframe::egui::TextureHandle>(id)) {
        return Some(handle.id());
    }
    let bytes = ART.iter().chain(MORE_ART.iter()).find(|(key, _)| *key == name)?.1;
    let image = super::icons::decode(bytes).ok()?;
    let handle = pen.ctx.load_texture(format!("game-ui-{name}"), image, TextureOptions::LINEAR);
    let texture = handle.id();
    pen.ctx.data_mut(|data| data.insert_temp(id, handle));
    Some(texture)
}

fn image(pen: &Pen<'_>, texture: Option<TextureId>, rect: Rect, u: f32) {
    if let Some(texture) = texture {
        pen.add(Shape::image(texture, rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(u, 1.0)), Color32::WHITE));
    }
}

/// Fontin SmallCaps sets a little smaller than the panel's font in fake small capitals.
const FX_SCALE: f32 = 1.12;

/// The game's font is loaded and the card may use it.
fn fx(pen: &Pen<'_>) -> bool {
    crate::fontin::live(pen.ctx, |ctx| super::install_fonts(ctx))
}

/// The baseline of a line of `size` in a box of `line`, for the font the card is set in.
fn bl(pen: &Pen<'_>, size: f32, line: f32) -> f32 {
    if fx(pen) { pen.baseline(crate::fontin::metrics_em(), size * FX_SCALE, line) } else { pen.bl(size, line) }
}

/// `runs` on one line, in small capitals: Fontin's own when it is loaded, else a lower-case letter
/// is a smaller capital of the panel's font.
fn caps(pen: &Pen<'_>, runs: &[(&str, Color32)], f: F, size: f32) -> Txt {
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    job.round_output_to_gui = false;
    job.keep_trailing_whitespace = true;
    let own = fx(pen);
    for (text, colour) in runs.iter().filter(|_| own) {
        let format = eframe::egui::TextFormat {
            font_id: eframe::egui::FontId::new(size * FX_SCALE, crate::fontin::family()),
            color: *colour,
            ..Default::default()
        };
        job.append(text, 0.0, format);
    }
    let runs = if own { &[][..] } else { runs };
    for (text, colour) in runs {
        let mut buffer = String::new();
        let mut small = false;
        let flush = |job: &mut LayoutJob, buffer: &mut String, small: bool| {
            if !buffer.is_empty() {
                job.append(buffer, 0.0, f.format(if small { size * SMALL } else { size }, *colour, 0.0));
                buffer.clear();
            }
        };
        for c in text.chars() {
            let lower = c.is_lowercase();
            if lower != small {
                flush(&mut job, &mut buffer, small);
                small = lower;
            }
            buffer.extend(c.to_uppercase());
        }
        flush(&mut job, &mut buffer, small);
    }
    let galley = pen.layout(job.clone());
    let (w, base) = match galley.rows.first() {
        Some(row) => (
            row.row.glyphs.iter().map(|glyph| glyph.advance_width).sum(),
            row.pos.y + row.row.glyphs.first().map_or(0.0, |glyph| glyph.pos.y),
        ),
        None => (0.0, 0.0),
    };
    Txt { job, w, base }
}

/// One text broken at its spaces into lines no wider than `max`.
fn wrap_with(pen: &Pen<'_>, text: &str, colour: Color32, font: F, size: f32, max: f32) -> Vec<Txt> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let longer = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
        if !line.is_empty() && caps(pen, &[(&longer, colour)], font, size).w > max {
            lines.push(caps(pen, &[(&line, colour)], font, size));
            line = word.to_owned();
        } else {
            line = longer;
        }
    }
    if !line.is_empty() {
        lines.push(caps(pen, &[(&line, colour)], font, size));
    }
    lines
}

fn wrap(pen: &Pen<'_>, text: &str, colour: Color32, size: f32, max: f32) -> Vec<Txt> {
    wrap_with(pen, text, colour, F::T500, size, max)
}

/// The small dim line over a mod, short: `P T2 "Dazzling"`, `Crafted S`, `Fractured P T1
/// "Name"`. `None` when the mod's side is not known.
pub fn header(m: &ListedMod) -> Option<String> {
    let side = match m.affix? {
        Affix::Prefix => "P",
        Affix::Suffix => "S",
    };
    let mut text = match m.kind {
        ListedModKind::Crafted => format!("Crafted {side}"),
        ListedModKind::Fractured => format!("Fractured {side}"),
        _ => side.to_owned(),
    };
    if let Some(tier) = m.tier {
        text.push_str(&format!(" T{tier}"));
    }
    if !m.name.is_empty() {
        text.push_str(&format!(" \"{}\"", m.name));
    }
    Some(text)
}

/// "Level 68, 112 Int" as the game colours it: the words grey, the numbers white.
fn requirement_runs(text: &str) -> Vec<(String, Color32)> {
    let mut runs = vec![("Requires ".to_owned(), LABEL)];
    let text = text.trim_start_matches("Requires ").trim();
    for (index, part) in text.split(", ").enumerate() {
        if index > 0 {
            runs.push((", ".into(), LABEL));
        }
        let part = part.trim();
        match part.rsplit_once(' ') {
            Some((word, number)) if number.chars().all(|c| c.is_ascii_digit()) => {
                runs.push((format!("{word} "), LABEL));
                runs.push((number.into(), VALUE));
            }
            _ => match part.split_once(' ') {
                Some((number, word)) if number.chars().all(|c| c.is_ascii_digit()) => {
                    runs.push((number.into(), VALUE));
                    runs.push((format!(" {word}"), LABEL));
                }
                _ => runs.push((part.into(), VALUE)),
            },
        }
    }
    runs
}

/// A line of the card: its runs, or the small header over a mod.
enum Line {
    Runs(Vec<(String, Color32)>),
    Wrapped(Txt),
    Head(Txt),
    /// The sockets, "R-G-B B", in their frame.
    Sockets(String),
    Gap,
}

/// A socket's colour from its letter: the game's.
fn socket_colour(letter: &str) -> (Color32, Color32) {
    let rgb = |v: u32| Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8);
    match letter {
        "R" => (rgb(0xc8423a), rgb(0x6e211d)),
        "G" => (rgb(0x6aa84f), rgb(0x365a28)),
        "B" => (rgb(0x5a7ee0), rgb(0x2c3f77)),
        "W" => (rgb(0xe0e0e0), rgb(0x808080)),
        // Abyss: a dark socket with a light ring. Delve and anything else: a muted violet.
        "A" => (rgb(0x2a2a2e), rgb(0xb8b8c0)),
        _ => (rgb(0x6a5a8a), rgb(0x362c4a)),
    }
}

/// The sockets as groups of letters: "R-G B" is [[R, G], [B]].
fn socket_groups(text: &str) -> Vec<Vec<&str>> {
    text.split_whitespace().map(|group| group.split('-').filter(|s| !s.is_empty()).collect()).collect()
}

/// The sockets' row width: circles, links between linked ones, gaps between groups.
fn socket_width(groups: &[Vec<&str>]) -> f32 {
    let mut w = 0.0;
    for (g, group) in groups.iter().enumerate() {
        if g > 0 {
            w += SOCKET_GAP;
        }
        w += group.len() as f32 * SOCKET_D + group.len().saturating_sub(1) as f32 * SOCKET_LINK;
    }
    w
}


/// The card's sections, in order: a separator goes between two of them.
fn sections(pen: &Pen<'_>, item: &ListedItem, inner: f32, advanced: bool) -> Vec<Vec<Line>> {
    let mut sections = Vec::new();
    let properties: Vec<Line> = item
        .properties
        .iter()
        .map(|property| {
            if property.label.is_empty() {
                Line::Runs(vec![(property.value.clone(), LABEL)])
            } else {
                Line::Runs(vec![(format!("{}: ", property.label), LABEL), (property.value.clone(), if property.augmented { MOD } else { VALUE })])
            }
        })
        .collect();
    sections.push(properties);
    if !item.requirements.is_empty() {
        sections.push(vec![Line::Runs(requirement_runs(&item.requirements))]);
    }
    if !item.sockets.is_empty() {
        sections.push(vec![Line::Sockets(item.sockets.clone())]);
    }
    if let Some(level) = item.item_level {
        sections.push(vec![Line::Runs(vec![("Item Level: ".into(), LABEL), (level.to_string(), VALUE)])]);
    }
    for (at, block) in item.blocks.iter().enumerate() {
        let colour_of = |kind: ListedModKind| match kind {
            ListedModKind::Enchant | ListedModKind::Crafted => CRAFTED,
            ListedModKind::Fractured => FRACTURED,
            _ => MOD,
        };
        let mut lines = Vec::new();
        if !advanced {
            // The game without Alt: the site's lines in its order, no headers.
            let own: Vec<String>;
            let shown = match item.plain.get(at) {
                Some(shown) => shown,
                None => {
                    own = block.iter().flat_map(|m| m.lines.iter().cloned()).collect();
                    &own
                }
            };
            for text in shown {
                let kind = block.iter().find(|m| m.lines.contains(text)).or(block.first()).map_or(ListedModKind::Explicit, |m| m.kind);
                lines.extend(wrap(pen, text, colour_of(kind), SIZE, inner).into_iter().map(Line::Wrapped));
            }
            sections.push(lines);
            continue;
        }
        // Advanced: prefixes, then suffixes, each mod under its header.
        let mut mods: Vec<&ListedMod> = block.iter().collect();
        if mods.iter().any(|m| m.affix.is_some()) {
            mods.sort_by_key(|m| match m.affix {
                Some(Affix::Prefix) => 0,
                Some(Affix::Suffix) => 1,
                None => 2,
            });
            let count = |side| mods.iter().filter(|m| m.affix == Some(side)).count();
            let (prefixes, suffixes) = (count(Affix::Prefix), count(Affix::Suffix));
            let text = format!(
                "{prefixes} {} \u{b7} {suffixes} {}",
                if prefixes == 1 { "Prefix" } else { "Prefixes" },
                if suffixes == 1 { "Suffix" } else { "Suffixes" }
            );
            lines.push(Line::Head(caps(pen, &[(&text, HEAD)], F::T400, HEAD_SIZE)));
        }
        let mut prev_side = None;
        for (index, m) in mods.into_iter().enumerate() {
            let head = header(m);
            if prev_side == Some(Affix::Prefix) && m.affix == Some(Affix::Suffix) {
                // The two groups read apart: the card's separator between them.
                sections.push(std::mem::take(&mut lines));
            }
            prev_side = m.affix;
            if index > 0 && head.is_none() {
                lines.push(Line::Gap);
            }
            if let Some(head) = head {
                lines.extend(wrap_with(pen, &head, HEAD, F::T400, HEAD_SIZE, inner).into_iter().map(Line::Head));
            }
            let colour = colour_of(m.kind);
            for text in m.ranged_lines() {
                lines.extend(wrap(pen, &text, colour, SIZE, inner).into_iter().map(Line::Wrapped));
            }
        }
        sections.push(lines);
    }
    let mut last = Vec::new();
    if !item.identified {
        last.push(Line::Runs(vec![("Unidentified".into(), RED)]));
    }
    for (on, word) in [(item.mirrored, "Mirrored"), (item.split, "Split")] {
        if on {
            last.push(Line::Runs(vec![(word.into(), MOD)]));
        }
    }
    if item.corrupted {
        last.push(Line::Runs(vec![("Corrupted".into(), RED)]));
    }
    sections.push(last);
    sections.retain(|section| !section.is_empty());
    sections
}

/// The card's height, shadow included, without drawing it.
pub(super) fn height(pen: &Pen<'_>, item: &ListedItem, advanced: bool) -> f32 {
    let quiet = Pen { ctx: pen.ctx, p: None, o: pen.o, hair: pen.hair, ppp: pen.ppp };
    lay(&quiet, item, 0.0, 0.0, advanced) + SHADOW
}

/// Draws the card with its top-left corner at `x`, `y` (in `pen`'s points) and gives its height.
pub(super) fn draw(pen: &Pen<'_>, item: &ListedItem, x: f32, y: f32, advanced: bool) -> f32 {
    let h = height(pen, item, advanced) - SHADOW;
    let (_, edge, _) = colours(item.rarity);
    let frame = pen.r(x, y, CARD_W, h);
    pen.add(Shape::rect_filled(frame.translate(vec2(0.0, SHADOW)), 4, Color32::from_rgba_unmultiplied(4, 7, 16, 179)));
    pen.add(Shape::rect_filled(frame, 2, Color32::from_rgba_unmultiplied(0, 0, 0, 230)));
    let border = edge.lerp_to_gamma(Color32::BLACK, 0.4);
    pen.boxed(frame, 2, Color32::TRANSPARENT, border);
    lay(pen, item, x, y, advanced)
}

/// The name plate and the body, from `x`, `y`; gives the height.
fn lay(pen: &Pen<'_>, item: &ListedItem, x: f32, y: f32, advanced: bool) -> f32 {
    let (name_colour, edge, band) = colours(item.rarity);
    let named = matches!(item.rarity, ListedRarity::Rare | ListedRarity::Unique) && !item.name.is_empty() && !item.base.is_empty();
    let plate_h = if named { 54.0 } else { 34.0 };
    let w = CARD_W;

    // ---- the plate: the tint, then the art's left cap, tiled middle and right cap ----
    pen.fill(x + 1.0, y + 1.0, w - 2.0, plate_h - 1.0, 0, band);
    let stem = match item.rarity {
        ListedRarity::Normal => Some("White"),
        ListedRarity::Magic => Some("Magic"),
        ListedRarity::Rare => Some(if named { "Rare" } else { "RareSingleLine" }),
        ListedRarity::Unique => Some(if named { "Unique" } else { "UniqueSingleLine" }),
        _ => None,
    };
    match stem {
        Some(stem) => {
            let (art_w, art_h) = if named { (70.0, 87.0) } else { (46.0, 54.0) };
            let cap = art_w * plate_h / art_h;
            let (left, right) = (x + 1.0, x + w - 1.0);
            image(pen, art(pen, &format!("{stem}Left")), pen.r(left, y + 1.0, cap, plate_h - 1.0), 1.0);
            image(pen, art(pen, &format!("{stem}Right")), pen.r(right - cap, y + 1.0, cap, plate_h - 1.0), 1.0);
            let middle = art(pen, &format!("{stem}Middle"));
            let mut at = left + cap;
            while at < right - cap - 0.01 {
                let piece = cap.min(right - cap - at);
                image(pen, middle, pen.r(at, y + 1.0, piece, plate_h - 1.0), piece / cap);
                at += piece;
            }
        }
        None => pen.fill(x + 1.0, y + plate_h - 1.0, w - 2.0, 1.0, 0, edge),
    }
    // The influences at the plate's ends: one shows at both.
    let mut symbols: Vec<&str> = item.influences.iter().map(String::as_str).filter(|key| matches!(*key, "shaper" | "elder" | "crusader" | "hunter" | "redeemer" | "warlord")).collect();
    if symbols.is_empty() && item.fractured {
        symbols.push("fractured");
    }
    if symbols.is_empty() && item.synthesised {
        symbols.push("synthesised");
    }
    if let Some(first) = symbols.first().copied() {
        let second = symbols.get(1).copied().unwrap_or(first);
        let (size, inset) = if named { (24.0, 9.0) } else { (20.0, 6.0) };
        let top = y + (plate_h - size) / 2.0;
        image(pen, art(pen, first), pen.r(x + inset, top, size, size), 1.0);
        image(pen, art(pen, second), pen.r(x + w - inset - size, top, size, size), 1.0);
    }
    let room = w - 2.0 * if named { 46.0 } else { 34.0 };
    let name_size = 17.0;
    let names: Vec<&str> = if named { vec![&item.name, &item.base] } else { vec![if item.name.is_empty() { &item.base } else { &item.name }] };
    let line_h = 21.0;
    let first_line = y + (plate_h - line_h * names.len() as f32) / 2.0;
    for (index, text) in names.iter().enumerate() {
        let t = caps(pen, &[(text, name_colour)], F::T600, name_size);
        let size = if t.w > room { name_size * room / t.w } else { name_size };
        let t = caps(pen, &[(text, name_colour)], F::T600, size);
        pen.draw(&t, x + (w - t.w) / 2.0, first_line + index as f32 * line_h + bl(pen, name_size, line_h));
    }

    // ---- the body ----
    let inner = w - 2.0 * PAD_X;
    let mut at = y + plate_h + 6.0;
    let sep = art(pen, &format!(
        "sep-{}",
        match item.rarity {
            ListedRarity::Normal | ListedRarity::Other => "normal",
            ListedRarity::Magic => "magic",
            ListedRarity::Rare => "rare",
            ListedRarity::Unique => "unique",
            ListedRarity::Gem => "gem",
            ListedRarity::Currency => "currency",
        }
    ));
    for (index, section) in sections(pen, item, inner, advanced).into_iter().enumerate() {
        if index > 0 {
            at += SEP_MARGIN;
            let sep_w = SEP.0.min(inner);
            image(pen, sep, pen.r(x + (w - sep_w) / 2.0, at, sep_w, SEP.1), 1.0);
            at += SEP.1 + SEP_MARGIN;
        }
        for line in section {
            match line {
                Line::Runs(runs) => {
                    let runs: Vec<(&str, Color32)> = runs.iter().map(|(text, colour)| (text.as_str(), *colour)).collect();
                    let t = caps(pen, &runs, F::T500, SIZE);
                    pen.draw(&t, x + (w - t.w) / 2.0, at + bl(pen, SIZE, LINE));
                    at += LINE;
                }
                Line::Wrapped(t) => {
                    pen.draw(&t, x + (w - t.w) / 2.0, at + bl(pen, SIZE, LINE));
                    at += LINE;
                }
                Line::Head(t) => {
                    at += 2.0;
                    pen.draw(&t, x + (w - t.w) / 2.0, at + bl(pen, HEAD_SIZE, HEAD_LINE));
                    at += HEAD_LINE;
                }
                Line::Sockets(text) => {
                    let groups = socket_groups(&text);
                    let row = socket_width(&groups);
                    let frame_w = row + 2.0 * SOCKET_PAD;
                    let top = at + 2.0;
                    let frame = pen.r(x + (w - frame_w) / 2.0, top, frame_w, SOCKET_BOX);
                    pen.boxed(frame, 5, Color32::from_rgba_unmultiplied(255, 255, 255, 8), edge.lerp_to_gamma(Color32::BLACK, 0.4));
                    let mid = top + SOCKET_BOX / 2.0;
                    let mut cx = x + (w - row) / 2.0;
                    for (g, group) in groups.iter().enumerate() {
                        if g > 0 {
                            cx += SOCKET_GAP;
                        }
                        for (s, letter) in group.iter().enumerate() {
                            if s > 0 {
                                pen.fill(cx, mid - 1.0, SOCKET_LINK, 2.0, 0, Color32::from_rgb(0x8a, 0x8a, 0x8a));
                                cx += SOCKET_LINK;
                            }
                            let (fill, ring) = socket_colour(letter);
                            let centre = pen.r(cx + SOCKET_D / 2.0, mid, 0.0, 0.0).min;
                            pen.add(Shape::circle_filled(centre, SOCKET_D / 2.0, fill));
                            pen.add(Shape::circle_stroke(centre, SOCKET_D / 2.0 - 0.5, eframe::egui::Stroke::new(pen.hair, ring)));
                            cx += SOCKET_D;
                        }
                    }
                    at += SOCKET_BOX + 4.0;
                }
                Line::Gap => at += MOD_GAP,
            }
        }
    }
    at + 8.0 - y
}
