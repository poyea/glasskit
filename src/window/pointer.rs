// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The mouse: clicks, drags, scrolling, and the context menu.

use windows::Win32::Foundation::{HWND, LPARAM, POINT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageTime, GetSystemMetrics, SM_CXDOUBLECLK, SM_CYDOUBLECLK,
};

use super::WindowEvent;
use super::draw::Dragging;
use super::state::{Clicks, State};
use crate::edit::Editor;
use crate::native;
use crate::widget::AnyId;
use crate::widget::tree::Content;

/// How far a scroll moves for one notch of the wheel, in DIPs: three lines of body text.
const WHEEL_STEP: f32 = 60.0;

/// How far the pointer moves with the button down before a drag starts, in DIPs.
const DRAG_START: f32 = 4.0;

impl State {
    /// Move the widget that the pointer holds. It gives false when the pointer drags
    /// nothing.
    pub(super) fn drag_to(&self, hwnd: HWND, point: (f32, f32)) -> bool {
        let Some(mut grab) = self.grab.get() else {
            return false;
        };
        let offset = (point.0 - grab.start.0, point.1 - grab.start.1);
        if !grab.dragging {
            if offset.0.abs().max(offset.1.abs()) < DRAG_START {
                return false;
            }
            grab.dragging = true;
            self.grab.set(Some(grab));
        }
        let place = self.tree.borrow().drop_at(point.0, point.1, grab.item);
        let place = place.filter(|p| p.changes);
        self.set_interaction(hwnd, |now| {
            now.hover = None;
            now.pressed = None;
            now.drag = Some(Dragging {
                item: grab.item,
                offset,
                target: place.map(|p| p.target),
                line: place.and_then(|p| p.line),
            });
        });
        true
    }

    /// End a drag. With a point, the widget drops there.
    pub(super) fn end_drag(&self, hwnd: HWND, at: Option<(f32, f32)>) {
        let Some(grab) = self.grab.take() else {
            return;
        };
        self.set_interaction(hwnd, |now| now.drag = None);
        let (true, Some((x, y))) = (grab.dragging, at) else {
            return;
        };
        let place = self.tree.borrow().drop_at(x, y, grab.item);
        if let Some(place) = place.filter(|p| p.changes) {
            self.push(WindowEvent::Dropped {
                item: AnyId(grab.item),
                target: AnyId(place.target),
                index: place.index,
            });
        }
    }

    /// Handle `WM_CONTEXTMENU`. It gives false when no widget is there.
    pub(super) fn context(&self, hwnd: HWND, lparam: LPARAM) -> bool {
        // The keyboard gives no point: the menu is for the widget with the focus.
        let found = if lparam.0 as u32 == u32::MAX {
            let tree = self.tree.borrow();
            let focus = self.now.get().focus.filter(|&f| tree.focusable(f));
            focus.map(|focus| {
                let (r, s) = (tree.nodes[focus].rect, self.scale.get());
                let mut at = POINT {
                    x: ((r.x + r.w / 2.0) * s).round() as i32,
                    y: ((r.y + r.h / 2.0) * s).round() as i32,
                };
                unsafe {
                    let _ = ClientToScreen(hwnd, &mut at);
                }
                (focus, at)
            })
        } else {
            let (x, y) = native::point(lparam);
            let at = POINT { x, y };
            let (x, y) = self.screen_point(hwnd, at);
            self.tree.borrow().hit(x, y).map(|hit| (hit, at))
        };
        let Some((index, at)) = found else {
            return false;
        };
        self.menu_at.set(Some(at));
        self.push(WindowEvent::ContextRequested(AnyId(index)));
        true
    }

    pub(super) fn is_dragging(&self) -> bool {
        self.grab.get().is_some_and(|g| g.dragging)
    }

    /// Count the presses of the left button: 1, 2 for a double click, or 3 for a triple
    /// click.
    pub(super) fn count_click(&self, lparam: LPARAM) -> u32 {
        let at = native::point(lparam);
        let time = unsafe { GetMessageTime() } as u32;
        let last = self.clicks.get();
        let (w, h) = unsafe {
            (
                GetSystemMetrics(SM_CXDOUBLECLK),
                GetSystemMetrics(SM_CYDOUBLECLK),
            )
        };
        let near = (at.0 - last.at.0).abs() * 2 <= w && (at.1 - last.at.1).abs() * 2 <= h;
        let soon = time.wrapping_sub(last.time) <= unsafe { GetDoubleClickTime() };
        let count = if near && soon && (1..3).contains(&last.count) {
            last.count + 1
        } else {
            1
        };
        self.clicks.set(Clicks { count, time, at });
        count
    }

    /// Scroll for the mouse wheel: a text box with many lines at the point, else the
    /// innermost scroll there that can move. It gives false when nothing scrolled.
    pub(super) fn wheel(&self, hwnd: HWND, (x, y): (f32, f32), delta: f32) -> bool {
        if self.wheel_box(hwnd, (x, y), delta) {
            return true;
        }
        // Delta is positive when the user turns the wheel away, to see what is above.
        let step = -delta / 120.0 * WHEEL_STEP;
        let Some(index) = self.tree.borrow().scroll_at(x, y, step) else {
            return false;
        };
        self.tree.borrow_mut().scroll_by(index, step);
        self.invalidate(hwnd);
        true
    }

    fn wheel_box(&self, hwnd: HWND, (x, y): (f32, f32), delta: f32) -> bool {
        let Some(index) = self.tree.borrow().hit(x, y) else {
            return false;
        };
        if !self.is_box(index) || self.read_box(index, Editor::multiline) != Some(true) {
            return false;
        }
        let Some(field) = self.field(index) else {
            return false;
        };
        // Three lines for each notch of the wheel, as Windows does by default.
        let step = delta / 120.0 * 3.0 * field.line_height();
        if let Some(Content::TextBox { scroll, .. }) = self
            .tree
            .borrow_mut()
            .get_mut(index)
            .map(|n| &mut n.content)
        {
            *scroll = (*scroll - step).clamp(0.0, field.max_scroll());
        }
        self.invalidate(hwnd);
        true
    }
}
