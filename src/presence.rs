// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! Whether the user can see a popup now.

use windows::Win32::UI::Shell::{
    QUNS_APP, QUNS_BUSY, QUNS_NOT_PRESENT, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
    SHQueryUserNotificationState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Why the user cannot see a popup now.
pub enum Busy {
    /// The screen is locked, the screen saver shows, or another user session is active.
    Away,
    /// A full-screen app or game is in front.
    FullScreen,
    /// Windows is in presentation mode.
    Presenting,
}

/// The reason not to show a popup now, as Windows gives it for its own notifications.
/// Focus assist (do not disturb) is not included: Windows has no public API for it.
pub fn busy() -> Option<Busy> {
    let state = unsafe { SHQueryUserNotificationState() }.ok()?;
    match state {
        QUNS_NOT_PRESENT => Some(Busy::Away),
        QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_APP => Some(Busy::FullScreen),
        QUNS_PRESENTATION_MODE => Some(Busy::Presenting),
        _ => None,
    }
}
