// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! A frosted-glass popup that asks a question with choice buttons.
//!
//! The blurred desktop shows through the popup. It fades in at a [`Placement`] in the work
//! area of the primary monitor, stays on top, and does not take the keyboard focus.

mod countdown;
mod glass;
mod layout;
mod prompt;
mod state;

use std::ffi::c_void;
use std::rc::Rc;
use std::time::SystemTime;

use windows::Win32::Foundation::{HINSTANCE, HWND, RECT, TRUE};
use windows::Win32::Graphics::Dwm::{
    DWMWA_USE_HOSTBACKDROPBRUSH, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_DONOTROUND, DWMWINDOWATTRIBUTE, DwmSetWindowAttribute,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, IDC_ARROW, SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    SystemParametersInfoW, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{BOOL, PCWSTR, w};

use crate::Ui;
use crate::error::{Call, Error, Result};
use crate::events::PopupId;
use crate::placement::{Area, Placement, SCREEN_MARGIN};
use crate::theme::Theme;
use crate::window::{self, Window};
use countdown::{label, whole_seconds};
use glass::{Graphics, Scene};
pub use prompt::{Mark, PopupStyle, Prompt};
use state::{WindowState, wndproc};

const CLASS_NAME: PCWSTR = w!("GlasskitChoicePopup");

/// How a popup ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// The answer of [`ChoicePopup::ask`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reply {
    /// How the popup ended.
    pub response: Response,
    /// The time of the click or of the timeout, not the time the popup closed.
    pub at: SystemTime,
}

/// A popup that asks one [`Prompt`] at a time.
///
/// It keeps no window between prompts: [`ChoicePopup::show`] makes the window and the GPU
/// resources, and [`ChoicePopup::take_reply`] frees them.
pub struct ChoicePopup {
    ui: Ui,
    id: PopupId,
    placement: Placement,
    style: PopupStyle,
    active: Option<Active>,
}

/// The window of a prompt that shows, and what it needs.
struct Active {
    window: Window<WindowState>,
    /// The GPU resources. They go last, after the window and its scene.
    _gfx: Graphics,
}

impl ChoicePopup {
    /// The next popup uses this style.
    pub fn set_style(&mut self, style: PopupStyle) {
        self.style = style;
    }

    /// The next popup shows at this place in the work area.
    pub fn set_placement(&mut self, placement: Placement) {
        self.placement = placement;
    }

    /// A popup that shows at `placement` in the work area.
    pub fn new(ui: &Ui, placement: Placement) -> Result<Self> {
        window::register_class(CLASS_NAME, Some(wndproc), ui.hinstance(), IDC_ARROW)?;
        Ok(Self {
            ui: ui.clone(),
            id: PopupId(ui.new_id()),
            placement,
            style: PopupStyle::default(),
            active: None,
        })
    }

    /// The id that [`Event::Popup`](crate::events::Event::Popup) gives when a prompt of this
    /// popup ends.
    pub fn id(&self) -> PopupId {
        self.id
    }

    /// Whether a prompt shows now.
    pub fn is_showing(&self) -> bool {
        self.active.is_some()
    }

    /// Show the prompt and return at once. The prompt ends when the user clicks a choice or the
    /// dismiss button, or when the countdown ends. Then the event loop gives
    /// [`Event::Popup`](crate::events::Event::Popup) with [`ChoicePopup::id`], and
    /// [`ChoicePopup::take_reply`] gives the reply. A click on the dismiss button, or on a choice
    /// of a prompt without marks, ends the prompt at once.
    ///
    /// It fails with [`Error::PopupShowing`] while another prompt of this popup shows.
    pub fn show(&mut self, request: &Prompt) -> Result<()> {
        if self.active.is_some() {
            return Err(Error::PopupShowing);
        }
        let scale = primary_monitor_scale();
        let gfx = Graphics::new()?;
        let fonts = layout::Fonts::new(&gfx, scale, &self.style)?;
        let dismiss_label = match request.auto_dismiss {
            Some(after) => label(&request.dismiss, whole_seconds(after)),
            None => request.dismiss.to_string(),
        };
        let layout = layout::build(
            scale,
            self.style.width,
            &gfx,
            &fonts,
            &request.text,
            &request.choices,
            &dismiss_label,
        )?;
        let theme = Theme::current();
        let hwnd = create_window(self.ui.hinstance(), &layout, self.placement, scale, &theme)?;
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
                window::destroy(hwnd);
                return Err(err);
            }
        };
        let state = WindowState::new(
            self.id.0,
            self.style,
            layout,
            scene,
            theme,
            fonts.button,
            request,
        );
        let handle = Window::attach(hwnd, Rc::new(state));
        handle.state().open(hwnd)?;
        self.active = Some(Active {
            window: handle,
            _gfx: gfx,
        });
        Ok(())
    }

    /// The reply of the prompt, once it has ended. It closes the window and frees its GPU
    /// resources. It gives `None` while the prompt still shows, and when no prompt shows.
    pub fn take_reply(&mut self) -> Option<Reply> {
        let reply = self.active.as_ref()?.window.state().reply()?;
        self.close();
        Some(reply)
    }

    /// Close the prompt that shows, without a reply.
    pub fn close(&mut self) {
        self.active = None;
    }

    /// Show the prompt and wait until it ends, with a message loop of its own. It is for an app
    /// without an [`EventLoop`](crate::events::EventLoop); with one, use
    /// [`ChoicePopup::show`]. If the thread gets `WM_QUIT`, the popup closes, posts `WM_QUIT`
    /// again for the caller, and gives [`Response::Quit`].
    pub fn ask(&mut self, request: &Prompt) -> Result<Reply> {
        self.show(request)?;
        loop {
            if let Some(reply) = self.take_reply() {
                return Ok(reply);
            }
            match self.ui.pump_wait() {
                Ok(true) => {}
                Ok(false) => {
                    let at = SystemTime::now();
                    self.close();
                    return Ok(Reply {
                        response: Response::Quit,
                        at,
                    });
                }
                Err(err) => {
                    self.close();
                    return Err(err);
                }
            }
        }
    }
}

impl Drop for ChoicePopup {
    fn drop(&mut self) {
        self.close();
    }
}

fn primary_monitor_scale() -> f32 {
    crate::primary_monitor_dpi() as f32 / 96.0
}

/// Create the popup window at `placement` in the work area, with its DWM attributes.
fn create_window(
    hinstance: HINSTANCE,
    layout: &layout::Layout,
    placement: Placement,
    scale: f32,
    theme: &Theme,
) -> Result<HWND> {
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
        let (win_w, win_h) = (layout.width as i32, layout.height.ceil() as i32);
        let (x, y) = placement.place(
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
            Some(hinstance),
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
        Ok(hwnd)
    }
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
