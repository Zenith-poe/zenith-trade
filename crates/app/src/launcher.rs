//! The quick menu: a small row of tools, shown in the overlay window over the game, and the
//! saved-searches widget it opens. This module is the pure part: what the entries are, where
//! each one opens, how the buttons are laid out, and which one a point is on. The painting is
//! in `panel`, the clicks are handled in `main`.

use eframe::egui::{Rect, pos2, vec2};

/// What an entry of the quick menu does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Replaces the menu with the saved searches.
    Searches,
    /// Opens the Settings window, on this page (the one it was last on when none is named).
    Settings(Option<&'static str>),
}

/// The little picture before a label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Search,
    Filter,
    Gear,
}

/// One button of the quick menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    pub label: &'static str,
    pub icon: Icon,
    pub target: Target,
}

/// The tools, in order. A new tool is a new line here.
const ITEMS: [Item; 3] = [
    Item { label: "Stash searches", icon: Icon::Search, target: Target::Searches },
    Item { label: "Map filter", icon: Icon::Filter, target: Target::Settings(Some("Map filter")) },
    Item { label: "Settings", icon: Icon::Gear, target: Target::Settings(None) },
];

pub fn items() -> &'static [Item] {
    &ITEMS
}

/// What the button at `index` does.
pub fn target(index: usize) -> Option<Target> {
    ITEMS.get(index).map(|item| item.target)
}

/// Most searches the widget lists.
pub const MAX_SEARCHES: usize = 16;
/// Buttons side by side in the widget.
pub const COLUMNS: usize = 2;

/// Where a row of buttons of these widths goes between `left` and `right`: `(x, width)` of each,
/// `gap` apart, centred. Too wide for the room, they all shrink in proportion.
pub fn row(widths: &[f32], left: f32, right: f32, gap: f32) -> Vec<(f32, f32)> {
    if widths.is_empty() {
        return Vec::new();
    }
    let gaps = gap * (widths.len() - 1) as f32;
    let room = (right - left - gaps).max(0.0);
    let total: f32 = widths.iter().sum();
    let scale = if total > room && total > 0.0 { room / total } else { 1.0 };
    let used = total * scale + gaps;
    let mut x = left + ((right - left - used) / 2.0).max(0.0);
    widths
        .iter()
        .map(|width| {
            let w = width * scale;
            let at = (x, w);
            x += w + gap;
            at
        })
        .collect()
}

/// The cells of a grid of `count` buttons in `COLUMNS` columns across `width` from `(left, top)`,
/// each `height` tall, `gap` apart, filled row by row.
pub fn grid(count: usize, left: f32, top: f32, width: f32, height: f32, gap: f32) -> Vec<Rect> {
    let cell = (width - gap * (COLUMNS - 1) as f32) / COLUMNS as f32;
    (0..count)
        .map(|n| {
            let (col, line) = (n % COLUMNS, n / COLUMNS);
            Rect::from_min_size(pos2(left + col as f32 * (cell + gap), top + line as f32 * (height + gap)), vec2(cell, height))
        })
        .collect()
}

/// The height the cells of `grid` take.
pub fn grid_height(count: usize, height: f32, gap: f32) -> f32 {
    let lines = count.div_ceil(COLUMNS);
    if lines == 0 { 0.0 } else { lines as f32 * height + (lines - 1) as f32 * gap }
}

/// Which cell `at` is in; the right and bottom edges belong to what comes next. (The panel hit-tests its
/// zones; this is the same rule for a grid, kept for the tests.)
#[cfg(test)]
pub fn cell_at(cells: &[Rect], at: eframe::egui::Pos2) -> Option<usize> {
    cells.iter().position(|c| c.min.x <= at.x && at.x < c.max.x && c.min.y <= at.y && at.y < c.max.y)
}

/// Where the menu's top left goes: centred on the top of the game's window (`left`, `top`,
/// `right` in physical pixels), `drop` pixels under its top edge, and kept inside `work`
/// (left, top, right, bottom) when the game's window is not.
pub fn anchor(game: (i32, i32, i32), size: (i32, i32), drop: i32, work: (i32, i32, i32, i32)) -> (i32, i32) {
    let (left, top, right) = game;
    let x = (left + (right - left - size.0) / 2).min(work.2 - size.0).max(work.0);
    let y = (top + drop).min(work.3 - size.1).max(work.1);
    (x, y)
}
