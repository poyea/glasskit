// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! A frosted-glass popup that asks a question with choice buttons.
//!
//! The blurred desktop shows through the popup. It fades in at a [`Placement`] in the work
//! area of the primary monitor, stays on top, and does not take the keyboard focus.

mod glass;

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::time::{Duration, Instant, SystemTime};

use crate::error::{Call, Result};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, TRUE, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ROUNDED_RECT, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
    ID2D1DeviceContext,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL,
    IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dwm::{
    DWMWA_USE_HOSTBACKDROPBRUSH, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_DONOTROUND, DWMWINDOWATTRIBUTE, DwmSetWindowAttribute,
};
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetMessageW,
    GetWindowLongPtrW, HWND_TOPMOST, IDC_ARROW, IDC_HAND, IsWindow, KillTimer, LoadCursorW,
    MA_NOACTIVATE, MSG, PostMessageW, PostQuitMessage, SPI_GETWORKAREA, SW_SHOWNOACTIVATE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetCursor,
    SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, SystemParametersInfoW, TranslateMessage,
    WM_DESTROY, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_NCDESTROY,
    WM_NULL, WM_SETCURSOR, WM_TIMER, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{BOOL, PCWSTR, w};
use windows_numerics::Vector2;

use crate::Ui;
use crate::placement::{Area, Placement, SCREEN_MARGIN};
use crate::theme::Theme;
use glass::{Graphics, Scene};

const CLASS_NAME: PCWSTR = w!("GlasskitChoicePopup");
const FADE_DURATION: Duration = Duration::from_millis(300);
/// The window closes this long after the fade-out starts. The extra time lets the last frame show.
const FADE_OUT_CLOSE: Duration = Duration::from_millis(350);
const COUNTDOWN_TICK: Duration = Duration::from_millis(200);
/// How long the popup shows the result of a choice before it fades out.
const FEEDBACK_HOLD: Duration = Duration::from_millis(1500);
const TIMER_COUNTDOWN: usize = 1;
const TIMER_FADE_OUT: usize = 2;
const TIMER_FEEDBACK: usize = 3;

const WIDTH: f32 = 360.0;
const PADDING: f32 = 18.0;
const TEXT_GAP: f32 = 14.0;
const BUTTON_MIN_HEIGHT: f32 = 36.0;
const BUTTON_TEXT_PADDING: f32 = 9.0;
const BUTTON_GAP: f32 = 8.0;
const BUTTON_RADIUS: f32 = 6.0;
const WINDOW_RADIUS: f32 = 8.0;
const DISMISS_MIN_WIDTH: f32 = 72.0;
const DISMISS_TEXT_PADDING: f32 = 12.0;
const DISMISS_HEIGHT: f32 = 28.0;
const PROMPT_FONT: f32 = 16.0;
const BUTTON_FONT: f32 = 14.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// How a popup ended.
pub enum Response {
    /// The user clicked the choice at this index.
    Choice(usize),
    /// The user clicked the dismiss button.
    Dismissed,
    /// The auto-dismiss countdown came to zero, and the popup faded out.
    TimedOut,
    /// The thread got WM_QUIT while the popup showed. The popup posts WM_QUIT again for the caller.
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The answer of [`ChoicePopup::ask`].
pub struct Reply {
    /// How the popup ended.
    pub response: Response,
    /// The time of the click or of the timeout, not the time the popup closed.
    pub at: SystemTime,
}

/// What a [`ChoicePopup`] asks: a text, the choices, and how the popup ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt<'a> {
    text: &'a str,
    choices: Vec<&'a str>,
    correct: Option<usize>,
    dismiss: &'a str,
    auto_dismiss: Option<Duration>,
}

impl<'a> Prompt<'a> {
    /// A prompt with this text and these choices, one button for each. The dismiss button
    /// says "Dismiss", there is no correct choice, and the popup stays until the user clicks.
    pub fn new(text: &'a str, choices: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            text,
            choices: choices.into_iter().collect(),
            correct: None,
            dismiss: "Dismiss",
            auto_dismiss: None,
        }
    }

    /// Show the result of a click: the choice at `index` becomes green, and a wrong selected
    /// choice becomes red. Then the popup fades out. An index out of range is ignored.
    #[must_use]
    pub fn correct(mut self, index: usize) -> Self {
        self.correct = Some(index);
        self
    }

    /// The text of the dismiss button.
    #[must_use]
    pub fn dismiss_label(mut self, label: &'a str) -> Self {
        self.dismiss = label;
        self
    }

    /// With `Some(after)`, the dismiss button counts down the seconds. The count stops while
    /// the pointer is on the popup. At zero, the popup fades out and gives
    /// [`Response::TimedOut`]. `None` turns the countdown off.
    #[must_use]
    pub fn auto_dismiss(mut self, after: Option<Duration>) -> Self {
        self.auto_dismiss = after;
        self
    }
}

/// A popup that asks one [`Prompt`] at a time.
///
/// It keeps no window between prompts: [`ChoicePopup::ask`] makes the window and the GPU
/// resources, and frees them before it returns.
pub struct ChoicePopup {
    ui: Ui,
    placement: Placement,
}

fn primary_monitor_scale() -> f32 {
    crate::primary_monitor_dpi() as f32 / 96.0
}

struct Fonts {
    prompt: IDWriteTextFormat,
    button: IDWriteTextFormat,
}

impl ChoicePopup {
    /// The next popup shows at this place in the work area.
    pub fn set_placement(&mut self, placement: Placement) {
        self.placement = placement;
    }

    /// A popup that shows at `placement` in the work area.
    pub fn new(ui: &Ui, placement: Placement) -> Result<Self> {
        crate::register_class(&WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: ui.hinstance(),
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.call("LoadCursorW")?,
            lpszClassName: CLASS_NAME,
            ..Default::default()
        })?;
        Ok(Self {
            ui: ui.clone(),
            placement,
        })
    }

    /// Show the prompt and wait until it ends: the user clicks a choice or the dismiss button,
    /// the countdown ends, or the thread gets `WM_QUIT`. A click on the dismiss button, or on a
    /// choice of a prompt without a correct choice, closes the popup immediately.
    pub fn ask(&mut self, request: &Prompt) -> Result<Reply> {
        let prompt = request.text;
        let choices: &[&str] = &request.choices;
        let correct = request.correct;
        let dismiss = request.dismiss;
        let auto_dismiss = request.auto_dismiss;
        let scale = primary_monitor_scale();
        let gfx = Graphics::new()?;
        let fonts = Fonts {
            prompt: gfx.text_format(PROMPT_FONT * scale, DWRITE_FONT_WEIGHT_SEMI_BOLD, false)?,
            button: gfx.text_format(BUTTON_FONT * scale, DWRITE_FONT_WEIGHT_NORMAL, true)?,
        };
        let countdown = auto_dismiss.map(|left| Countdown {
            left,
            last_tick: Instant::now(),
            shown: whole_seconds(left),
        });
        let dismiss_label = match &countdown {
            Some(c) => countdown_label(dismiss, c.shown),
            None => dismiss.to_string(),
        };
        let layout = layout(scale, &gfx, &fonts, prompt, choices, &dismiss_label)?;
        let theme = Theme::current();
        let (win_w, win_h) = (layout.width as i32, layout.height.ceil() as i32);

        unsafe {
            let mut work = RECT::default();
            SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(&mut work as *mut _ as *mut c_void),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
            .call("SystemParametersInfoW(SPI_GETWORKAREA)")?;
            let margin = (SCREEN_MARGIN * scale).round() as i32;
            let (x, y) = self.placement.place(
                Area::from((work.left, work.top, work.right, work.bottom)),
                win_w,
                win_h,
                margin,
            );

            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_NOREDIRECTIONBITMAP,
                CLASS_NAME,
                CLASS_NAME,
                WS_POPUP,
                x,
                y,
                win_w,
                win_h,
                None,
                None,
                Some(self.ui.hinstance()),
                None,
            )
            .call("CreateWindowExW")?;

            set_dwm_attribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND.0);
            set_dwm_attribute(hwnd, DWMWA_USE_HOSTBACKDROPBRUSH, TRUE.0);
            set_dwm_attribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                BOOL::from(theme.dark).0,
            );

            let scene = match Scene::new(
                &gfx,
                hwnd,
                layout.width,
                layout.height,
                layout.window_radius,
                &theme,
            ) {
                Ok(scene) => scene,
                Err(err) => {
                    let _ = DestroyWindow(hwnd);
                    return Err(err);
                }
            };

            let state = Box::into_raw(Box::new(WindowState {
                layout: RefCell::new(layout),
                scene,
                theme,
                button_format: fonts.button.clone(),
                hover: Cell::new(None),
                pressed: Cell::new(None),
                pointer_inside: Cell::new(false),
                dismiss: dismiss.to_string(),
                countdown: RefCell::new(countdown),
                correct: correct.filter(|&c| c < choices.len()),
                selected: Cell::new(None),
                closing: Cell::new(false),
                pending: Cell::new(None),
                decided_at: Cell::new(None),
                response: Cell::new(None),
            }));
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
            let shared = &*state;

            let shown = (|| -> Result<()> {
                shared.draw()?;
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
                shared.scene.fade(0.0, 1.0, FADE_DURATION)?;
                if let Some(c) = shared.countdown.borrow_mut().as_mut() {
                    c.last_tick = Instant::now();
                    SetTimer(
                        Some(hwnd),
                        TIMER_COUNTDOWN,
                        COUNTDOWN_TICK.as_millis() as u32,
                        None,
                    );
                }
                Ok(())
            })();

            let mut quit = None;
            if shown.is_ok() {
                let mut msg = MSG::default();
                while shared.response.get().is_none() {
                    match GetMessageW(&mut msg, None, 0, 0).0 {
                        0 => {
                            quit = Some(msg.wParam.0 as i32);
                            break;
                        }
                        -1 => break,
                        _ => {}
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }

            if IsWindow(Some(hwnd)).as_bool() {
                let _ = DestroyWindow(hwnd);
            }
            let state = Box::from_raw(state);
            if let Some(code) = quit {
                PostQuitMessage(code);
            }
            shown?;
            let response = match quit {
                Some(_) => Response::Quit,
                None => state.response.get().unwrap_or(Response::Dismissed),
            };
            Ok(Reply {
                response,
                at: state.decided_at.get().unwrap_or_else(SystemTime::now),
            })
        }
    }
}

fn layout<S: AsRef<str>>(
    scale: f32,
    gfx: &Graphics,
    fonts: &Fonts,
    prompt: &str,
    choices: &[S],
    dismiss: &str,
) -> Result<Layout> {
    let s = |v: f32| (v * scale).round();
    let width = s(WIDTH);
    let pad = s(PADDING);
    let inner = width - 2.0 * pad;

    let prompt_layout = gfx.text_layout(prompt, &fonts.prompt, inner)?;
    let mut y = pad + glass::layout_height(&prompt_layout)?.ceil() + s(TEXT_GAP);

    let mut buttons = Vec::with_capacity(choices.len() + 1);
    for (i, choice) in choices.iter().enumerate() {
        let choice = choice.as_ref();
        let text_w = inner - 2.0 * s(BUTTON_TEXT_PADDING);
        let label = gfx.text_layout(choice, &fonts.button, text_w)?;
        let h = (glass::layout_height(&label)?.ceil() + 2.0 * s(BUTTON_TEXT_PADDING))
            .max(s(BUTTON_MIN_HEIGHT));
        buttons.push(Button {
            rect: glass::rect(pad, y, inner, h),
            label: choice.encode_utf16().collect(),
            response: Response::Choice(i),
        });
        y += h + s(BUTTON_GAP);
    }
    let dismiss_text = gfx.text_layout(dismiss, &fonts.button, inner)?;
    let dismiss_w = (glass::layout_width(&dismiss_text)?.ceil() + 2.0 * s(DISMISS_TEXT_PADDING))
        .max(s(DISMISS_MIN_WIDTH));
    buttons.push(Button {
        rect: glass::rect(width - pad - dismiss_w, y, dismiss_w, s(DISMISS_HEIGHT)),
        label: dismiss.encode_utf16().collect(),
        response: Response::Dismissed,
    });
    y += s(DISMISS_HEIGHT) + pad;

    Ok(Layout {
        width,
        height: y,
        pad,
        radius: s(BUTTON_RADIUS),
        window_radius: s(WINDOW_RADIUS),
        prompt: prompt_layout,
        buttons,
    })
}

fn set_dwm_attribute(hwnd: HWND, attribute: DWMWINDOWATTRIBUTE, value: i32) {
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            attribute,
            &value as *const i32 as *const c_void,
            size_of::<i32>() as u32,
        );
    }
}

struct Button {
    rect: D2D_RECT_F,
    label: Vec<u16>,
    response: Response,
}

struct Layout {
    width: f32,
    height: f32,
    pad: f32,
    radius: f32,
    window_radius: f32,
    prompt: IDWriteTextLayout,
    buttons: Vec<Button>,
}

struct Countdown {
    left: Duration,
    last_tick: Instant,
    shown: u64,
}

fn whole_seconds(d: Duration) -> u64 {
    d.as_millis().div_ceil(1000) as u64
}

fn countdown_label(dismiss: &str, seconds: u64) -> String {
    format!("{dismiss} ({seconds})")
}

/// The window procedure can run again inside a call that it makes (for example, `ReleaseCapture`
/// sends `WM_CAPTURECHANGED`). Thus it gets only shared references, and the state that changes
/// is in `Cell` and `RefCell`. Handlers do not destroy the window: they set `response`, and the
/// loop in `ChoicePopup::ask` destroys the window.
struct WindowState {
    layout: RefCell<Layout>,
    scene: Scene,
    theme: Theme,
    button_format: IDWriteTextFormat,
    hover: Cell<Option<usize>>,
    pressed: Cell<Option<usize>>,
    pointer_inside: Cell<bool>,
    dismiss: String,
    countdown: RefCell<Option<Countdown>>,
    correct: Option<usize>,
    selected: Cell<Option<usize>>,
    /// True from the start of the feedback or the fade-out. Then the popup ignores the mouse.
    closing: Cell<bool>,
    /// The response to give when the fade-out is complete.
    pending: Cell<Option<Response>>,
    decided_at: Cell<Option<SystemTime>>,
    response: Cell<Option<Response>>,
}

impl WindowState {
    fn tick(&self, hwnd: HWND) {
        let seconds = {
            let mut countdown = self.countdown.borrow_mut();
            let Some(c) = countdown.as_mut() else {
                return;
            };
            let now = Instant::now();
            if !self.pointer_inside.get() {
                c.left = c.left.saturating_sub(now - c.last_tick);
            }
            c.last_tick = now;
            if c.left.is_zero() {
                None
            } else {
                let seconds = whole_seconds(c.left);
                if seconds == c.shown {
                    return;
                }
                c.shown = seconds;
                Some(seconds)
            }
        };
        match seconds {
            None => {
                self.decide();
                self.fade_out(hwnd, Response::TimedOut);
            }
            Some(seconds) => {
                let label = countdown_label(&self.dismiss, seconds);
                if let Some(b) = self.layout.borrow_mut().buttons.last_mut() {
                    b.label = label.encode_utf16().collect();
                }
                self.redraw();
            }
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
                FEEDBACK_HOLD.as_millis() as u32,
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
            let started = match self.scene.fade(1.0, 0.0, FADE_DURATION) {
                Ok(()) => {
                    SetTimer(
                        Some(hwnd),
                        TIMER_FADE_OUT,
                        FADE_OUT_CLOSE.as_millis() as u32,
                        None,
                    ) != 0
                }
                // Without the animation, the popup closes at once.
                Err(_) => false,
            };
            if !started {
                self.response.set(Some(response));
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
        let result = self.selected.get().zip(self.correct);
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
                let fill = match result {
                    Some((_, correct)) if i == correct => t.correct,
                    Some((selected, _)) if i == selected => t.wrong,
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

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
        if state.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let state = &*state;

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
                    TIMER_FADE_OUT => state.response.set(state.pending.get()),
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
                    state.pointer_inside.set(TrackMouseEvent(&mut tme).is_ok());
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
                            Response::Choice(choice) if state.correct.is_some() => {
                                state.show_feedback(hwnd, choice);
                            }
                            _ => state.response.set(Some(response)),
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
                if state.response.get().is_none() {
                    state.response.set(Some(Response::Dismissed));
                }
                // Wake the loop in `ask`. If a sent message (not a posted message) destroyed the window,
                // GetMessageW continues to wait.
                let _ = PostMessageW(None, WM_NULL, WPARAM(0), LPARAM(0));
                LRESULT(0)
            }
            WM_NCDESTROY => {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_has_quiet_defaults() {
        let p = Prompt::new("Q?", ["a", "b"]);
        assert_eq!(p.choices, ["a", "b"]);
        assert_eq!(p.correct, None);
        assert_eq!(p.dismiss, "Dismiss");
        assert_eq!(p.auto_dismiss, None);
    }

    #[test]
    fn prompt_builders_set_each_part() {
        let p = Prompt::new("Q?", ["a", "b"])
            .correct(1)
            .dismiss_label("Skip")
            .auto_dismiss(Some(Duration::from_secs(30)));
        assert_eq!(p.correct, Some(1));
        assert_eq!(p.dismiss, "Skip");
        assert_eq!(p.auto_dismiss, Some(Duration::from_secs(30)));
    }
}
