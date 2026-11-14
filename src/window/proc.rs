// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The window procedure, which handles the messages of a window.

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::ValidateRect;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GetCursorPos, HTCLIENT, IDC_IBEAM, LoadCursorW, MINMAXINFO, SIZE_MINIMIZED,
    SWP_NOACTIVATE, SWP_NOZORDER, SetCursor, SetWindowPos, WM_APP, WM_CAPTURECHANGED, WM_CHAR,
    WM_CLOSE, WM_CONTEXTMENU, WM_DPICHANGED, WM_ERASEBKGND, WM_GETMINMAXINFO,
    WM_IME_STARTCOMPOSITION, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_PAINT, WM_SETCURSOR, WM_SETFOCUS, WM_SETTINGCHANGE, WM_SIZE,
    WM_SYSCOLORCHANGE, WM_SYSKEYDOWN, WM_THEMECHANGED, WM_TIMER,
};

use super::state::{Grab, State, client_pixels, outer_size};
use super::typing::TIMER_BLINK;
use super::{Key, WindowEvent};
use crate::native::{self, state_of};

/// A private message: draw the window again, once for many changes.
pub(super) const WM_REDRAW: u32 = WM_APP + 1;

const WM_DWMCOLORIZATIONCOLORCHANGED: u32 = 0x0320;

pub(super) extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let Some(state) = state_of::<State>(hwnd) else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };
        match msg {
            WM_REDRAW => {
                state.redraw_posted.set(false);
                state.render(hwnd);
                LRESULT(0)
            }
            WM_PAINT => {
                // The content is in the Composition tree, so there is nothing to paint here.
                let _ = ValidateRect(Some(hwnd), None);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_SIZE => {
                if wparam.0 != SIZE_MINIMIZED as usize {
                    let (w, h) = client_pixels(hwnd);
                    let s = state.scale.get();
                    state.size.set((w as f32 / s, h as f32 / s));
                    state.with_scene(|scene| scene.resize(w, h));
                    // Draw now, so the window never shows a frame of the old size.
                    state.render(hwnd);
                }
                LRESULT(0)
            }
            WM_DPICHANGED => {
                state.scale.set((wparam.0 & 0xFFFF) as f32 / 96.0);
                let r = &*(lparam.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                let (w, h) = client_pixels(hwnd);
                let s = state.scale.get();
                state.size.set((w as f32 / s, h as f32 / s));
                state.render(hwnd);
                LRESULT(0)
            }
            WM_GETMINMAXINFO => {
                let (w, h) = state.min_size.get();
                if w > 0.0 || h > 0.0 {
                    let (ow, oh) = outer_size(w, h, state.scale.get());
                    let info = &mut *(lparam.0 as *mut MINMAXINFO);
                    info.ptMinTrackSize.x = ow;
                    info.ptMinTrackSize.y = oh;
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                state.push(WindowEvent::CloseRequested);
                LRESULT(0)
            }
            WM_SETTINGCHANGE
            | WM_THEMECHANGED
            | WM_SYSCOLORCHANGE
            | WM_DWMCOLORIZATIONCOLORCHANGED => {
                // Windows sends these for many settings. Most of them do not change the look.
                if state.apply_theme(hwnd) {
                    state.render(hwnd);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_MOUSEMOVE => {
                if !state.tracking.get() {
                    state.tracking.set(native::track_leave(hwnd));
                }
                let point = state.point(lparam);
                if state.drag_to(hwnd, point) {
                    return LRESULT(0);
                }
                let hit = state.tree.borrow().hit(point.0, point.1);
                state.set_interaction(hwnd, |now| now.hover = hit);
                // A drag from a text box selects text. After a double click, the word stays.
                if let Some(p) = state.now.get().pressed.filter(|&p| state.is_box(p))
                    && state.clicks.get().count == 1
                {
                    state.place_caret(hwnd, p, point, true);
                }
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                state.tracking.set(false);
                state.set_interaction(hwnd, |now| now.hover = None);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let clicks = state.count_click(lparam);
                let point = state.point(lparam);
                let hit = state.tree.borrow().hit(point.0, point.1);
                // A press on a button in a card does not drag the card.
                let source = state.tree.borrow().drag_source_at(point.0, point.1);
                let source = source.filter(|&s| hit.is_none_or(|h| h == s));
                state.grab.set(source.map(|item| Grab {
                    item,
                    start: point,
                    dragging: false,
                }));
                if hit.is_some() || source.is_some() {
                    SetCapture(hwnd);
                }
                state.set_interaction(hwnd, |now| {
                    now.pressed = hit;
                    now.hover = hit;
                    if hit.is_some() {
                        now.focus = hit;
                        now.cues = false;
                    }
                });
                if let Some(h) = hit.filter(|&h| state.is_box(h)) {
                    if clicks > 1 {
                        state.select_at(hwnd, h, point, clicks);
                    } else {
                        let shift = GetKeyState(i32::from(VK_SHIFT.0)) < 0;
                        state.place_caret(hwnd, h, point, shift);
                    }
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                if state.is_dragging() {
                    state.end_drag(hwnd, Some(state.point(lparam)));
                    let _ = ReleaseCapture();
                    return LRESULT(0);
                }
                state.grab.set(None);
                let pressed = state.now.get().pressed;
                let (x, y) = state.point(lparam);
                let hit = state.tree.borrow().hit(x, y);
                state.set_interaction(hwnd, |now| now.pressed = None);
                let _ = ReleaseCapture();
                if let Some(p) = pressed
                    && hit == Some(p)
                    && !state.is_box(p)
                {
                    state.click(hwnd, p);
                }
                LRESULT(0)
            }
            WM_CAPTURECHANGED => {
                state.end_drag(hwnd, None);
                state.set_interaction(hwnd, |now| now.pressed = None);
                LRESULT(0)
            }
            // A posted WM_CHAR also counts, so a test can type.
            WM_CHAR if state.type_char(hwnd, wparam.0 as u16) => LRESULT(0),
            WM_SETFOCUS | WM_KILLFOCUS => {
                state.set_interaction(hwnd, |now| now.active = msg == WM_SETFOCUS);
                LRESULT(0)
            }
            WM_SETCURSOR if (lparam.0 & 0xFFFF) as u32 == HTCLIENT => {
                let mut p = POINT::default();
                let _ = GetCursorPos(&mut p);
                let (x, y) = state.screen_point(hwnd, p);
                let hit = state.tree.borrow().hit(x, y);
                if hit.is_some_and(|h| state.is_box(h))
                    && let Ok(beam) = LoadCursorW(None, IDC_IBEAM)
                {
                    SetCursor(Some(beam));
                    return LRESULT(1);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_MOUSEWHEEL => {
                let (x, y) = native::point(lparam);
                let point = state.screen_point(hwnd, POINT { x, y });
                let delta = f32::from((wparam.0 >> 16) as u16 as i16);
                if state.wheel(hwnd, point, delta) {
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_CONTEXTMENU if state.context(hwnd, lparam) => LRESULT(0),
            WM_IME_STARTCOMPOSITION => {
                state.place_ime(hwnd);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_TIMER if wparam.0 == TIMER_BLINK => {
                state.blink(hwnd);
                LRESULT(0)
            }
            // Alt and F10 come here. Windows still handles them, for Alt+F4 and the menu.
            WM_SYSKEYDOWN => {
                let repeat = (lparam.0 >> 30) & 1 == 1;
                state.push_key(Key(wparam.0 as u16), repeat);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_KEYDOWN => {
                // Bit 30 is set when the key was down before this message.
                let repeat = (lparam.0 >> 30) & 1 == 1;
                if state.key(hwnd, wparam.0 as u16, repeat) {
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
