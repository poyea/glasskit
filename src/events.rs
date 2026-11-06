// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The main loop of an app: one loop for all trays, popups, and dialogs of a thread.
//!
//! [`EventLoop::next`] handles the window messages of the thread, and returns when there is
//! something for the app: a menu command, the end of a popup, a change in a dialog, a deadline,
//! an unlock of the session, or a quit. Popups and dialogs that show do not stop the loop, so
//! a dialog can stay open while popups come and go.

use std::cell::Cell;
use std::rc::{Rc, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
};
use windows::Win32::System::Threading::{
    CancelWaitableTimer, CreateWaitableTimerExW, INFINITE, SetWaitableTimer, TIMER_ALL_ACCESS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DispatchMessageW, IDC_ARROW, MSG, MWMO_INPUTAVAILABLE,
    MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW, QS_ALLINPUT, TranslateMessage, WM_QUIT,
    WM_WTSSESSION_CHANGE, WTS_SESSION_UNLOCK,
};
use windows::core::{PCWSTR, w};

use crate::Ui;
use crate::error::{Call, Result};
use crate::ui::Notice;
use crate::window::{self, Window};

const CLASS_NAME: PCWSTR = w!("GlasskitEvents");
/// Ticks of 100 ns from 1601-01-01 (FILETIME) to 1970-01-01 (Unix).
const UNIX_EPOCH_AS_FILETIME: i64 = 116_444_736_000_000_000;

/// The id of a [`ChoicePopup`](crate::popup::ChoicePopup), as [`Event::Popup`] gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PopupId(pub(crate) u32);

/// The id of a [`Dialog`](crate::dialog::Dialog), as [`Event::Dialog`] gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DialogId(pub(crate) u32);

/// Something for the app. `C` is the command type of the trays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event<C> {
    /// The user picked a command item of a tray menu.
    Command(C),
    /// A popup has ended: call [`ChoicePopup::take_reply`](crate::popup::ChoicePopup::take_reply)
    /// on the popup with this id.
    Popup(PopupId),
    /// A dialog has events: call [`Dialog::next_event`](crate::dialog::Dialog::next_event) on
    /// the dialog with this id until it gives `None`.
    Dialog(DialogId),
    /// The time given to [`EventLoop::next`] came.
    Deadline,
    /// The user unlocked the session: someone is at the screen again.
    SessionUnlocked,
    /// The user picked the quit item of a tray menu, or the thread got `WM_QUIT`.
    Quit,
}

/// Something that gives commands to the loop, such as a tray.
pub(crate) trait Source<C> {
    fn take_command(&self) -> Option<C>;
}

/// The main loop of the thread. `C` is the command type of the trays.
///
/// Make it with a [`Ui`], then give it to [`Tray::new`](crate::tray::Tray::new).
pub struct EventLoop<C> {
    ui: Ui,
    timer: DeadlineTimer,
    window: Window<Cell<bool>>,
    sources: Vec<Weak<dyn Source<C>>>,
}

impl<C> EventLoop<C> {
    /// A loop for the windows of this thread.
    pub fn new(ui: &Ui) -> Result<Self> {
        let timer = DeadlineTimer::new()?;
        let hinstance = ui.hinstance();
        window::register_class(CLASS_NAME, Some(wndproc), hinstance, IDC_ARROW)?;
        let hwnd = window::create_hidden(CLASS_NAME, hinstance)?;
        let window = Window::attach(hwnd, Rc::new(Cell::new(false)));
        unsafe {
            let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
        }
        Ok(Self {
            ui: ui.clone(),
            timer,
            window,
            sources: Vec::new(),
        })
    }

    /// The setup of the thread that the loop serves.
    pub fn ui(&self) -> &Ui {
        &self.ui
    }

    pub(crate) fn add_source(&mut self, source: Weak<dyn Source<C>>) {
        self.sources.push(source);
    }

    /// Handle the window messages until there is something for the app. With `until`, it
    /// also returns [`Event::Deadline`] at that clock time. The deadline stays exact after
    /// sleep or a clock change. While it waits, the thread uses no CPU.
    pub fn next(&mut self, until: Option<SystemTime>) -> Result<Event<C>> {
        if until.is_none() {
            self.timer.cancel();
        }
        loop {
            unsafe {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    if msg.message == WM_QUIT {
                        return Ok(Event::Quit);
                    }
                    if self.ui.dialog_message(&msg) {
                        continue;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            self.sources.retain(|s| s.strong_count() > 0);
            for source in &self.sources {
                if let Some(command) = source.upgrade().and_then(|s| s.take_command()) {
                    return Ok(Event::Command(command));
                }
            }
            match self.ui.take_notice() {
                Some(Notice::Popup(id)) => return Ok(Event::Popup(PopupId(id))),
                Some(Notice::Dialog(id)) => return Ok(Event::Dialog(DialogId(id))),
                None => {}
            }
            if self.window.state().replace(false) {
                return Ok(Event::SessionUnlocked);
            }
            if let Some(t) = until {
                if SystemTime::now() >= t {
                    return Ok(Event::Deadline);
                }
                self.timer.set(t)?;
            }
            unsafe {
                MsgWaitForMultipleObjectsEx(
                    Some(&[self.timer.0]),
                    INFINITE,
                    QS_ALLINPUT,
                    MWMO_INPUTAVAILABLE,
                );
            }
        }
    }
}

impl<C> Drop for EventLoop<C> {
    fn drop(&mut self) {
        unsafe {
            let _ = WTSUnRegisterSessionNotification(self.window.hwnd());
        }
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if let (WM_WTSSESSION_CHANGE, Some(unlocked)) = (msg, window::state_of::<Cell<bool>>(hwnd))
        {
            if wparam.0 == WTS_SESSION_UNLOCK as usize {
                unlocked.set(true);
            }
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

/// For `SetWaitableTimer`, a positive due time is absolute and a negative one is relative.
/// The value must stay positive. It saturates at the maximum and does not wrap.
fn absolute_due_time(at: SystemTime) -> i64 {
    let since_unix = at.duration_since(UNIX_EPOCH).unwrap_or_default();
    i64::try_from(since_unix.as_nanos().div_ceil(100))
        .unwrap_or(i64::MAX)
        .saturating_add(UNIX_EPOCH_AS_FILETIME)
}

/// An absolute timer fires at the right clock time after sleep or a clock change. The wait
/// need not wake up to check the clock.
struct DeadlineTimer(HANDLE);

impl DeadlineTimer {
    fn new() -> Result<Self> {
        let handle = unsafe {
            CreateWaitableTimerExW(None, PCWSTR::null(), 0, TIMER_ALL_ACCESS.0)
                .call("CreateWaitableTimerExW")?
        };
        Ok(Self(handle))
    }

    fn set(&self, at: SystemTime) -> Result<()> {
        let due = absolute_due_time(at);
        unsafe { SetWaitableTimer(self.0, &due, 0, None, None, false).call("SetWaitableTimer") }
    }

    fn cancel(&self) {
        unsafe {
            let _ = CancelWaitableTimer(self.0);
        }
    }
}

impl Drop for DeadlineTimer {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn due_time_counts_from_1601() {
        assert_eq!(absolute_due_time(UNIX_EPOCH), UNIX_EPOCH_AS_FILETIME);
        let one_second = UNIX_EPOCH + Duration::from_secs(1);
        assert_eq!(
            absolute_due_time(one_second),
            UNIX_EPOCH_AS_FILETIME + 10_000_000
        );
    }

    #[test]
    fn due_time_stays_positive_far_in_the_future() {
        const YEAR: u64 = 365 * 24 * 60 * 60;
        let far = UNIX_EPOCH
            .checked_add(Duration::from_secs(20_000 * YEAR))
            .expect("SystemTime holds 20000 years");
        assert!(absolute_due_time(far) > UNIX_EPOCH_AS_FILETIME);
    }

    #[test]
    fn due_time_before_1970_is_1970() {
        let before = UNIX_EPOCH - Duration::from_secs(60);
        assert_eq!(absolute_due_time(before), UNIX_EPOCH_AS_FILETIME);
    }

    #[test]
    fn a_deadline_in_the_past_returns_at_once() {
        let ui = Ui::new().unwrap();
        let mut events: EventLoop<()> = EventLoop::new(&ui).unwrap();
        let past = SystemTime::now() - Duration::from_secs(1);
        assert_eq!(events.next(Some(past)).unwrap(), Event::Deadline);
    }

    #[test]
    fn a_notice_wakes_the_loop() {
        let ui = Ui::new().unwrap();
        let mut events: EventLoop<()> = EventLoop::new(&ui).unwrap();
        crate::ui::notify(Notice::Popup(7));
        assert_eq!(events.next(None).unwrap(), Event::Popup(PopupId(7)));
    }
}
