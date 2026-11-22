//! The slider of a searched mod: a track that stands for the range the mod was rolled in, on
//! which the minimum of its filter is set by a press or a drag, the high end by a double click.
//!
//! `Scale` is the range and what a place on the track is worth. `Drag` is a slider being held:
//! where its track is, the value under the pointer, and the `Action` its release is. Nothing is
//! changed in the check before the release. Nothing here knows of a window, of the pointer's
//! events or of how a track is drawn: see `panel`.

use crate::check::{Action, Check, Mark, Row};

/// The range of a span this wide or wider is gone through in whole numbers; a narrower one in
/// tenths, when its ends or the item's value have decimals, and in whole numbers when they have none.
const WHOLE_FROM: f64 = 10.0;

/// The range a slider goes through, from its low end to its high end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    low: f64,
    high: f64,
    /// Its ends or the value of the mod have decimals.
    fine: bool,
}

impl Scale {
    /// The two ends as written at the ends of the track: whole numbers unless the mod has decimals.
    pub fn bounds_text(&self) -> (String, String) {
        let write = |value: f64| crate::field::rounded(value, if self.fine { 2 } else { 0 });
        (write(self.low), write(self.high))
    }

    /// The scale of a range. `None` without a range, and for one whose ends are the same or are
    /// no numbers: there is nothing to slide through.
    pub fn of(range: Option<(f64, f64)>) -> Option<Self> {
        let (a, b) = range?;
        let (low, high) = (a.min(b), a.max(b));
        (low.is_finite() && high.is_finite() && low < high).then_some(Self { low, high, fine: crate::check::has_decimals(Some((low, high)), None) })
    }

    /// The same scale, in tenths too when the value the item has one has decimals.
    pub fn with_value(self, value: Option<f64>) -> Self {
        Self { fine: self.fine || crate::check::has_decimals(None, value), ..self }
    }

    pub fn low(&self) -> f64 {
        self.low
    }

    pub fn high(&self) -> f64 {
        self.high
    }

    /// What the values between the two ends are multiples of.
    fn step(&self) -> f64 {
        if !self.fine || self.high - self.low >= WHOLE_FROM { 1.0 } else { 0.1 }
    }

    /// The value at `x` on a track that goes from `left` to `right`: the low end at `left` and
    /// before it, the high end at `right` and after it, and between them the nearest whole
    /// number, or the nearest tenth over a narrow range, never outside the range.
    pub fn value_at(&self, x: f32, left: f32, right: f32) -> f64 {
        let share = if right > left { f64::from(((x - left) / (right - left)).clamp(0.0, 1.0)) } else { 0.0 };
        if share <= 0.0 {
            return self.low;
        }
        if share >= 1.0 {
            return self.high;
        }
        let step = self.step();
        let at = self.low + share * (self.high - self.low);
        // Counted in steps, so that a tenth is written as one: 0.3 and not 0.30000000000000004.
        let snapped = if step < 1.0 { (at / step).round() / (1.0 / step) } else { at.round() };
        // Not "-0".
        snapped.clamp(self.low, self.high) + 0.0
    }

    /// How far along the track a minimum sits, from 0 at the low end to 1 at the high end. A
    /// minimum under the range is at the low end, one above it at the high end; no minimum, and
    /// one that is no number, at the low end.
    pub fn share(&self, min: Option<f64>) -> f32 {
        match min {
            Some(min) if min.is_finite() => (((min - self.low) / (self.high - self.low)) as f32).clamp(0.0, 1.0),
            _ => 0.0,
        }
    }

    /// Where the handle is for a minimum, on a track that goes from `left` to `right`.
    pub fn x_of(&self, min: Option<f64>, left: f32, right: f32) -> f32 {
        left + self.share(min) * (right - left)
    }
}

/// The scale of the slider a row shows, if it shows one: the row is a mod that is searched, the
/// range it was rolled in is known and has two ends, and the row is `engaged`: the pointer is
/// over it, one of its fields is being typed in, or its slider is being held. A property has no
/// slider.
pub fn scale_of(check: &Check, row: &Row, engaged: bool) -> Option<Scale> {
    let searched = row.kind.is_some() && check.mark(row) == Mark::Searched && check.leads(row);
    Scale::of(row.range).map(|scale| scale.with_value(row.value)).filter(|_| searched && engaged)
}

/// A slider being held: the button went down on its track and is not released yet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drag {
    /// The filter whose minimum it sets.
    filter: usize,
    scale: Scale,
    /// Where its track starts and ends, as it was when the button went down.
    left: f32,
    right: f32,
    /// The value under the pointer.
    value: f64,
}

impl Drag {
    /// The button went down at `x` on the track of `filter`, which goes from `left` to `right`:
    /// the value there is held at once.
    pub fn press(filter: usize, scale: Scale, (left, right): (f32, f32), x: f32) -> Self {
        Self { filter, scale, left, right, value: scale.value_at(x, left, right) }
    }

    /// A slider held at `value`, wherever its track is: for a panel that is posed, not handled.
    pub fn held(filter: usize, scale: Scale, value: f64) -> Self {
        Self { filter, scale, left: 0.0, right: 0.0, value: value.clamp(scale.low, scale.high) }
    }

    pub fn filter(&self) -> usize {
        self.filter
    }

    /// The value the minimum would be given if the button were released now.
    pub fn value(&self) -> f64 {
        self.value
    }

    /// The pointer is at `x` now, wherever it is above or under the track.
    pub fn follow(self, x: f32) -> Self {
        Self { value: self.scale.value_at(x, self.left, self.right), ..self }
    }

    /// What the release is: the minimum set to the value held, or to the high end of the range
    /// when the release ends a double click.
    pub fn release(self, double: bool) -> Action {
        Action::SetMin(self.filter, Some(if double { self.scale.high } else { self.value }))
    }
}
