// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The setup that all `glasskit` windows of a thread share.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use windows::System::DispatcherQueueController;
use windows::Win32::Foundation::HINSTANCE;
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

use crate::error::{Call, Result};

thread_local! {
    static CURRENT: RefCell<Weak<Inner>> = const { RefCell::new(Weak::new()) };
}

struct Inner {
    hinstance: HINSTANCE,
    _dispatcher: DispatcherQueueController,
}

/// Proof that the current thread is set up for `glasskit` windows.
///
/// Create it once, on the thread that shows the windows, and give it to [`Tray::new`],
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
/// [`Tray::new`]: crate::tray::Tray::new
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
            // Both fail harmlessly if the manifest or the app made the same choice before.
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
            });
            CURRENT.set(Rc::downgrade(&inner));
            Ok(Self { inner })
        }
    }

    pub(crate) fn hinstance(&self) -> HINSTANCE {
        self.inner.hinstance
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
