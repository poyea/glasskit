// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The placement picker: a small picture of the screen where the user drags a window.

use super::*;

pub(super) const PICKER_LABEL_HEIGHT: f32 = 20.0;

pub(super) const PICKER_SCREEN_RADIUS: f32 = 6.0;

pub(super) const PICKER_WINDOW_RADIUS: f32 = 3.0;

/// A fraction this near to an edge or to the center snaps to it.
pub(super) const PICKER_SNAP: f32 = 0.04;

pub(super) const PICKER_KEY_STEP: f32 = 0.05;

/// Where the parts of a placement picker are, in its client pixels. The picker draws the work
/// area at a smaller scale, with the window in it at the same scale.
pub(super) struct Picker {
    pub(super) screen: Area,
    pub(super) window_width: f32,
    pub(super) window_height: f32,
    pub(super) margin: f32,
    pub(super) work: RECT,
}

impl Picker {
    pub(super) fn new(shared: &Shared, index: usize, hwnd: HWND) -> Self {
        let s = shared.look.scale;
        let mut client = RECT::default();
        unsafe {
            let _ = GetClientRect(hwnd, &mut client);
        }
        let work = work_area();
        let (work_w, work_h) = (
            (work.right - work.left) as f32,
            (work.bottom - work.top) as f32,
        );
        let room_w = (client.right as f32 - 2.0).max(1.0);
        let room_h = (client.bottom as f32 - PICKER_LABEL_HEIGHT * s - 2.0).max(1.0);
        let k = (room_w / work_w).min(room_h / work_h);
        let (w, h) = (work_w * k, work_h * k);
        let left = ((client.right as f32 - w) / 2.0).floor();
        let (window_w, window_h) = shared.window_sizes.borrow()[index];
        Self {
            screen: Area {
                left,
                top: 1.0,
                right: left + w,
                bottom: 1.0 + h,
            },
            window_width: window_w as f32 * s * k,
            window_height: window_h as f32 * s * k,
            margin: SCREEN_MARGIN * s * k,
            work,
        }
    }

    pub(super) fn window(&self, placement: Placement) -> D2D_RECT_F {
        let (left, top) = placement.place_exact(
            self.screen,
            self.window_width,
            self.window_height,
            self.margin,
        );
        D2D_RECT_F {
            left,
            top,
            right: left + self.window_width,
            bottom: top + self.window_height,
        }
    }

    /// The placement that puts the top-left corner of the window at `corner`, snapped.
    pub(super) fn placement_at(&self, corner: (f32, f32)) -> Placement {
        let p = Placement::at_exact(
            self.screen,
            self.window_width,
            self.window_height,
            self.margin,
            corner,
        );
        Placement::new(snap(p.x()), snap(p.y()))
    }
}

/// Move a fraction to an edge or to the center when it is near, so the named places are easy
/// to get exactly.
pub(super) fn snap(fraction: f32) -> f32 {
    [0.0, 0.5, 1.0]
        .into_iter()
        .find(|target| (fraction - target).abs() < PICKER_SNAP)
        .unwrap_or(fraction)
}

pub(super) fn paint_placement(shared: &Shared, index: usize, hwnd: HWND, hdc: HDC) {
    let look = &shared.look;
    let Some(painter) = &look.painter else {
        return;
    };
    let theme = &look.theme;
    let s = look.scale;
    let picker = Picker::new(shared, index, hwnd);
    let window = picker.window(shared.placements.borrow()[index]);
    let (surface, _) = look.surface(shared.surfaces.borrow()[index]);
    let focused = unsafe { GetFocus() } == hwnd;
    let size: Vec<u16> = format!(
        "{} \u{d7} {}",
        picker.work.right - picker.work.left,
        picker.work.bottom - picker.work.top
    )
    .encode_utf16()
    .collect();
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut client);
    }
    painter.paint(hdc, &client, |t| unsafe {
        t.Clear(Some(&surface));
        let (Ok(screen_fill), Ok(border), Ok(ring), Ok(window_fill), Ok(lines), Ok(label)) = (
            t.CreateSolidColorBrush(&look.field, None),
            t.CreateSolidColorBrush(&theme.button_border, None),
            t.CreateSolidColorBrush(&faded(theme.text, 0.7), None),
            t.CreateSolidColorBrush(&faded(theme.text, 0.85), None),
            t.CreateSolidColorBrush(&look.field, None),
            t.CreateSolidColorBrush(&faded(theme.text, 0.6), None),
        ) else {
            return;
        };
        let a = picker.screen;
        let screen = rounded(
            a.left + 0.5,
            a.top + 0.5,
            a.right - 0.5,
            a.bottom - 0.5,
            PICKER_SCREEN_RADIUS * s,
        );
        t.FillRoundedRectangle(&screen, &screen_fill);
        if focused {
            t.DrawRoundedRectangle(&screen, &ring, 1.5 * s, None);
        } else {
            t.DrawRoundedRectangle(&screen, &border, 1.0, None);
        }
        let shape = rounded(
            window.left,
            window.top,
            window.right,
            window.bottom,
            PICKER_WINDOW_RADIUS * s,
        );
        t.FillRoundedRectangle(&shape, &window_fill);
        // Lines like the question text and the choices, so the box looks like the popup.
        let (w, h) = (window.right - window.left, window.bottom - window.top);
        let stroke = (h * 0.05).max(1.0);
        for (y, length) in [(0.18, 0.62), (0.40, 0.76), (0.56, 0.76), (0.72, 0.76)] {
            let from = Vector2 {
                X: window.left + w * 0.12,
                Y: window.top + h * y,
            };
            let to = Vector2 {
                X: window.left + w * (0.12 + length),
                Y: window.top + h * y,
            };
            t.DrawLine(from, to, &lines, stroke, None);
        }
        let area = D2D_RECT_F {
            left: 0.0,
            top: a.bottom + 2.0,
            right: client.right as f32,
            bottom: client.bottom as f32,
        };
        t.DrawText(
            &size,
            &painter.centered,
            &area,
            &label,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    });
}

/// Store a placement that the user made, repaint, and tell the app.
pub(super) fn user_placement(shared: &Shared, index: usize, hwnd: HWND, placement: Placement) {
    {
        let mut placements = shared.placements.borrow_mut();
        if placements[index] == placement {
            return;
        }
        placements[index] = placement;
    }
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    if !shared.muted.get() {
        shared.push(Event::Changed(AnyId(index)));
    }
}

pub(super) extern "system" fn placement_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let parent = GetParent(hwnd).unwrap_or_default();
        let Some(shared) = state_of::<Shared>(parent) else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };
        let Some(index) = shared.index_of(hwnd) else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };
        let point = || {
            let (x, y) = native::point(lparam);
            (x as f32, y as f32)
        };
        match msg {
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                paint_placement(shared, index, hwnd, hdc);
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_GETDLGCODE => LRESULT(DLGC_WANTARROWS as isize),
            WM_SETFOCUS | WM_KILLFOCUS => {
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let _ = SetFocus(Some(hwnd));
                SetCapture(hwnd);
                let picker = Picker::new(shared, index, hwnd);
                let window = picker.window(shared.placements.borrow()[index]);
                let (x, y) = point();
                let inside =
                    x >= window.left && x < window.right && y >= window.top && y < window.bottom;
                // Drag from the clicked point. A click outside the window moves its center there.
                let grab = if inside {
                    (x - window.left, y - window.top)
                } else {
                    (picker.window_width / 2.0, picker.window_height / 2.0)
                };
                shared.grab.set(Some(grab));
                let placement = picker.placement_at((x - grab.0, y - grab.1));
                user_placement(shared, index, hwnd, placement);
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if let Some(grab) = shared.grab.get() {
                    let (x, y) = point();
                    let picker = Picker::new(shared, index, hwnd);
                    let placement = picker.placement_at((x - grab.0, y - grab.1));
                    user_placement(shared, index, hwnd, placement);
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                shared.grab.set(None);
                let _ = ReleaseCapture();
                LRESULT(0)
            }
            WM_CAPTURECHANGED => {
                shared.grab.set(None);
                LRESULT(0)
            }
            WM_KEYDOWN => {
                let (dx, dy) = match VIRTUAL_KEY(wparam.0 as u16) {
                    VK_LEFT => (-PICKER_KEY_STEP, 0.0),
                    VK_RIGHT => (PICKER_KEY_STEP, 0.0),
                    VK_UP => (0.0, -PICKER_KEY_STEP),
                    VK_DOWN => (0.0, PICKER_KEY_STEP),
                    _ => return DefWindowProcW(hwnd, msg, wparam, lparam),
                };
                let p = shared.placements.borrow()[index];
                let moved = Placement::new(snap(p.x() + dx), snap(p.y() + dy));
                user_placement(shared, index, hwnd, moved);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
