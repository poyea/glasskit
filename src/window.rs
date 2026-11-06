// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! Window classes, hidden windows, and windows that hold Rust state.

use std::rc::Rc;

use windows::Win32::Foundation::{ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GWLP_USERDATA, GetWindowLongPtrW, IsWindow, LoadCursorW,
    RegisterClassExW, SetWindowLongPtrW, WNDCLASSEXW, WNDPROC, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::PCWSTR;

use crate::error::{Call, Error, Result};

/// Register a window class. Windows keeps a class until the process ends, so a second
/// registration is not an error.
pub(crate) fn register_class(
    name: PCWSTR,
    proc: WNDPROC,
    hinstance: HINSTANCE,
    cursor: PCWSTR,
) -> Result<()> {
    unsafe {
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: proc,
            hInstance: hinstance,
            hCursor: LoadCursorW(None, cursor).call("LoadCursorW")?,
            lpszClassName: name,
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return Err(Error::last("RegisterClassExW"));
        }
    }
    Ok(())
}

/// Create a hidden top-level window. A message-only window gets fewer messages, such as
/// the session and taskbar notices.
pub(crate) fn create_hidden(class: PCWSTR, hinstance: HINSTANCE) -> Result<HWND> {
    unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class,
            class,
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinstance),
            None,
        )
        .call("CreateWindowExW")
    }
}

/// Destroy a window that is still open.
pub(crate) fn destroy(hwnd: HWND) {
    unsafe {
        if IsWindow(Some(hwnd)).as_bool() {
            let _ = DestroyWindow(hwnd);
        }
    }
}

/// The state that [`Window::attach`] gave to `hwnd`. It gives `None` if the window has no
/// state yet.
///
/// # Safety
///
/// `hwnd` must belong to this thread. If it has a state, the state must be a `T`. The
/// reference must not outlive the window.
pub(crate) unsafe fn state_of<'a, T>(hwnd: HWND) -> Option<&'a T> {
    unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const T).as_ref() }
}

/// A window with a shared state. The window procedure reads the state with [`state_of`].
///
/// The window holds one count of the state, so the state lives as long as the window.
/// Dropping the value destroys the window, then releases the count.
pub(crate) struct Window<T> {
    hwnd: HWND,
    state: Rc<T>,
}

impl<T> Window<T> {
    /// Make `state` the state of `hwnd`.
    pub(crate) fn attach(hwnd: HWND, state: Rc<T>) -> Self {
        let count = Rc::into_raw(Rc::clone(&state));
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, count as isize);
        }
        Self { hwnd, state }
    }

    pub(crate) fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub(crate) fn state(&self) -> &T {
        &self.state
    }

    /// Another handle to the state. It keeps the state alive after the window ends.
    pub(crate) fn share(&self) -> Rc<T> {
        Rc::clone(&self.state)
    }
}

impl<T> Drop for Window<T> {
    fn drop(&mut self) {
        destroy(self.hwnd);
        // The window is gone, so no message reads the state now. Release its count.
        unsafe {
            drop(Rc::from_raw(Rc::as_ptr(&self.state)));
        }
    }
}
