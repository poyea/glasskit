// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

#![warn(missing_docs)]

//! Native Windows UI with a glass look: a tray icon and menu, a frosted-glass popup, themed
//! dialogs, and helpers for tray apps.
//!
//! - [`events`]: [`EventLoop`], the main loop of the app. It sleeps until a menu command, the
//!   end of a popup, a change in a dialog, a deadline, or an unlock of the session.
//! - [`tray`]: an icon in the notification area with a menu.
//! - [`popup`]: a frosted-glass popup with a question and choice buttons. It fades in without
//!   taking the keyboard focus, can show whether a choice was correct, and can dismiss itself.
//! - [`dialog`]: a small modal window made of standard controls, in the same colors.
//! - [`layout`]: rows and columns that place the controls of a dialog.
//! - [`placement`]: where a window shows, as fractions of the free space of the screen.
//! - [`autostart`], [`presence`], [`process`], [`single_instance`], and [`show_error`]: small
//!   helpers that tray apps often need.
//!
//! All windows follow the Windows light or dark app mode and the display scale.
//!
//! # Threads
//!
//! Make one [`Ui`] on the thread that shows the windows, and make all windows on that thread.
//! [`Ui`] is not `Send`, so the compiler stops a window from going to another thread. The
//! thread must run [`EventLoop::next`], which handles the window messages of all windows of
//! the thread. An app without a tray can use [`ChoicePopup::ask`] and [`Dialog::run`]
//! instead: each of them has a message loop of its own and returns when its window ends.
//!
//! # Example
//!
//! A tray app that asks a question each minute and on "Ask now". The same program is in
//! `examples/quiz.rs`.
//!
//! ```no_run
//! use std::time::{Duration, SystemTime};
//!
//! use glasskit::Ui;
//! use glasskit::events::{Event, EventLoop};
//! use glasskit::placement::Placement;
//! use glasskit::popup::{ChoicePopup, Mark, Prompt};
//! use glasskit::tray::{Icon, MenuItem, Tray};
//!
//! #[derive(Debug, Clone, PartialEq, Eq)]
//! enum Command {
//!     AskNow,
//! }
//!
//! fn main() -> glasskit::Result<()> {
//!     let ui = Ui::new()?;
//!     let mut events = EventLoop::new(&ui)?;
//!     let size = Icon::tray_size();
//!     let icon = Icon::from_argb(size, vec![0xFF2E_86DE; (size * size) as usize])?;
//!     let mut tray = Tray::new(&mut events, "Quiz", &icon)?;
//!     tray.set_menu(&[
//!         MenuItem::command(Command::AskNow, "Ask now"),
//!         MenuItem::Separator,
//!         MenuItem::Quit("Quit".into()),
//!     ]);
//!     let mut popup = ChoicePopup::new(&ui, Placement::default())?;
//!     let mut next = SystemTime::now() + Duration::from_secs(60);
//!     loop {
//!         match events.next(Some(next))? {
//!             Event::Quit => return Ok(()),
//!             Event::Deadline | Event::Command(Command::AskNow) => {
//!                 next = SystemTime::now() + Duration::from_secs(60);
//!                 if !popup.is_showing() {
//!                     let prompt = Prompt::new("What is 2 + 2?", ["3", "4", "5"])
//!                         .marks(|clicked| answer_marks(clicked, 1))
//!                         .auto_dismiss(Some(Duration::from_secs(30)));
//!                     popup.show(&prompt)?;
//!                 }
//!             }
//!             Event::Popup(id) if id == popup.id() => {
//!                 if let Some(reply) = popup.take_reply() {
//!                     println!("{:?}", reply.response);
//!                 }
//!             }
//!             _ => {}
//!         }
//!     }
//! }
//!
//! /// Mark the correct answer good and a wrong click bad.
//! fn answer_marks(clicked: usize, correct: usize) -> Vec<(usize, Mark)> {
//!     let mut marks = vec![(correct, Mark::Good)];
//!     if clicked != correct {
//!         marks.push((clicked, Mark::Bad));
//!     }
//!     marks
//! }
//! ```
//!
//! [`EventLoop`]: events::EventLoop
//! [`EventLoop::next`]: events::EventLoop::next
//! [`ChoicePopup::ask`]: popup::ChoicePopup::ask
//! [`Dialog::run`]: dialog::Dialog::run

#[cfg(not(windows))]
compile_error!("glasskit works only on Windows: it uses the Win32 and Windows Runtime APIs.");

pub mod autostart;
pub mod dialog;
pub mod error;
pub mod events;
pub mod layout;
pub mod placement;
pub mod popup;
pub mod presence;
pub mod process;
mod theme;
pub mod tray;
mod ui;

/// Compiles the example of the README as a doc test.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeExample;

pub use error::{Error, Result};
pub use ui::Ui;

use crate::error::Call;
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_CLASS_ALREADY_EXISTS, GetLastError, HANDLE, POINT,
};
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForSystem, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    MB_ICONERROR, MB_OK, MessageBoxW, RegisterClassExW, WNDCLASSEXW,
};
use windows::core::HSTRING;

/// Register a window class. A class stays registered until the process ends, so a second
/// registration of the same class is not an error.
pub(crate) fn register_class(class: &WNDCLASSEXW) -> Result<()> {
    unsafe {
        if RegisterClassExW(class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return Err(Error::last("RegisterClassExW"));
        }
    }
    Ok(())
}

/// The system DPI stays at its sign-in value. Read the DPI of the monitor each time a window
/// opens, so a change of the display scale applies to the next window.
pub(crate) fn primary_monitor_dpi() -> u32 {
    unsafe {
        let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let (mut dpi_x, mut dpi_y) = (0u32, 0u32);
        match GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) {
            Ok(()) if dpi_x > 0 => dpi_x,
            _ => GetDpiForSystem(),
        }
    }
}

/// Show an error in a message box, and wait until the user closes it.
/// It needs no [`Ui`], so it also works before the setup or after a failed setup.
pub fn show_error(title: &str, message: &str) {
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(message),
            &HSTRING::from(title),
            MB_OK | MB_ICONERROR,
        );
    }
}

/// Proof that this process is the only one with a name in the user session.
/// The name is free again when the guard is dropped or the process ends.
pub struct InstanceGuard(HANDLE);

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Take `name` for this process in the user session. It gives `None` if another process
/// has the name, for example a second start of the same app.
pub fn single_instance(name: &str) -> Result<Option<InstanceGuard>> {
    unsafe {
        let handle = CreateMutexW(None, true, &HSTRING::from(format!("Local\\{name}")))
            .call("CreateMutexW")?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(handle);
            return Ok(None);
        }
        Ok(Some(InstanceGuard(handle)))
    }
}
