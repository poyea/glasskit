// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

use std::time::{Duration, Instant};

/// The countdown of the dismiss button. It runs from `resume` until `pause`, and it keeps
/// its time left while it does not run.
pub(super) struct Countdown {
    left: Duration,
    ends: Option<Instant>,
    /// The whole seconds that the button shows.
    pub(super) shown: u64,
}

impl Countdown {
    pub(super) fn new(after: Duration) -> Self {
        Self {
            left: after,
            ends: None,
            shown: whole_seconds(after),
        }
    }

    pub(super) fn is_running(&self) -> bool {
        self.ends.is_some()
    }

    pub(super) fn remaining(&self, now: Instant) -> Duration {
        match self.ends {
            Some(ends) => ends.saturating_duration_since(now),
            None => self.left,
        }
    }

    pub(super) fn pause(&mut self, now: Instant) {
        if let Some(ends) = self.ends.take() {
            self.left = ends.saturating_duration_since(now);
        }
    }

    pub(super) fn resume(&mut self, now: Instant) {
        if self.ends.is_none() {
            self.ends = Some(now + self.left);
        }
    }
}

pub(super) fn whole_seconds(d: Duration) -> u64 {
    d.as_millis().div_ceil(1000) as u64
}

/// The time until the whole seconds of `left` change. The change happens at each whole
/// second, so a timer for this time wakes the popup only when the label changes.
pub(super) fn until_next_second(left: Duration) -> Duration {
    let ms = left.as_millis() as u64;
    let boundary = ms.saturating_sub(1) / 1000 * 1000;
    Duration::from_millis(ms - boundary)
}

pub(super) fn label(dismiss: &str, seconds: u64) -> String {
    format!("{dismiss} ({seconds})")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_paused_countdown_keeps_its_time() {
        let t0 = Instant::now();
        let mut c = Countdown::new(ms(30_000));
        c.resume(t0);
        c.pause(t0 + ms(10_000));
        assert_eq!(c.remaining(t0 + ms(20_000)), ms(20_000));
        c.resume(t0 + ms(20_000));
        assert_eq!(c.remaining(t0 + ms(25_000)), ms(15_000));
    }

    #[test]
    fn remaining_is_zero_after_the_end() {
        let t0 = Instant::now();
        let mut c = Countdown::new(ms(1_000));
        c.resume(t0);
        assert!(c.remaining(t0 + ms(2_000)).is_zero());
    }

    #[test]
    fn whole_seconds_round_up() {
        assert_eq!(whole_seconds(ms(3_000)), 3);
        assert_eq!(whole_seconds(ms(3_001)), 4);
        assert_eq!(whole_seconds(ms(0)), 0);
    }

    #[test]
    fn timer_waits_for_the_next_whole_second() {
        assert_eq!(until_next_second(ms(3_500)), ms(500));
        assert_eq!(until_next_second(ms(3_000)), ms(1_000));
        assert_eq!(until_next_second(ms(1)), ms(1));
    }
}
