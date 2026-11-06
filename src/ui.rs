// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The setup that all `glasskit` windows of a thread share.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

use windows::System::DispatcherQueueController;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::WinRT::{
    CreateDispatcherQueueController, DQTAT_COM_STA, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
    RO_INIT_SINGLETHREADED, RoInitialize,
};
use windows::Win32::UI::Controls::{
    ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, IsDialogMessageW, MSG, PM_REMOVE, PeekMessageW, PostMessageW,
    PostQuitMessage, TranslateMessage, WM_NULL, WM_QUIT,
};

use crate::error::{Call, Error, Result};

thread_local! {
    static CURRENT: RefCell<Weak<Inner>> = const { RefCell::new(Weak::new()) };
}

/// What a window tells the event loop: a popup or a dialog has something for the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Notice {
    Popup(u32),
    Dialog(u32),
}

struct Inner {
    hinstance: HINSTANCE,
    _dispatcher: DispatcherQueueController,
    next_id: Cell<u32>,
    notices: RefCell<VecDeque<Notice>>,
    /// The dialogs that show now. The message loops give them their keyboard messages.
    dialogs: RefCell<Vec<HWND>>,
}

/// Proof that the current thread is set up for `glasskit` windows.
///
/// Create it once, on the thread that shows the windows, and give it to [`EventLoop::new`],
/// [`ChoicePopup::new`], and [`Dialog::new`]. Each of them keeps a copy, so the setup stays
/// while any window exists. `Ui` is not [`Send`]: windows must stay on the thread that made
/// them, and the compiler enforces it.
///
/// [`Ui::new`] does this setup for the process and the thread:
///
/// - It makes the process aware of the DPI of each monitor (per-monitor v2), unless the
///   application manifest or an earlier call has set the DPI awareness.
/// - It starts the Windows Runtime on the thread (single-threaded apartment).
/// - It loads the standard common controls.
/// - It creates the dispatcher queue that the popup animations need.
///
/// [`EventLoop::new`]: crate::events::EventLoop::new
/// [`ChoicePopup::new`]: crate::popup::ChoicePopup::new
/// [`Dialog::new`]: crate::dialog::Dialog::new
#[derive(Clone)]
pub struct Ui {
    inner: Rc<Inner>,
}

impl Ui {
    /// Set up the current thread, or get the setup that exists on it.
    pub fn new() -> Result<Self> {
        if let Some(inner) = CURRENT.with_borrow(Weak::upgrade) {
            return Ok(Self { inner });
        }
        unsafe {
            // Both calls fail if the manifest or the app already chose the same value.
            // That is harmless.
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let _ = RoInitialize(RO_INIT_SINGLETHREADED);
            let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX {
                dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_STANDARD_CLASSES,
            });
            let hinstance: HINSTANCE = GetModuleHandleW(None).call("GetModuleHandleW")?.into();
            let dispatcher = CreateDispatcherQueueController(DispatcherQueueOptions {
                dwSize: size_of::<DispatcherQueueOptions>() as u32,
                threadType: DQTYPE_THREAD_CURRENT,
                apartmentType: DQTAT_COM_STA,
            })
            .call("CreateDispatcherQueueController")?;
            let inner = Rc::new(Inner {
                hinstance,
                _dispatcher: dispatcher,
                next_id: Cell::new(1),
                notices: RefCell::new(VecDeque::new()),
                dialogs: RefCell::new(Vec::new()),
            });
            CURRENT.set(Rc::downgrade(&inner));
            Ok(Self { inner })
        }
    }

    pub(crate) fn hinstance(&self) -> HINSTANCE {
        self.inner.hinstance
    }

    pub(crate) fn new_id(&self) -> u32 {
        let id = self.inner.next_id.get();
        self.inner.next_id.set(id + 1);
        id
    }

    pub(crate) fn take_notice(&self) -> Option<Notice> {
        self.inner.notices.borrow_mut().pop_front()
    }

    pub(crate) fn add_dialog(&self, hwnd: HWND) {
        self.inner.dialogs.borrow_mut().push(hwnd);
    }

    pub(crate) fn remove_dialog(&self, hwnd: HWND) {
        self.inner.dialogs.borrow_mut().retain(|&h| h != hwnd);
    }

    /// Give a keyboard message to the dialog that it belongs to (Tab, Enter, Esc). It gives
    /// true if a dialog used the message.
    pub(crate) fn dialog_message(&self, msg: &MSG) -> bool {
        let dialogs = self.inner.dialogs.borrow().clone();
        dialogs
            .into_iter()
            .any(|hwnd| unsafe { IsDialogMessageW(hwnd, msg) }.as_bool())
    }

    fn dispatch(&self, msg: &MSG) {
        if !self.dialog_message(msg) {
            unsafe {
                let _ = TranslateMessage(msg);
                DispatchMessageW(msg);
            }
        }
    }

    /// Dispatch the messages that wait now. It gives true if one of them is `WM_QUIT`.
    pub(crate) fn pump_pending(&self) -> bool {
        let mut msg = MSG::default();
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    return true;
                }
                self.dispatch(&msg);
            }
        }
        false
    }

    /// Wait for one message and dispatch it. It gives false on `WM_QUIT`, after it posts
    /// `WM_QUIT` again for the caller.
    pub(crate) fn pump_wait(&self) -> Result<bool> {
        let mut msg = MSG::default();
        match unsafe { GetMessageW(&mut msg, None, 0, 0).0 } {
            0 => {
                unsafe { PostQuitMessage(msg.wParam.0 as i32) };
                Ok(false)
            }
            -1 => Err(Error::last("GetMessageW")),
            _ => {
                self.dispatch(&msg);
                Ok(true)
            }
        }
    }
}

/// Tell the event loop of this thread that a window has something for the app, and wake it.
/// A notice that waits already is not added again.
pub(crate) fn notify(notice: Notice) {
    if let Some(inner) = CURRENT.with_borrow(Weak::upgrade) {
        let mut notices = inner.notices.borrow_mut();
        if !notices.contains(&notice) {
            notices.push_back(notice);
        }
    }
    unsafe {
        let _ = PostMessageW(None, WM_NULL, WPARAM(0), LPARAM(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thread_shares_one_setup() {
        let a = Ui::new().unwrap();
        let b = Ui::new().unwrap();
        assert!(Rc::ptr_eq(&a.inner, &b.inner));
    }

    #[test]
    fn setup_ends_with_the_last_copy_and_can_start_again() {
        let first = Ui::new().unwrap();
        let weak = Rc::downgrade(&first.inner);
        drop(first);
        assert!(weak.upgrade().is_none());
        assert!(Ui::new().is_ok());
    }
}

#[cfg(test)]
mod notice_tests {
    use super::*;

    #[test]
    fn notices_come_once_in_order() {
        let ui = Ui::new().unwrap();
        notify(Notice::Popup(1));
        notify(Notice::Dialog(2));
        notify(Notice::Popup(1));
        assert_eq!(ui.take_notice(), Some(Notice::Popup(1)));
        assert_eq!(ui.take_notice(), Some(Notice::Dialog(2)));
        assert_eq!(ui.take_notice(), None);
    }

    #[test]
    fn ids_are_new_each_time() {
        let ui = Ui::new().unwrap();
        assert_ne!(ui.new_id(), ui.new_id());
    }
}
