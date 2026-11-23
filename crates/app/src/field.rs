//! The number fields of the panel and the typing in them: the two of a searched row, its minimum
//! and its maximum, and the one of an option chip that holds a number, the item level or the
//! quality, while the chip is on.
//!
//! `Editor` is the field being typed in: keys go in, and its text, caret and selection come out,
//! with the value it holds once it is left. `Fields` is every field of the panel for one show:
//! which one is being typed in, what was left in a field that is no number, and what a key or a
//! turn of the wheel does to the check, as `Action`s. Nothing here knows of a window or of who
//! has the keyboard.

use crate::check::{Action, Check, ItemOption, stepped_by};

/// Which end of a filter a field holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    Min,
    Max,
}

/// One field: an end of a filter of `Check::filters`, or the number of an option chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Min(usize),
    Max(usize),
    /// The least item level the search asks for.
    ItemLevel,
    /// The least quality.
    Quality,
}

impl Target {
    pub const fn min(filter: usize) -> Self {
        Target::Min(filter)
    }

    pub const fn max(filter: usize) -> Self {
        Target::Max(filter)
    }

    /// The filter a field belongs to; `None` for the number of a chip.
    pub fn filter(self) -> Option<usize> {
        match self {
            Target::Min(filter) | Target::Max(filter) => Some(filter),
            Target::ItemLevel | Target::Quality => None,
        }
    }

    /// Which end of its filter a field holds; `None` for the number of a chip.
    pub fn bound(self) -> Option<Bound> {
        match self {
            Target::Min(_) => Some(Bound::Min),
            Target::Max(_) => Some(Bound::Max),
            Target::ItemLevel | Target::Quality => None,
        }
    }

    /// The number of a chip: whole, never negative, and short.
    pub fn of_chip(self) -> bool {
        self.filter().is_none()
    }

    /// The most the number of a chip can be: an item level goes to 100, and no quality has three
    /// figures.
    fn most(self) -> u32 {
        if self == Target::ItemLevel { 100 } else { 99 }
    }
}

/// A key pressed while a field has the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A character typed. Only digits, the decimal point (a comma is one too) and the minus sign
    /// are ever taken.
    Char(char),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    /// Select everything.
    All,
    Tab,
    /// Shift+Tab.
    BackTab,
    Enter,
    Escape,
}

/// What a field holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Typed {
    /// Nothing: no bound.
    Empty,
    Number(f64),
    /// Something that is no number yet: a sign or a point alone.
    NotANumber,
}

/// How a field is left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// For the next field, or the one before.
    Next,
    Previous,
    /// To search again.
    Search,
    /// For nothing else.
    Leave,
}

/// The longest text a field takes, and the longest the number of a chip is.
const LONGEST: usize = 9;
const LONGEST_OF_CHIP: usize = 3;

/// What `text` holds.
pub fn read(text: &str) -> Typed {
    if text.is_empty() {
        return Typed::Empty;
    }
    match text.parse::<f64>() {
        // Not "-0".
        Ok(value) if value.is_finite() => Typed::Number(value + 0.0),
        _ => Typed::NotANumber,
    }
}

/// A bound as a field shows it: the shortest text that reads back as the same number, so that a
/// field opened and left untouched changes nothing.
pub fn written(value: f64) -> String {
    format!("{}", value + 0.0)
}

/// A number with at most `decimals` decimals and no trailing zero (`315.4`, `14.5`, `120`).
pub fn rounded(value: f64, decimals: usize) -> String {
    let text = format!("{:.decimals$}", value + 0.0);
    let text = if text.contains('.') { text.trim_end_matches('0').trim_end_matches('.').to_owned() } else { text };
    if text == "-0" { "0".to_owned() } else { text }
}

/// Whether a field may hold `text`: digits with at most one decimal point, after a minus sign
/// when the stat can be negative. The number of a chip is digits and nothing else, three at most.
fn fits(text: &str, signed: bool, of_chip: bool) -> bool {
    let plain = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    if of_chip {
        return text.len() <= LONGEST_OF_CHIP && plain(text);
    }
    let digits = match text.strip_prefix('-') {
        Some(rest) if signed => rest,
        Some(_) => return false,
        None => text,
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    text.len() <= LONGEST && plain(whole) && plain(fraction)
}

/// The field being typed in. Its text is plain ASCII, so a position in it is a count of
/// characters and of bytes alike.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editor {
    target: Target,
    text: String,
    /// Where the next character goes: 0 is before the first one.
    caret: usize,
    /// The other end of the selection; the caret itself when nothing is selected.
    anchor: usize,
    /// The stat can be negative: a minus sign is taken.
    signed: bool,
}

impl Editor {
    /// The field as a click gives it the caret: its content selected.
    pub fn open(target: Target, text: &str, signed: bool) -> Self {
        Self { target, text: text.to_owned(), caret: text.len(), anchor: 0, signed }
    }

    pub fn target(&self) -> Target {
        self.target
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    /// The selected part, from its first character to after its last; `None` when nothing is.
    pub fn selection(&self) -> Option<(usize, usize)> {
        (self.caret != self.anchor).then(|| (self.caret.min(self.anchor), self.caret.max(self.anchor)))
    }

    /// The text selected, for the clipboard; `None` when nothing is.
    pub fn selected_text(&self) -> Option<&str> {
        self.selection().map(|(start, end)| &self.text[start..end])
    }

    /// What the field holds now.
    pub fn typed(&self) -> Typed {
        read(&self.text)
    }

    /// Replaces the content, and selects it.
    fn set(&mut self, text: &str) {
        *self = Self::open(self.target, text, self.signed);
    }

    /// The caret at `at`, nothing selected.
    fn place(&mut self, at: usize) {
        self.caret = at.min(self.text.len());
        self.anchor = self.caret;
    }

    /// The text with `start..end` replaced by `with`, when a field may hold it.
    fn replace(&mut self, start: usize, end: usize, with: &str) -> bool {
        let text = format!("{}{with}{}", &self.text[..start], &self.text[end..]);
        if !fits(&text, self.signed, self.target.of_chip()) {
            return false;
        }
        self.text = text;
        self.place(start + with.len());
        true
    }

    /// Takes a key. Returns how the field is left when the key leaves it; the text is then what
    /// is to be committed. A key the field has no use for changes nothing.
    pub fn key(&mut self, key: Key) -> Option<Exit> {
        let (start, end) = self.selection().unwrap_or((self.caret, self.caret));
        let selected = start != end;
        match key {
            Key::Char(typed) => {
                let typed = if typed == ',' { '.' } else { typed };
                if typed.is_ascii_digit() || typed == '.' || typed == '-' {
                    self.replace(start, end, typed.encode_utf8(&mut [0; 4]));
                }
            }
            Key::Backspace if selected => drop(self.replace(start, end, "")),
            Key::Backspace if start > 0 => drop(self.replace(start - 1, start, "")),
            Key::Delete if selected => drop(self.replace(start, end, "")),
            Key::Delete if end < self.text.len() => drop(self.replace(end, end + 1, "")),
            Key::Backspace | Key::Delete => {}
            // With a selection, to its end on that side.
            Key::Left => self.place(if selected { start } else { start.saturating_sub(1) }),
            Key::Right => self.place(if selected { end } else { end + 1 }),
            Key::Home => self.place(0),
            Key::End => self.place(self.text.len()),
            Key::All => self.set(&self.text.clone()),
            Key::Tab => return Some(Exit::Next),
            Key::BackTab => return Some(Exit::Previous),
            Key::Enter => return Some(Exit::Search),
            Key::Escape => return Some(Exit::Leave),
        }
        None
    }
}

/// Whether the panel shows a field: its filter is on, or its chip is there and on.
pub fn shown(check: &Check, target: Target) -> bool {
    match target {
        Target::Min(filter) | Target::Max(filter) => check.filters.get(filter).is_some_and(|filter| filter.on),
        Target::ItemLevel | Target::Quality => {
            let option = if target == Target::ItemLevel { ItemOption::ItemLevel } else { ItemOption::Quality };
            check.option_chips().iter().any(|chip| chip.option == option && chip.on)
        }
    }
}

/// Whether a searched row has number fields: a property always, a mod when it has a number. A mod
/// that is there or not ("Unaffected by Shocked Ground") is asked for as it is, and its text takes
/// the room of the fields; so is one of the choices of a stat (an id with `|`), whatever number
/// its text has.
pub fn has_fields(check: &Check, row: &crate::check::Row) -> bool {
    // A mod of a unique that did not roll (the item gives the ranges of the others) is there or
    // not: no number to ask.
    if crate::check::fixed_unique_mod(check, row) {
        return false;
    }
    let choice = row.filter.and_then(|filter| check.filters.get(filter)).is_some_and(|filter| filter.id.contains('|'));
    row.kind.is_none() || (!choice && row.lines.iter().any(|line| line.chars().any(|c| c.is_ascii_digit())))
}

/// The fields the panel shows, in the order Tab goes through them: the numbers of the chips that
/// are on, then the rows that are searched as the panel lists them, the minimum then the maximum.
pub fn order(check: &Check) -> Vec<Target> {
    let of_chips = [Target::ItemLevel, Target::Quality].into_iter().filter(|target| shown(check, *target));
    let of_rows = check
        .rows
        .iter()
        .filter(|row| check.leads(row) && has_fields(check, row))
        .filter_map(|row| row.filter)
        .filter(|filter| check.filters.get(*filter).is_some_and(|filter| filter.on))
        .flat_map(|filter| [Target::min(filter), Target::max(filter)]);
    of_chips.chain(of_rows).collect()
}

/// The bound a field holds.
fn bound_of(check: &Check, target: Target) -> Option<f64> {
    match target {
        Target::Min(filter) => check.filters.get(filter)?.min,
        Target::Max(filter) => check.filters.get(filter)?.max,
        Target::ItemLevel => check.options.item_level.map(f64::from),
        Target::Quality => check.options.quality.map(f64::from),
    }
}

/// Whether a field is flagged: it holds something that is no number, or its filter asks for a
/// minimum above its maximum, which flags both of its fields. The number of a chip never is.
pub fn invalid(check: &Check, target: Target) -> bool {
    let Some(filter) = target.filter().and_then(|filter| check.filters.get(filter)) else {
        return false;
    };
    let crossed = matches!((filter.min, filter.max), (Some(min), Some(max)) if min > max);
    crossed || bound_of(check, target).is_some_and(|bound| !bound.is_finite())
}

/// The action that gives a field this bound. A chip left without a number is switched off.
fn set(target: Target, bound: Option<f64>) -> Action {
    let whole = |bound: Option<f64>| bound.filter(|bound| bound.is_finite()).map(|bound| (bound.max(0.0) as u32).min(target.most()));
    match target {
        Target::Min(filter) => Action::SetMin(filter, bound),
        Target::Max(filter) => Action::SetMax(filter, bound),
        Target::ItemLevel => Action::SetItemLevel(whole(bound)),
        Target::Quality => Action::SetQuality(whole(bound)),
    }
}

/// What a key did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Keyed {
    /// What it does to the check, in order.
    pub actions: Vec<Action>,
    /// A field had the caret and none has it any more: the keyboard is no longer needed.
    pub left: bool,
}

/// Every field of the panel, for one show of it.
#[derive(Debug, Clone)]
pub struct Fields {
    active: Option<Editor>,
    /// What was left in a field that is no number, shown as it was typed. The check holds such
    /// a bound as a number that is none.
    odd: Vec<(Target, String)>,
    /// The part of a notch the wheel has turned so far, and over which field.
    turned: f32,
    turned_over: Option<Target>,
}

impl Default for Fields {
    fn default() -> Self {
        Self::new()
    }
}

impl Fields {
    /// No field has the caret and nothing is remembered.
    pub const fn new() -> Self {
        Self { active: None, odd: Vec::new(), turned: 0.0, turned_over: None }
    }

    /// The text selected in the field being typed in, for the clipboard.
    pub fn selected_text(&self) -> Option<String> {
        self.active.as_ref().and_then(Editor::selected_text).map(str::to_owned)
    }

    /// The field being typed in.
    pub fn active(&self) -> Option<&Editor> {
        self.active.as_ref()
    }

    /// As for a panel just shown.
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    /// What a field that is not being typed in shows; empty for no bound.
    pub fn text(&self, check: &Check, target: Target) -> String {
        match bound_of(check, target) {
            None => String::new(),
            Some(bound) if bound.is_finite() => match target {
                Target::Min(filter) | Target::Max(filter) if check.filters.get(filter).is_some_and(|row| crate::check::is_property(&row.id)) => {
                    rounded(bound, 1)
                }
                _ => written(bound),
            },
            Some(_) => self.odd.iter().find(|(odd, _)| *odd == target).map(|(_, text)| text.clone()).unwrap_or_default(),
        }
    }

    /// Gives a field the caret, its content selected. Only a field the panel shows can have it:
    /// one of a filter that is on, or of a chip that is. The field that had it must have been
    /// closed before.
    pub fn open(&mut self, check: &Check, target: Target) -> bool {
        if !shown(check, target) {
            return false;
        }
        let signed = target.filter().is_some_and(|filter| check.signed(filter));
        self.active = Some(Editor::open(target, &self.text(check, target), signed));
        true
    }

    /// Takes the caret from the field that has it. Returns the action that commits what the
    /// field holds; `None` when no field had the caret.
    pub fn close(&mut self) -> Option<Action> {
        let editor = self.active.take()?;
        let target = editor.target();
        self.odd.retain(|(odd, _)| *odd != target);
        let bound = match editor.typed() {
            Typed::Empty => None,
            Typed::Number(value) => Some(value),
            Typed::NotANumber => {
                self.odd.push((target, editor.text().to_owned()));
                Some(f64::NAN)
            }
        };
        Some(set(target, bound))
    }

    /// A key, while the panel has the keyboard. In a field it types, or leaves the field: what
    /// the field holds is committed then, Tab and Shift+Tab give the caret to the field after or
    /// before it, going round, and Enter searches again. Without a field Enter still searches.
    pub fn key(&mut self, check: &Check, key: Key) -> Keyed {
        let Some(editor) = self.active.as_mut() else {
            let actions = if key == Key::Enter { vec![Action::SearchAgain] } else { Vec::new() };
            return Keyed { actions, left: false };
        };
        let Some(exit) = editor.key(key) else {
            return Keyed::default();
        };
        let from = editor.target();
        let mut actions: Vec<Action> = self.close().into_iter().collect();
        let mut left = true;
        match exit {
            Exit::Next | Exit::Previous => {
                let order = order(check);
                let step = if exit == Exit::Next { 1 } else { order.len().saturating_sub(1) };
                let to = order.iter().position(|target| *target == from).map(|at| order[(at + step) % order.len()]);
                left = !to.is_some_and(|to| self.open(check, to));
            }
            Exit::Search => actions.push(Action::SearchAgain),
            Exit::Leave => {}
        }
        Keyed { actions, left }
    }

    /// The wheel turned by so many lines over a field: up is more. Returns what it does to the
    /// check once a whole notch is turned. The minimum steps from where it is, and from nothing
    /// only upward; the maximum steps once it has a value. The number of a chip steps by one
    /// a notch, between nothing and the most it can be. Over the field being typed in, what is
    /// typed steps and is committed at once.
    pub fn wheel(&mut self, check: &Check, target: Target, lines: f32) -> Option<Action> {
        if self.turned_over != Some(target) {
            (self.turned, self.turned_over) = (0.0, Some(target));
        }
        self.turned += lines;
        let notches = self.turned.trunc();
        self.turned -= notches;
        let notches = notches as i32;
        if notches == 0 || !shown(check, target) {
            return None;
        }
        // Where a notch takes a value.
        let fine = match target {
            Target::Min(filter) | Target::Max(filter) => check.fine(filter),
            _ => false,
        };
        let step = |value: f64| {
            if target.of_chip() { (value + f64::from(notches)).clamp(0.0, f64::from(target.most())) } else { stepped_by(value, notches, fine) }
        };
        // From nothing, and from what is no number: a minimum, upward.
        let fresh = || (target.bound() != Some(Bound::Max) && notches > 0).then(|| step(0.0));
        if let Some(editor) = self.active.as_mut().filter(|editor| editor.target() == target) {
            let next = match editor.typed() {
                Typed::Number(value) => step(value),
                Typed::Empty | Typed::NotANumber => fresh()?,
            };
            editor.set(&written(next));
            self.odd.retain(|(odd, _)| *odd != target);
            return Some(set(target, Some(next)));
        }
        match (bound_of(check, target), target) {
            (Some(bound), _) if !bound.is_finite() => {
                let next = fresh()?;
                self.odd.retain(|(odd, _)| *odd != target);
                Some(set(target, Some(next)))
            }
            (_, Target::Min(filter)) => Some(Action::StepMin(filter, notches)),
            (_, Target::Max(filter)) => Some(Action::StepMax(filter, notches)),
            (bound, Target::ItemLevel | Target::Quality) => Some(set(target, Some(step(bound?)))),
        }
    }

    /// The field that has the caret is no longer one the panel shows: its filter is gone or off,
    /// or its chip is.
    pub fn stray(&self, check: Option<&Check>) -> bool {
        self.active.as_ref().is_some_and(|editor| !check.is_some_and(|check| shown(check, editor.target())))
    }
}
