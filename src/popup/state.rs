// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The state of a prompt window: its mouse and timer handling, and its drawing.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant, SystemTime};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ROUNDED_RECT, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
    ID2D1DeviceContext,
};
use windows::Win32::Graphics::DirectWrite::{DWRITE_MEASURING_MODE_NATURAL, IDWriteTextFormat};
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, HWND_TOPMOST, IDC_HAND, KillTimer, LoadCursorW, MA_NOACTIVATE,
    SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetCursor, SetTimer, SetWindowPos,
    ShowWindow, WM_DESTROY, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE,
    WM_SETCURSOR, WM_TIMER,
};
use windows_numerics::Vector2;

use super::countdown::{self, Countdown};
use super::glass::{self, Scene};
use super::layout::Layout;
use super::{Mark, PopupStyle, Prompt, Reply, Response};
use crate::error::{Call, Result};
use crate::theme::Theme;
use crate::ui::Notice;
use crate::window::state_of;

/// The window closes this long after the fade-out, so the last frame shows.
const FADE_OUT_EXTRA: Duration = Duration::from_millis(50);
const TIMER_COUNTDOWN: usize = 1;
const TIMER_FADE_OUT: usize = 2;
const TIMER_FEEDBACK: usize = 3;

/// The window procedure can run again inside a call that it makes. For example,
/// `ReleaseCapture` sends `WM_CAPTURECHANGED`. Thus the state is shared and changes only in
/// `Cell` and `RefCell`. Handlers do not destroy the window. They set `response`, and the
/// loop in `ChoicePopup::ask` destroys the window.
pub(super) struct WindowState {
    pub(super) id: u32,
    pub(super) style: PopupStyle,
    pub(super) layout: RefCell<Layout>,
    pub(super) scene: Scene,
    pub(super) theme: Theme,
    pub(super) button_format: IDWriteTextFormat,
    hover: Cell<Option<usize>>,
    pressed: Cell<Option<usize>>,
    pointer_inside: Cell<bool>,
    dismiss: String,
    countdown: RefCell<Option<Countdown>>,
    marks: Option<Vec<Vec<(usize, Mark)>>>,
    selected: Cell<Option<usize>>,
    /// True from the start of the feedback or the fade-out. Then the popup ignores the mouse.
    closing: Cell<bool>,
    /// The response to give when the fade-out is complete.
    pending: Cell<Option<Response>>,
    decided_at: Cell<Option<SystemTime>>,
    response: Cell<Option<Response>>,
}

impl WindowState {
    pub(super) fn new(
        id: u32,
        style: PopupStyle,
        layout: Layout,
        scene: Scene,
        theme: Theme,
        button_format: IDWriteTextFormat,
        request: &Prompt,
    ) -> Self {
        Self {
            id,
            style,
            layout: RefCell::new(layout),
            scene,
            theme,
            button_format,
            hover: Cell::new(None),
            pressed: Cell::new(None),
            pointer_inside: Cell::new(false),
            dismiss: request.dismiss.to_string(),
            countdown: RefCell::new(request.auto_dismiss.map(Countdown::new)),
            marks: request.marks.clone(),
            selected: Cell::new(None),
            closing: Cell::new(false),
            pending: Cell::new(None),
            decided_at: Cell::new(None),
            response: Cell::new(None),
        }
    }

    /// The reply, once the prompt has ended.
    pub(super) fn reply(&self) -> Option<Reply> {
        let response = self.response.get()?;
        let at = self.decided_at.get().unwrap_or_else(SystemTime::now);
        Some(Reply { response, at })
    }

    /// Draw the first frame, show the window, and start the fade-in and the countdown.
    pub(super) fn open(&self, hwnd: HWND) -> Result<()> {
        self.draw()?;
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
            .call("SetWindowPos")?;
        }
        self.scene.fade(0.0, 1.0, self.style.fade)?;
        self.resume_countdown(hwnd);
        Ok(())
    }

    /// End the prompt with this response, and wake the event loop. A later response is
    /// ignored.
    fn finish(&self, response: Response) {
        if self.response.get().is_none() {
            self.response.set(Some(response));
            crate::ui::notify(Notice::Popup(self.id));
        }
    }

    /// Called when the timer of the countdown fires. It updates the label, if the whole
    /// seconds changed, and sets the next timer.
    fn tick(&self, hwnd: HWND) {
        let (left, label, running) = {
            let mut countdown = self.countdown.borrow_mut();
            let Some(c) = countdown.as_mut() else {
                return;
            };
            let left = c.remaining(Instant::now());
            let seconds = countdown::whole_seconds(left);
            let mut label = None;
            if seconds != c.shown {
                c.shown = seconds;
                label = Some(countdown::label(&self.dismiss, seconds));
            }
            (left, label, c.is_running())
        };
        if left.is_zero() {
            self.decide();
            self.fade_out(hwnd, Response::TimedOut);
            return;
        }
        if let Some(label) = label {
            if let Some(b) = self.layout.borrow_mut().buttons.last_mut() {
                b.label = label.encode_utf16().collect();
            }
            self.redraw();
        }
        if running {
            self.arm_countdown(hwnd, left);
        }
    }

    /// Let the countdown run from its time left, and wake up at its next whole second.
    fn resume_countdown(&self, hwnd: HWND) {
        let now = Instant::now();
        let left = self.countdown.borrow_mut().as_mut().map(|c| {
            c.resume(now);
            c.remaining(now)
        });
        if let Some(left) = left {
            self.arm_countdown(hwnd, left);
        }
    }

    /// Stop the countdown while the pointer is on the popup. It keeps the time left.
    fn pause_countdown(&self, hwnd: HWND) {
        if let Some(c) = self.countdown.borrow_mut().as_mut() {
            c.pause(Instant::now());
        }
        unsafe {
            let _ = KillTimer(Some(hwnd), TIMER_COUNTDOWN);
        }
    }

    fn arm_countdown(&self, hwnd: HWND, left: Duration) {
        let delay = countdown::until_next_second(left);
        unsafe {
            SetTimer(Some(hwnd), TIMER_COUNTDOWN, delay.as_millis() as u32, None);
        }
    }

    fn decide(&self) {
        if self.decided_at.get().is_none() {
            self.decided_at.set(Some(SystemTime::now()));
        }
    }

    /// Stop the countdown and ignore the mouse from now on.
    fn close_input(&self, hwnd: HWND) {
        self.countdown.replace(None);
        self.closing.set(true);
        self.hover.set(None);
        unsafe {
            let _ = KillTimer(Some(hwnd), TIMER_COUNTDOWN);
            if self.pressed.take().is_some() {
                let _ = ReleaseCapture();
            }
        }
    }

    fn show_feedback(&self, hwnd: HWND, selected: usize) {
        self.close_input(hwnd);
        self.selected.set(Some(selected));
        self.redraw();
        let held = unsafe {
            SetTimer(
                Some(hwnd),
                TIMER_FEEDBACK,
                self.style.hold.as_millis() as u32,
                None,
            ) != 0
        };
        if !held {
            self.fade_out(hwnd, Response::Choice(selected));
        }
    }

    fn fade_out(&self, hwnd: HWND, response: Response) {
        self.close_input(hwnd);
        self.pending.set(Some(response));
        unsafe {
            let started = match self.scene.fade(1.0, 0.0, self.style.fade) {
                Ok(()) => {
                    SetTimer(
                        Some(hwnd),
                        TIMER_FADE_OUT,
                        (self.style.fade + FADE_OUT_EXTRA).as_millis() as u32,
                        None,
                    ) != 0
                }
                // Without the animation, the popup closes at once.
                Err(_) => false,
            };
            if !started {
                self.finish(response);
            }
        }
    }

    fn hit(&self, lparam: LPARAM) -> Option<usize> {
        let x = (lparam.0 & 0xFFFF) as i16 as f32;
        let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
        self.layout.borrow().buttons.iter().position(|b| {
            x >= b.rect.left && x < b.rect.right && y >= b.rect.top && y < b.rect.bottom
        })
    }

    fn draw(&self) -> Result<()> {
        let t = &self.theme;
        let l = &*self.layout.borrow();
        let (hover, pressed) = (self.hover.get(), self.pressed.get());
        let shown: &[(usize, Mark)] = match (self.selected.get(), &self.marks) {
            (Some(clicked), Some(marks)) => &marks[clicked],
            _ => &[],
        };
        self.scene.draw(|dc: &ID2D1DeviceContext| unsafe {
            dc.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let frame = D2D1_ROUNDED_RECT {
                rect: glass::rect(0.5, 0.5, l.width - 1.0, l.height - 1.0),
                radiusX: l.window_radius,
                radiusY: l.window_radius,
            };
            let window_border = dc.CreateSolidColorBrush(&t.window_border, None)?;
            dc.DrawRoundedRectangle(&frame, &window_border, 1.0, None);

            let text = dc.CreateSolidColorBrush(&t.text, None)?;
            dc.DrawTextLayout(
                Vector2 { X: l.pad, Y: l.pad },
                &l.prompt,
                &text,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );

            let border = dc.CreateSolidColorBrush(&t.button_border, None)?;
            for (i, b) in l.buttons.iter().enumerate() {
                let mark = shown
                    .iter()
                    .find(|&&(index, _)| index == i)
                    .map(|&(_, m)| m);
                let fill = match mark {
                    Some(Mark::Good) => t.good,
                    Some(Mark::Bad) => t.bad,
                    _ if pressed == Some(i) && hover == Some(i) => t.button_pressed,
                    _ if hover == Some(i) => t.button_hover,
                    _ => t.button,
                };
                let fill = dc.CreateSolidColorBrush(&fill, None)?;
                let shape = D2D1_ROUNDED_RECT {
                    rect: b.rect,
                    radiusX: l.radius,
                    radiusY: l.radius,
                };
                dc.FillRoundedRectangle(&shape, &fill);
                let outline = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: b.rect.left + 0.5,
                        top: b.rect.top + 0.5,
                        right: b.rect.right - 0.5,
                        bottom: b.rect.bottom - 0.5,
                    },
                    radiusX: l.radius,
                    radiusY: l.radius,
                };
                dc.DrawRoundedRectangle(&outline, &border, 1.0, None);
                dc.DrawText(
                    &b.label,
                    &self.button_format,
                    &b.rect,
                    &text,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
            Ok(())
        })
    }

    /// A failed redraw keeps the last frame. The next redraw tries again.
    fn redraw(&self) {
        let _ = self.draw();
    }
}

pub(super) extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let Some(state) = state_of::<WindowState>(hwnd) else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };

        match msg {
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_TIMER => {
                match wparam.0 {
                    TIMER_COUNTDOWN => state.tick(hwnd),
                    TIMER_FEEDBACK => {
                        let _ = KillTimer(Some(hwnd), TIMER_FEEDBACK);
                        if let Some(selected) = state.selected.get() {
                            state.fade_out(hwnd, Response::Choice(selected));
                        }
                    }
                    TIMER_FADE_OUT => {
                        if let Some(response) = state.pending.get() {
                            state.finish(response);
                        }
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP if state.closing.get() => LRESULT(0),
            WM_MOUSEMOVE => {
                if !state.pointer_inside.get() {
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    if TrackMouseEvent(&mut tme).is_ok() {
                        state.pointer_inside.set(true);
                        state.pause_countdown(hwnd);
                    }
                }
                let hit = state.hit(lparam);
                if hit != state.hover.get() {
                    state.hover.set(hit);
                    state.redraw();
                }
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                state.pointer_inside.set(false);
                state.resume_countdown(hwnd);
                state.hover.set(None);
                state.redraw();
                LRESULT(0)
            }
            WM_SETCURSOR if state.hover.get().is_some() => {
                if let Ok(hand) = LoadCursorW(None, IDC_HAND) {
                    SetCursor(Some(hand));
                }
                LRESULT(1)
            }
            WM_LBUTTONDOWN => {
                let hit = state.hit(lparam);
                state.pressed.set(hit);
                if hit.is_some() {
                    SetCapture(hwnd);
                    state.redraw();
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let pressed = state.pressed.take();
                let _ = ReleaseCapture();
                let hit = state.hit(lparam);
                match pressed {
                    Some(i) if hit == Some(i) => {
                        state.decide();
                        let response = state.layout.borrow().buttons[i].response;
                        match response {
                            Response::Choice(choice) if state.marks.is_some() => {
                                state.show_feedback(hwnd, choice);
                            }
                            _ => state.finish(response),
                        }
                    }
                    Some(_) => {
                        state.hover.set(hit);
                        state.redraw();
                    }
                    None => {}
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                state.finish(Response::Dismissed);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
