//! Rate limiter driven by the `X-Rate-Limit-*` response headers.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Pace assumed until the site has told us its rules: one request per interval.
const FALLBACK_INTERVAL: Duration = Duration::from_secs(5);

/// Upper bound on remembered send times; real limits are far below it.
const LOG_CAP: usize = 1024;

/// One rule of a rate-limit policy together with its state when last reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// Requests allowed per window.
    pub max: u32,
    /// Length of the window, in seconds.
    pub window_s: u32,
    /// Length of the ban for breaking the rule, in seconds.
    pub penalty_s: u32,
    /// Requests the site counted in the current window.
    pub used: u32,
    /// Seconds of ban left; zero when not banned.
    pub banned_s: u32,
}

/// Pairs a limit header (`max:window:penalty,...`) with its state header
/// (`used:window:banned,...`) by position. Entries that cannot be read on
/// either side are skipped.
pub fn parse_rules(limit: &str, state: &str) -> Vec<Rule> {
    limit
        .split(',')
        .zip(state.split(','))
        .filter_map(|(limit, state)| {
            let [max, window_s, penalty_s] = triple(limit)?;
            let [used, _, banned_s] = triple(state)?;
            Some(Rule {
                max,
                window_s,
                penalty_s,
                used,
                banned_s,
            })
        })
        .collect()
}

fn triple(text: &str) -> Option<[u32; 3]> {
    let mut parts = text.split(':').map(|part| part.trim().parse::<u32>().ok());
    let triple = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(triple)
}

/// Decides whether a request may be sent now, from the rules the site last
/// reported and the times of our own requests.
#[derive(Debug, Default)]
pub struct Limiter {
    rules: Vec<Rule>,
    /// When `rules` were reported; `None` until the first report.
    updated: Option<Instant>,
    /// Send times of our own requests, oldest first.
    sent: VecDeque<Instant>,
}

impl Limiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes the rules read from a response received at `now`. A response
    /// without rules tells nothing and leaves the known ones in place.
    pub fn update(&mut self, rules: Vec<Rule>, now: Instant) {
        if rules.is_empty() {
            return;
        }
        self.rules = rules;
        self.updated = Some(now);
    }

    /// Records a request sent at `now`.
    pub fn note_request(&mut self, now: Instant) {
        self.sent.push_back(now);
        let horizon = self
            .rules
            .iter()
            .map(window)
            .max()
            .unwrap_or(FALLBACK_INTERVAL)
            .max(FALLBACK_INTERVAL);
        while let Some(&oldest) = self.sent.front() {
            if oldest + horizon <= now || self.sent.len() > LOG_CAP {
                self.sent.pop_front();
            } else {
                break;
            }
        }
    }

    /// How long to wait before the next request; zero when it may go now.
    ///
    /// One request of margin is kept on every rule: with `max` 5, the fifth
    /// request of a window is never sent.
    pub fn wait(&self, now: Instant) -> Duration {
        let Some(updated) = self.updated else {
            return self.sent.back().map_or(Duration::ZERO, |&last| {
                (last + FALLBACK_INTERVAL).saturating_duration_since(now)
            });
        };
        self.rules
            .iter()
            .map(|rule| {
                let ban = Duration::from_secs(rule.banned_s.into());
                let ban_left = (updated + ban).saturating_duration_since(now);
                ban_left.max(self.window_wait(rule, updated, now))
            })
            .max()
            .unwrap_or(Duration::ZERO)
    }

    /// How many requests may be sent from `now` on, one right after the other, before a rule
    /// asks to wait: what the tightest rule leaves, with the margin of [`Self::wait`]. Until
    /// the site has told its rules, one request per interval.
    pub fn left(&self, now: Instant) -> u32 {
        if !self.wait(now).is_zero() {
            return 0;
        }
        let Some(updated) = self.updated else {
            return 1;
        };
        let left = self
            .rules
            .iter()
            .map(|rule| allowed(rule).saturating_sub(self.counted(rule, updated, now).len()))
            .min()
            .unwrap_or(1);
        u32::try_from(left).unwrap_or(u32::MAX)
    }

    /// When each request that `rule` still counts at `now` was sent, oldest first.
    fn counted(&self, rule: &Rule, updated: Instant, now: Instant) -> Vec<Instant> {
        let window = window(rule);
        let live = |at: Instant| at + window > now;

        // The requests the site counted are matched to our own most recent
        // ones inside the window at the time of the report. Those we did not
        // send ourselves (another program, an earlier run) are taken to be as
        // recent as the report, which is the cautious reading.
        let ours: Vec<Instant> = self
            .sent
            .iter()
            .copied()
            .filter(|&at| at <= updated && at + window > updated)
            .collect();
        let used = rule.used as usize;
        let matched = ours.len().min(used);
        let reported = ours[ours.len() - matched..].iter().copied();
        let unmatched = std::iter::repeat_n(updated, used - matched);
        let later = self.sent.iter().copied().filter(|&at| at > updated);
        reported
            .chain(unmatched)
            .chain(later)
            .filter(|&at| live(at))
            .collect()
    }

    /// Time until one more request fits under `rule` with the margin.
    fn window_wait(&self, rule: &Rule, updated: Instant, now: Instant) -> Duration {
        let counted = self.counted(rule, updated, now);
        let allowed = allowed(rule);
        if counted.len() < allowed {
            return Duration::ZERO;
        }
        // A request fits once the one at this position has left the window.
        let leaves = counted[counted.len() - allowed];
        (leaves + window(rule)).saturating_duration_since(now)
    }
}

/// The requests a rule lets through per window once one is kept as a margin. With `max`
/// below 2 no margin is possible; one request per window.
fn allowed(rule: &Rule) -> usize {
    rule.max.saturating_sub(1).max(1) as usize
}

fn window(rule: &Rule) -> Duration {
    Duration::from_secs(rule.window_s.into())
}
