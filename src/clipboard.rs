// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! Text on the clipboard, for copy and paste in input boxes.

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// The text on the clipboard, if there is text.
pub(crate) fn text(hwnd: HWND) -> Option<String> {
    unsafe {
        OpenClipboard(Some(hwnd)).ok()?;
        let text = read();
        let _ = CloseClipboard();
        text
    }
}

/// Put text on the clipboard. It gives false if the clipboard is not available.
pub(crate) fn set_text(hwnd: HWND, text: &str) -> bool {
    unsafe {
        if OpenClipboard(Some(hwnd)).is_err() {
            return false;
        }
        let done = EmptyClipboard().is_ok() && write(text);
        let _ = CloseClipboard();
        done
    }
}

/// Read the text of the clipboard. The clipboard must be open.
unsafe fn read() -> Option<String> {
    unsafe {
        let handle = GetClipboardData(u32::from(CF_UNICODETEXT.0)).ok()?;
        let global = HGLOBAL(handle.0);
        let start = GlobalLock(global) as *const u16;
        if start.is_null() {
            return None;
        }
        // The text ends at a zero, or at the end of the memory block.
        let max = GlobalSize(global) / 2;
        let mut len = 0;
        while len < max && *start.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(start, len));
        let _ = GlobalUnlock(global);
        Some(text)
    }
}

/// Write text to the clipboard. The clipboard must be open and empty.
unsafe fn write(text: &str) -> bool {
    let mut wide: Vec<u16> = text.replace('\n', "\r\n").encode_utf16().collect();
    wide.push(0);
    unsafe {
        let Ok(global) = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) else {
            return false;
        };
        let target = GlobalLock(global) as *mut u16;
        if target.is_null() {
            let _ = GlobalFree(Some(global));
            return false;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), target, wide.len());
        let _ = GlobalUnlock(global);
        // After a success, the clipboard owns the memory.
        if SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(global.0))).is_err() {
            let _ = GlobalFree(Some(global));
            return false;
        }
        true
    }
}
