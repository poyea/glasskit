// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

use std::time::{Duration, Instant};

pub(super) struct Countdown {
    pub(super) left: Duration,
    pub(super) last_tick: Instant,
    pub(super) shown: u64,
}

impl Countdown {
    pub(super) fn new(after: Duration) -> Self {
        Self {
            left: after,
            last_tick: Instant::now(),
            shown: whole_seconds(after),
        }
    }
}

pub(super) fn whole_seconds(d: Duration) -> u64 {
    d.as_millis().div_ceil(1000) as u64
}

pub(super) fn label(dismiss: &str, seconds: u64) -> String {
    format!("{dismiss} ({seconds})")
}
