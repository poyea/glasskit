// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! Window classes, hidden windows, windows that hold Rust state, and small helpers for
//! window messages that all windows use.

use std::ffi::c_void;
use std::rc::Rc;

use windows::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM,
};
use windows::Win32::Graphics::Dwm::{DWMWINDOWATTRIBUTE, DwmSetWindowAttribute};
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GWLP_USERDATA, GetWindowLongPtrW, IsIconic, IsWindow,
    LoadCursorW, RegisterClassExW, SW_HIDE, SW_RESTORE, SW_SHOW, SW_SHOWNORMAL,
    SetForegroundWindow, SetWindowLongPtrW, ShowWindow, WNDCLASSEXW, WNDPROC, WS_EX_TOOLWINDOW,
    WS_POPUP,
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

/// Set a DWM attribute that takes a 32-bit value. It gives false when Windows does not take
/// it, for example an attribute that is newer than the Windows version.
pub(crate) fn set_dwm_attribute(hwnd: HWND, attribute: DWMWINDOWATTRIBUTE, value: u32) -> bool {
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attribute,
            &value as *const u32 as *const c_void,
            size_of::<u32>() as u32,
        )
        .is_ok()
    }
}

/// Show a top-level window and bring it to the front.
pub(crate) fn show_window(hwnd: HWND) {
    unsafe {
        // The first ShowWindow of a process can use the show state of the shortcut or the
        // parent process, such as hidden. The second call uses the state that is wanted.
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Show a window that the app hid, in the state that it had, and bring it to the front.
pub(crate) fn show_again(hwnd: HWND) {
    unsafe {
        let how = if IsIconic(hwnd).as_bool() {
            SW_RESTORE
        } else {
            SW_SHOW
        };
        let _ = ShowWindow(hwnd, how);
        let _ = SetForegroundWindow(hwnd);
    }
}

pub(crate) fn hide_window(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

/// The point of a mouse message, in client pixels.
pub(crate) fn point(lparam: LPARAM) -> (i32, i32) {
    let x = (lparam.0 & 0xFFFF) as i16 as i32;
    let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
    (x, y)
}

/// Ask for `WM_MOUSELEAVE` when the pointer leaves the window. It gives false on failure.
pub(crate) fn track_leave(hwnd: HWND) -> bool {
    let mut tme = TRACKMOUSEEVENT {
        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: hwnd,
        dwHoverTime: 0,
    };
    unsafe { TrackMouseEvent(&mut tme).is_ok() }
}

/// Destroys a window when a step of its creation fails. Call `disarm` when the window is
/// complete.
pub(crate) struct DestroyOnError(Option<HWND>);

impl DestroyOnError {
    pub(crate) fn new(hwnd: HWND) -> Self {
        Self(Some(hwnd))
    }

    pub(crate) fn disarm(mut self) {
        self.0 = None;
    }
}

impl Drop for DestroyOnError {
    fn drop(&mut self) {
        if let Some(hwnd) = self.0 {
            destroy(hwnd);
        }
    }
}

/// The state that [`NativeWindow::attach`] gave to `hwnd`. It gives `None` if the window has no
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
pub(crate) struct NativeWindow<T> {
    hwnd: HWND,
    state: Rc<T>,
}

impl<T> NativeWindow<T> {
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

impl<T> Drop for NativeWindow<T> {
    fn drop(&mut self) {
        destroy(self.hwnd);
        // The window is gone, so no message reads the state now. Release its count.
        unsafe {
            drop(Rc::from_raw(Rc::as_ptr(&self.state)));
        }
    }
}
