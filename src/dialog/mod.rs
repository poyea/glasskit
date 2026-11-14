// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! A small modal window built from standard Windows controls, in the same colors as the popup.
//! The app places the controls and handles the events. `glasskit` knows nothing about the content.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::rc::Rc;

use crate::error::{Call, Result};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_IGNORE, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE,
    D2D1_ROUNDED_RECT, D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory, ID2D1RenderTarget,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TRIMMING,
    DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory,
    IDWriteFactory, IDWriteTextFormat,
};
use windows::Win32::Graphics::Dwm::{
    DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush,
    DEFAULT_CHARSET, DeleteObject, EndPaint, FW_NORMAL, FW_SEMIBOLD, FillRect, GetDC, GetTextFaceW,
    HBRUSH, HDC, HFONT, HGDIOBJ, InvalidateRect, OUT_DEFAULT_PRECIS, PAINTSTRUCT, ReleaseDC,
    SelectObject, SetBkColor, SetTextColor,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::UI::Controls::{
    DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_COMBOBOXEDIT, ODS_DISABLED, ODS_FOCUS, ODS_NOFOCUSRECT,
    ODS_SELECTED, ODT_BUTTON, ODT_COMBOBOX, ODT_LISTBOX, SetWindowTheme, TOOLTIPS_CLASSW,
    TTF_IDISHWND, TTF_SUBCLASS, TTM_ADDTOOLW, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW,
    WM_MOUSELEAVE,
};
use windows::Win32::UI::HiDpi::AdjustWindowRectExForDpi;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, GetFocus, IsWindowEnabled, ReleaseCapture, SetCapture, SetFocus, VIRTUAL_KEY,
    VK_DOWN, VK_LEFT, VK_RIGHT, VK_UP,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    BS_OWNERDRAW, CB_ADDSTRING, CB_GETCURSEL, CB_GETDROPPEDSTATE, CB_GETLBTEXT, CB_GETLBTEXTLEN,
    CB_RESETCONTENT, CB_SETCURSEL, CBS_DROPDOWNLIST, CBS_HASSTRINGS, CBS_OWNERDRAWFIXED,
    CW_USEDEFAULT, CreateWindowExW, DLGC_WANTARROWS, DefWindowProcW, ES_AUTOHSCROLL,
    ES_AUTOVSCROLL, ES_MULTILINE, ES_NUMBER, ES_WANTRETURN, GetClientRect, GetDlgCtrlID,
    GetDlgItem, GetParent, GetWindowTextLengthW, GetWindowTextW, HMENU, IDC_ARROW, IDC_SIZEALL,
    LB_ADDSTRING, LB_DELETESTRING, LB_GETCURSEL, LB_GETTEXT, LB_GETTEXTLEN, LB_INSERTSTRING,
    LB_RESETCONTENT, LB_SETCURSEL, LBS_HASSTRINGS, LBS_NOINTEGRALHEIGHT, LBS_NOTIFY,
    LBS_OWNERDRAWFIXED, MB_ICONWARNING, MB_OK, MessageBoxW, SPI_GETWORKAREA, SWP_NOSIZE,
    SWP_NOZORDER, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SendMessageW, SetWindowPos, SetWindowTextW,
    SystemParametersInfoW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CAPTURECHANGED, WM_CLOSE, WM_COMMAND,
    WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC, WM_DRAWITEM, WM_ENABLE, WM_ERASEBKGND,
    WM_GETDLGCODE, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MEASUREITEM,
    WM_MOUSEMOVE, WM_NCDESTROY, WM_PAINT, WM_SETFOCUS, WM_SETFONT, WS_CAPTION, WS_CHILD,
    WS_EX_APPWINDOW, WS_EX_CONTROLPARENT, WS_EX_DLGMODALFRAME, WS_OVERLAPPED, WS_SYSMENU,
    WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GWLP_HWNDPARENT, GetWindowLongPtrW, GetWindowRect, ICON_BIG, ICON_SMALL, IDYES,
    MB_YESNO, SetForegroundWindow, SetWindowLongPtrW, WM_GETICON, WM_SETICON, WS_EX_TOPMOST,
    WS_POPUP,
};
use windows::core::{HSTRING, PCWSTR, PWSTR, w};
use windows_numerics::Vector2;

use crate::Ui;
use crate::events::DialogId;
use crate::icon::{Icon, IconHandle};
use crate::native::{self, NativeWindow, state_of};
use crate::placement::{Area, Placement, SCREEN_MARGIN};
use crate::theme::{Theme, colorref, over, rgba};
use crate::ui::Notice;
use crate::window::Window;

mod paint;
mod picker;
mod types;

use paint::*;
use picker::*;
pub use types::*;

const CLASS_NAME: PCWSTR = w!("GlasskitDialog");

const PLACEMENT_CLASS: PCWSTR = w!("GlasskitPlacement");

const FIRST_ID: usize = 100;

const ID_OK: usize = 1;

const ID_CANCEL: usize = 2;

const SS_CENTERIMAGE: u32 = 0x200;

const BN_CLICKED: u32 = 0;

const SELCHANGE: u32 = 1;

const EN_CHANGE: u32 = 0x300;

/// The extra height of a closed drop-down list that holds its open list.
const DROP_DOWN_LIST_HEIGHT: i32 = 200;

const FONT_SIZE: f32 = 14.0;

const ICON_SIZE: f32 = 16.0;

const BUTTON_RADIUS: f32 = 6.0;

const CARD_RADIUS: f32 = 8.0;

const CHECK_BOX: f32 = 18.0;

const CHECK_RADIUS: f32 = 4.0;

const CHECK_GAP: f32 = 8.0;

const HEADING_INSET: i32 = 12;

const HEADING_TOP: i32 = 8;

const HEADING_HEIGHT: i32 = 20;

const FIELD_RADIUS: f32 = 4.0;

const FIELD_INSET_X: i32 = 8;

const FIELD_INSET_Y: i32 = 5;

const FIELD_LINE_HEIGHT: i32 = 20;

const LIST_INSET: i32 = 4;

const ITEM_HEIGHT: i32 = 24;

const ITEM_RADIUS: f32 = 4.0;

const ITEM_TEXT_INSET: f32 = 8.0;

const CHEVRON_WIDTH: f32 = 28.0;

/// The height of a closed drop-down at 100% display scale.
const DROP_DOWN_HEIGHT: i32 = 26;

/// Notification codes of edit boxes, lists, and drop-downs that change how a field looks.
const EN_SETFOCUS: u32 = 0x100;

const EN_KILLFOCUS: u32 = 0x200;

const LBN_SETFOCUS: u32 = 4;

const LBN_KILLFOCUS: u32 = 5;

const CBN_SETFOCUS: u32 = 3;

const CBN_KILLFOCUS: u32 = 4;

const CBN_DROPDOWN: u32 = 7;

const CBN_CLOSEUP: u32 = 8;

/// The faces to try, the first that Windows has is used. Segoe UI Variable is the popup font.
const FONT_FACES: [PCWSTR; 2] = [w!("Segoe UI Variable Text"), w!("Segoe UI")];

#[derive(Debug, Clone, Copy)]
enum Control<'a> {
    Label(&'a str),
    /// A card with a heading. Add it before the controls that it holds.
    Group(&'a str),
    Edit {
        multiline: bool,
        digits_only: bool,
    },
    DropDown,
    List,
    Check(&'a str),
    Button(&'a str),
    /// A button that shows a character of the icon font. Its name shows as a tip.
    IconButton {
        icon: &'a str,
        name: &'a str,
    },
    /// A picker for where a window shows on the primary monitor: a small copy of the work area
    /// with the window in it. The user drags the window, clicks a place, or uses the arrow keys.
    /// `window_width` and `window_height` are the window size at 100% display scale.
    Placement {
        window_width: i32,
        window_height: i32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Static,
    Edit,
    DropDown,
    List,
    Check,
    Button,
    Placement,
}

/// What is behind a control: the window or a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Surface {
    Window,
    Card,
}

/// What the dialog keeps for one control.
struct ControlState {
    kind: Kind,
    /// What is behind the control: the window or a card.
    surface: Surface,
    checked: bool,
    /// The character that an icon button shows.
    glyph: Option<Vec<u16>>,
    /// The field that the dialog paints around the control, in client pixels.
    field: Option<RECT>,
    /// The placement that a placement picker shows.
    placement: Placement,
    /// The window size that a placement picker shows, at 100% scale.
    window_size: (i32, i32),
}

/// The window procedure only puts events in the queue. [`Dialog::run`] gives them to the handler
/// outside the window procedure, so the handler can change the controls safely.
struct Shared {
    id: u32,
    controls: RefCell<Vec<ControlState>>,
    /// The cards in client pixels.
    cards: RefCell<Vec<RECT>>,
    /// While the user drags in a placement picker: the grab point in the window, in pixels.
    grab: Cell<Option<(f32, f32)>>,
    hover: Cell<Option<usize>>,
    events: RefCell<VecDeque<Event>>,
    muted: Cell<bool>,
    look: Look,
}

impl Shared {
    /// Queue an event for the app, and wake the event loop.
    fn push(&self, event: Event) {
        self.events.borrow_mut().push_back(event);
        crate::ui::notify(Notice::Dialog(self.id));
    }

    fn index_of(&self, child: HWND) -> Option<usize> {
        let id = unsafe { GetDlgCtrlID(child) } as usize;
        let index = id.checked_sub(FIRST_ID)?;
        (index < self.controls.borrow().len()).then_some(index)
    }
}

/// A modal window with controls that the app places.
///
/// Make it with [`Dialog::new`], add the controls with the `add_` methods, then call
/// [`Dialog::run`]. The window closes when the `Dialog` is dropped.
pub struct Dialog {
    ui: Ui,
    id: DialogId,
    shown: bool,
    /// Declared before the fonts, so the window is gone before its fonts.
    window: NativeWindow<Shared>,
    icon: Option<IconHandle>,
    /// The window that the dialog belongs to.
    owner: Option<HWND>,
    /// The window that shows the tips of the icon buttons.
    tips: Option<HWND>,
    font: Font,
    heading_font: Font,
    scale: f32,
    controls: Vec<HWND>,
}

/// A font that Windows deletes when the value is dropped.
struct Font(HFONT);

impl Drop for Font {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(self.0.0));
        }
    }
}

impl Dialog {
    /// `width` and `height` are the client size at 100% display scale.
    /// The colors follow the Windows light or dark app mode, as the popup does.
    pub fn new(ui: &Ui, title: &str, width: i32, height: i32) -> Result<Self> {
        let hinstance = ui.hinstance();
        unsafe {
            native::register_class(CLASS_NAME, Some(wndproc), hinstance, IDC_ARROW)?;
            native::register_class(
                PLACEMENT_CLASS,
                Some(placement_proc),
                hinstance,
                IDC_SIZEALL,
            )?;

            let dpi = crate::primary_monitor_dpi();
            let scale = dpi as f32 / 96.0;
            let theme = Theme::current();
            let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU;
            let ex_style = WS_EX_DLGMODALFRAME | WS_EX_CONTROLPARENT | WS_EX_APPWINDOW;
            let mut frame = RECT {
                left: 0,
                top: 0,
                right: scaled(width, scale),
                bottom: scaled(height, scale),
            };
            AdjustWindowRectExForDpi(&mut frame, style, false, ex_style, dpi)
                .call("AdjustWindowRectExForDpi")?;
            let (outer_w, outer_h) = (frame.right - frame.left, frame.bottom - frame.top);

            let hwnd = CreateWindowExW(
                ex_style,
                CLASS_NAME,
                &HSTRING::from(title),
                style,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                outer_w,
                outer_h,
                None,
                None,
                Some(hinstance),
                None,
            )
            .call("CreateWindowExW(dialog)")?;

            let look = Look::new(theme, scale);
            native::set_dwm_attribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                i32::from(theme.dark) as u32,
            );
            native::set_dwm_attribute(hwnd, DWMWA_CAPTION_COLOR, colorref(look.window).0);
            native::set_dwm_attribute(hwnd, DWMWA_TEXT_COLOR, look.text().0);

            let id = ui.new_id();
            let window = NativeWindow::attach(
                hwnd,
                Rc::new(Shared {
                    controls: RefCell::new(Vec::new()),
                    cards: RefCell::new(Vec::new()),
                    grab: Cell::new(None),
                    hover: Cell::new(None),
                    id,
                    events: RefCell::new(VecDeque::new()),
                    muted: Cell::new(false),
                    look,
                }),
            );

            let mut work = RECT::default();
            if SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(&mut work as *mut _ as *mut c_void),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
            .is_ok()
            {
                let x = work.left + ((work.right - work.left - outer_w) / 2).max(0);
                let y = work.top + ((work.bottom - work.top - outer_h) / 2).max(0);
                let _ = SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
            }

            Ok(Self {
                ui: ui.clone(),
                id: DialogId(id),
                shown: false,
                window,
                icon: None,
                owner: None,
                tips: None,
                font: Font(ui_font(scale, FW_NORMAL.0 as i32)),
                heading_font: Font(ui_font(scale, FW_SEMIBOLD.0 as i32)),
                scale,
                controls: Vec::new(),
            })
        }
    }

    fn shared(&self) -> &Shared {
        self.window.state()
    }

    fn add(&mut self, control: Control, bounds: Bounds) -> Result<usize> {
        let s = |v: i32| scaled(v, self.scale);
        let bounds = match control {
            Control::Group(_) => {
                self.shared().cards.borrow_mut().push(RECT {
                    left: s(bounds.x),
                    top: s(bounds.y),
                    right: s(bounds.x + bounds.width),
                    bottom: s(bounds.y + bounds.height),
                });
                Bounds::new(
                    bounds.x + HEADING_INSET,
                    bounds.y + HEADING_TOP,
                    bounds.width - 2 * HEADING_INSET,
                    HEADING_HEIGHT,
                )
            }
            _ => bounds,
        };
        let child = WS_CHILD | WS_VISIBLE;
        let owner_drawn = child | WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32);
        let (class, text, kind, style, ex_style) = match control {
            Control::Label(text) | Control::Group(text) => (
                w!("STATIC"),
                text,
                Kind::Static,
                child | WINDOW_STYLE(SS_CENTERIMAGE),
                WINDOW_EX_STYLE(0),
            ),
            Control::Edit {
                multiline,
                digits_only,
            } => {
                let mut style = child | WS_TABSTOP;
                if multiline {
                    style |= WS_VSCROLL
                        | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32);
                } else {
                    style |= WINDOW_STYLE(ES_AUTOHSCROLL as u32);
                }
                if digits_only {
                    style |= WINDOW_STYLE(ES_NUMBER as u32);
                }
                (w!("EDIT"), "", Kind::Edit, style, WINDOW_EX_STYLE(0))
            }
            Control::DropDown => (
                w!("COMBOBOX"),
                "",
                Kind::DropDown,
                child
                    | WS_TABSTOP
                    | WS_VSCROLL
                    | WINDOW_STYLE((CBS_DROPDOWNLIST | CBS_OWNERDRAWFIXED | CBS_HASSTRINGS) as u32),
                WINDOW_EX_STYLE(0),
            ),
            Control::List => (
                w!("LISTBOX"),
                "",
                Kind::List,
                child
                    | WS_TABSTOP
                    | WS_VSCROLL
                    | WINDOW_STYLE(
                        (LBS_NOTIFY | LBS_NOINTEGRALHEIGHT | LBS_OWNERDRAWFIXED | LBS_HASSTRINGS)
                            as u32,
                    ),
                WINDOW_EX_STYLE(0),
            ),
            Control::Check(text) => (
                w!("BUTTON"),
                text,
                Kind::Check,
                owner_drawn,
                WINDOW_EX_STYLE(0),
            ),
            // The default button is not marked: Enter gives Event::Submit in any case.
            Control::Button(text) => (
                w!("BUTTON"),
                text,
                Kind::Button,
                owner_drawn,
                WINDOW_EX_STYLE(0),
            ),
            Control::IconButton { name, .. } => (
                w!("BUTTON"),
                name,
                Kind::Button,
                owner_drawn,
                WINDOW_EX_STYLE(0),
            ),
            Control::Placement { .. } => (
                PLACEMENT_CLASS,
                "",
                Kind::Placement,
                child | WS_TABSTOP,
                WINDOW_EX_STYLE(0),
            ),
        };
        let rect = RECT {
            left: s(bounds.x),
            top: s(bounds.y),
            right: s(bounds.x + bounds.width),
            bottom: s(bounds.y + bounds.height),
        };
        let surface = if self
            .shared()
            .cards
            .borrow()
            .iter()
            .any(|c| contains(c, &rect))
        {
            Surface::Card
        } else {
            Surface::Window
        };
        // An edit box or a list sits without a frame in a field that the dialog paints.
        let inner = match (control, kind) {
            (
                Control::Edit {
                    multiline: false, ..
                },
                _,
            ) => {
                let line = s(FIELD_LINE_HEIGHT);
                let top = rect.top + ((rect.bottom - rect.top - line) / 2).max(0);
                RECT {
                    left: rect.left + s(FIELD_INSET_X),
                    top,
                    right: rect.right - s(FIELD_INSET_X),
                    bottom: top + line,
                }
            }
            (_, Kind::Edit) => inset(&rect, s(FIELD_INSET_X), s(FIELD_INSET_Y)),
            (_, Kind::List) => inset(&rect, s(LIST_INSET), s(LIST_INSET)),
            _ => rect,
        };
        let height = match kind {
            Kind::DropDown => s(bounds.height) + s(DROP_DOWN_LIST_HEIGHT),
            _ => inner.bottom - inner.top,
        };
        let index = self.controls.len();
        let id = FIRST_ID + index;
        self.shared().controls.borrow_mut().push(ControlState {
            kind,
            surface,
            checked: false,
            glyph: match control {
                Control::IconButton { icon, .. } => Some(icon.encode_utf16().collect()),
                _ => None,
            },
            field: matches!(kind, Kind::Edit | Kind::List).then_some(rect),
            placement: Placement::default(),
            window_size: match control {
                Control::Placement {
                    window_width,
                    window_height,
                } => (window_width, window_height),
                _ => (0, 0),
            },
        });
        let hwnd = unsafe {
            CreateWindowExW(
                ex_style,
                class,
                &HSTRING::from(text),
                style,
                inner.left,
                inner.top,
                inner.right - inner.left,
                height,
                Some(self.window.hwnd()),
                Some(HMENU(id as *mut c_void)),
                None,
                None,
            )
        };
        let hwnd = match hwnd {
            Ok(hwnd) => hwnd,
            Err(err) => {
                self.shared().controls.borrow_mut().pop();
                return Err(err).call("CreateWindowExW(control)");
            }
        };
        let font = match control {
            Control::Group(_) => self.heading_font.0,
            _ => self.font.0,
        };
        unsafe {
            SendMessageW(
                hwnd,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
            if self.shared().look.theme.dark
                && matches!(kind, Kind::Edit | Kind::List | Kind::DropDown)
            {
                let _ = SetWindowTheme(hwnd, w!("DarkMode_Explorer"), PCWSTR::null());
            }
            if matches!(kind, Kind::Button | Kind::Check | Kind::DropDown) {
                let shared = self.shared() as *const Shared as usize;
                let _ = SetWindowSubclass(hwnd, Some(themed_control), index, shared);
            }
        }
        self.controls.push(hwnd);
        if let Control::IconButton { name, .. } = control {
            self.add_tip(hwnd, name);
        }
        Ok(index)
    }

    /// Show `text` when the pointer stays on a control. A failure gives no tip.
    fn add_tip(&mut self, control: HWND, text: &str) {
        let dialog = self.window.hwnd();
        unsafe {
            let tips = match self.tips {
                Some(tips) => tips,
                None => {
                    let Ok(tips) = CreateWindowExW(
                        WS_EX_TOPMOST,
                        TOOLTIPS_CLASSW,
                        PCWSTR::null(),
                        WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
                        CW_USEDEFAULT,
                        CW_USEDEFAULT,
                        CW_USEDEFAULT,
                        CW_USEDEFAULT,
                        Some(dialog),
                        None,
                        Some(self.ui.hinstance()),
                        None,
                    ) else {
                        return;
                    };
                    if self.shared().look.theme.dark {
                        let _ = SetWindowTheme(tips, w!("DarkMode_Explorer"), PCWSTR::null());
                    }
                    self.tips = Some(tips);
                    tips
                }
            };
            let mut wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
            // The size without the last field is right for all versions of the controls.
            let tool = TTTOOLINFOW {
                cbSize: (size_of::<TTTOOLINFOW>() - size_of::<*mut c_void>()) as u32,
                uFlags: TTF_IDISHWND | TTF_SUBCLASS,
                hwnd: dialog,
                uId: control.0 as usize,
                lpszText: PWSTR(wide.as_mut_ptr()),
                ..Default::default()
            };
            SendMessageW(
                tips,
                TTM_ADDTOOLW,
                None,
                Some(LPARAM(&tool as *const TTTOOLINFOW as isize)),
            );
        }
    }

    /// Make the dialog belong to a window. The dialog then stays above the window, shows
    /// in its middle, has its icon, and has no button of its own on the taskbar. While
    /// [`Dialog::run`] runs, the window takes no input. Call it before the dialog shows.
    pub fn set_owner(&mut self, window: &Window) {
        let (hwnd, owner) = (self.window.hwnd(), window.hwnd());
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, owner.0 as isize);
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style & !(WS_EX_APPWINDOW.0 as isize));
            if self.icon.is_none() {
                for kind in [ICON_SMALL, ICON_BIG] {
                    let icon = SendMessageW(owner, WM_GETICON, Some(WPARAM(kind as usize)), None);
                    SendMessageW(
                        hwnd,
                        WM_SETICON,
                        Some(WPARAM(kind as usize)),
                        Some(LPARAM(icon.0)),
                    );
                }
            }
            let (mut over, mut own) = (RECT::default(), RECT::default());
            let mut info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let monitor = MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST);
            if GetWindowRect(owner, &mut over).is_ok()
                && GetWindowRect(hwnd, &mut own).is_ok()
                && GetMonitorInfoW(monitor, &mut info).as_bool()
            {
                let (w, h) = (own.right - own.left, own.bottom - own.top);
                let work = info.rcWork;
                let x = (over.left + over.right - w) / 2;
                let y = (over.top + over.bottom - h) / 2;
                let x = x.min(work.right - w).max(work.left);
                let y = y.min(work.bottom - h).max(work.top);
                let _ = SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
            }
        }
        self.owner = Some(owner);
    }

    /// Show this picture in the title bar, the taskbar, and Alt+Tab.
    pub fn set_icon(&mut self, icon: &Icon) -> Result<()> {
        let handle = IconHandle::new(icon)?;
        unsafe {
            for kind in [ICON_SMALL, ICON_BIG] {
                SendMessageW(
                    self.window.hwnd(),
                    WM_SETICON,
                    Some(WPARAM(kind as usize)),
                    Some(LPARAM(handle.0.0 as isize)),
                );
            }
        }
        self.icon = Some(handle);
        Ok(())
    }

    /// Add a text that the user cannot change.
    pub fn add_label(&mut self, text: &str, bounds: Bounds) -> Result<Id<kind::Label>> {
        self.add(Control::Label(text), bounds).map(Id::new)
    }

    /// Add a card with a heading. Add it before the controls that go on it, and make it
    /// contain their bounds: the controls then take the card color.
    pub fn add_group(&mut self, heading: &str, bounds: Bounds) -> Result<Id<kind::Group>> {
        self.add(Control::Group(heading), bounds).map(Id::new)
    }

    /// Add an empty edit box.
    pub fn add_edit(&mut self, style: EditStyle, bounds: Bounds) -> Result<Id<kind::Edit>> {
        self.add(
            Control::Edit {
                multiline: style.multiline,
                digits_only: style.digits_only,
            },
            bounds,
        )
        .map(Id::new)
    }

    /// Add an empty drop-down list. `bounds` is the size of the closed list.
    pub fn add_drop_down(&mut self, bounds: Bounds) -> Result<Id<kind::DropDown>> {
        self.add(Control::DropDown, bounds).map(Id::new)
    }

    /// Add an empty list.
    pub fn add_list(&mut self, bounds: Bounds) -> Result<Id<kind::List>> {
        self.add(Control::List, bounds).map(Id::new)
    }

    /// Add a check box that is not checked.
    pub fn add_check(&mut self, text: &str, bounds: Bounds) -> Result<Id<kind::Check>> {
        self.add(Control::Check(text), bounds).map(Id::new)
    }

    /// Add a push button. A click gives [`Event::Clicked`].
    pub fn add_button(&mut self, text: &str, bounds: Bounds) -> Result<Id<kind::Button>> {
        self.add(Control::Button(text), bounds).map(Id::new)
    }

    /// Add a push button that shows an icon: a character of the icon font of Windows, Segoe
    /// Fluent Icons, such as `"\u{E74D}"` for a bin. `name` tells what the button does: it
    /// shows as a tip when the pointer stays on the button.
    pub fn add_icon_button(
        &mut self,
        icon: &str,
        name: &str,
        bounds: Bounds,
    ) -> Result<Id<kind::Button>> {
        self.add(Control::IconButton { icon, name }, bounds)
            .map(Id::new)
    }

    /// Add a picker for where a window shows on the primary monitor: a small copy of the
    /// work area with the window in it. The user drags the window, clicks a place, or uses
    /// the arrow keys. `window_width` and `window_height` are the window size at 100% display
    /// scale. A change gives [`Event::Changed`].
    pub fn add_placement_picker(
        &mut self,
        window_width: i32,
        window_height: i32,
        bounds: Bounds,
    ) -> Result<Id<kind::PlacementPicker>> {
        self.add(
            Control::Placement {
                window_width,
                window_height,
            },
            bounds,
        )
        .map(Id::new)
    }

    fn control(&self, index: usize) -> HWND {
        self.controls[index]
    }

    fn kind(&self, index: usize) -> Kind {
        self.shared().controls.borrow()[index].kind
    }

    fn send(&self, index: usize, msg: u32, wparam: usize, lparam: isize) -> isize {
        unsafe {
            SendMessageW(
                self.control(index),
                msg,
                Some(WPARAM(wparam)),
                Some(LPARAM(lparam)),
            )
            .0
        }
    }

    /// Run `change` without [`Event::Changed`] events.
    fn quietly<T>(&self, change: impl FnOnce() -> T) -> T {
        self.shared().muted.set(true);
        let result = change();
        self.shared().muted.set(false);
        result
    }

    /// The text of a label or an edit box.
    pub fn text<K: HasText>(&self, id: Id<K>) -> String {
        unsafe {
            let hwnd = self.control(id.index);
            let len = GetWindowTextLengthW(hwnd).max(0) as usize;
            let mut buf = vec![0u16; len + 1];
            let copied = GetWindowTextW(hwnd, &mut buf).max(0) as usize;
            String::from_utf16_lossy(&buf[..copied])
        }
    }

    /// Change the text of a label or an edit box. It does not give [`Event::Changed`].
    /// A multiline edit box needs `"\r\n"` between lines.
    pub fn set_text<K: HasText>(&self, id: Id<K>, text: &str) {
        self.quietly(|| unsafe {
            let _ = SetWindowTextW(self.control(id.index), &HSTRING::from(text));
        });
    }

    /// Replace all items of a list or a drop-down. Then no item is selected.
    pub fn set_items<K: HasItems, S: AsRef<str>>(&self, id: Id<K>, items: &[S]) {
        let (reset, add) = match self.kind(id.index) {
            Kind::DropDown => (CB_RESETCONTENT, CB_ADDSTRING),
            _ => (LB_RESETCONTENT, LB_ADDSTRING),
        };
        self.quietly(|| {
            self.send(id.index, reset, 0, 0);
            for item in items {
                let text = HSTRING::from(item.as_ref());
                self.send(id.index, add, 0, text.as_ptr() as isize);
            }
        });
    }

    /// Replace the text of one item of a list, and keep the selection.
    pub fn set_item(&self, id: Id<kind::List>, index: usize, text: &str) {
        let selected = self.selected(id);
        let text = HSTRING::from(text);
        self.quietly(|| {
            self.send(id.index, LB_DELETESTRING, index, 0);
            self.send(id.index, LB_INSERTSTRING, index, text.as_ptr() as isize);
        });
        self.set_selected(id, selected);
    }

    /// The selected item of a list or a drop-down, if one is selected.
    pub fn selected<K: HasItems>(&self, id: Id<K>) -> Option<usize> {
        let get = match self.kind(id.index) {
            Kind::DropDown => CB_GETCURSEL,
            _ => LB_GETCURSEL,
        };
        usize::try_from(self.send(id.index, get, 0, 0)).ok()
    }

    /// Select an item of a list or a drop-down, or none. It does not give [`Event::Changed`].
    pub fn set_selected<K: HasItems>(&self, id: Id<K>, index: Option<usize>) {
        let set = match self.kind(id.index) {
            Kind::DropDown => CB_SETCURSEL,
            _ => LB_SETCURSEL,
        };
        self.quietly(|| self.send(id.index, set, index.unwrap_or(usize::MAX), 0));
    }

    /// Whether a check box is checked.
    pub fn checked(&self, id: Id<kind::Check>) -> bool {
        self.shared().controls.borrow()[id.index].checked
    }

    /// Check or clear a check box. It does not give [`Event::Clicked`].
    pub fn set_checked(&self, id: Id<kind::Check>, checked: bool) {
        self.shared().controls.borrow_mut()[id.index].checked = checked;
        unsafe {
            let _ = InvalidateRect(Some(self.control(id.index)), None, false);
        }
    }

    /// The place that a placement picker shows.
    pub fn placement(&self, id: Id<kind::PlacementPicker>) -> Placement {
        self.shared().controls.borrow()[id.index].placement
    }

    /// Change the place that a placement picker shows. It does not give [`Event::Changed`].
    pub fn set_placement(&self, id: Id<kind::PlacementPicker>, placement: Placement) {
        self.shared().controls.borrow_mut()[id.index].placement = placement;
        unsafe {
            let _ = InvalidateRect(Some(self.control(id.index)), None, false);
        }
    }

    /// Let the user use a control, or show it in gray and ignore the user.
    pub fn set_enabled(&self, id: impl Into<AnyId>, enabled: bool) {
        unsafe {
            let _ = EnableWindow(self.control(id.into().0), enabled);
        }
    }

    /// Move the keyboard focus to a control.
    pub fn focus(&self, id: impl Into<AnyId>) {
        unsafe {
            let _ = SetFocus(Some(self.control(id.into().0)));
        }
    }

    /// Show a message that the user must close before the dialog continues.
    pub fn warn(&self, title: &str, message: &str) {
        unsafe {
            MessageBoxW(
                Some(self.window.hwnd()),
                &HSTRING::from(message),
                &HSTRING::from(title),
                MB_OK | MB_ICONWARNING,
            );
        }
    }

    /// Ask a question that the user must answer with Yes or No before the dialog continues.
    /// It gives true for Yes.
    pub fn confirm(&self, title: &str, message: &str) -> bool {
        unsafe {
            MessageBoxW(
                Some(self.window.hwnd()),
                &HSTRING::from(message),
                &HSTRING::from(title),
                MB_YESNO | MB_ICONWARNING,
            ) == IDYES
        }
    }

    /// The id that [`Event::Dialog`](crate::events::Event::Dialog) gives when this dialog has
    /// events.
    pub fn id(&self) -> DialogId {
        self.id
    }

    /// Show the dialog and return at once. When the user does something, the event loop gives
    /// [`Event::Dialog`](crate::events::Event::Dialog) with [`Dialog::id`]; then call
    /// [`Dialog::next_event`] until it gives `None`. The dialog stays until it is dropped, and
    /// other windows go on working while it shows.
    pub fn show(&mut self) {
        if self.shown {
            return;
        }
        self.shown = true;
        self.ui.add_dialog(self.window.hwnd());
        native::show_window(self.window.hwnd());
        if let Some(first) =
            (0..self.controls.len()).find(|&index| self.kind(index) != Kind::Static)
        {
            self.focus(AnyId(first));
        }
    }

    /// The next thing that the user did, or `None` when there is nothing more now.
    pub fn next_event(&mut self) -> Option<Event> {
        self.shared().events.borrow_mut().pop_front()
    }

    /// Show the dialog and give each event to `handler` until it returns [`Flow::Close`], with a
    /// message loop of its own. It is for an app without an
    /// [`EventLoop`](crate::events::EventLoop); with one, use [`Dialog::show`].
    ///
    /// A dialog with an [owner](Dialog::set_owner) is modal: the owner takes no input until
    /// this call returns.
    pub fn run(&mut self, mut handler: impl FnMut(&mut Dialog, Event) -> Flow) -> Result<Outcome> {
        let _modal = Modal::new(self.owner);
        self.show();
        loop {
            while let Some(event) = self.next_event() {
                if handler(self, event) == Flow::Close {
                    return Ok(Outcome::Closed);
                }
            }
            if !self.ui.pump_wait()? {
                return Ok(Outcome::Quit);
            }
        }
    }
}

/// Takes the input away from the owner of a dialog, and gives it back when dropped.
struct Modal(Option<HWND>);

impl Modal {
    fn new(owner: Option<HWND>) -> Self {
        // An owner that was off before stays off after.
        Self(owner.filter(|&owner| unsafe { !EnableWindow(owner, false).as_bool() }))
    }
}

impl Drop for Modal {
    fn drop(&mut self) {
        if let Some(owner) = self.0 {
            unsafe {
                let _ = EnableWindow(owner, true);
                let _ = SetForegroundWindow(owner);
            }
        }
    }
}

impl Drop for Dialog {
    fn drop(&mut self) {
        self.ui.remove_dialog(self.window.hwnd());
    }
}

fn scaled(value: i32, scale: f32) -> i32 {
    (value as f32 * scale).round() as i32
}

fn contains(outer: &RECT, inner: &RECT) -> bool {
    inner.left >= outer.left
        && inner.top >= outer.top
        && inner.right <= outer.right
        && inner.bottom <= outer.bottom
}

/// The popup font at 14 px, or Segoe UI if Windows has no other face.
fn ui_font(scale: f32, weight: i32) -> HFONT {
    let height = -(FONT_SIZE * scale).round() as i32;
    for face in FONT_FACES {
        let font = unsafe {
            CreateFontW(
                height,
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                0,
                face,
            )
        };
        if font.is_invalid() || has_face(font, face) {
            return font;
        }
        unsafe {
            let _ = DeleteObject(HGDIOBJ(font.0));
        }
    }
    HFONT::default()
}

/// Windows gives a different font if it does not have the face that was asked for.
fn has_face(font: HFONT, face: PCWSTR) -> bool {
    unsafe {
        let hdc = GetDC(None);
        let old = SelectObject(hdc, HGDIOBJ(font.0));
        let mut buf = [0u16; 64];
        let len = GetTextFaceW(hdc, Some(&mut buf)).max(0) as usize;
        SelectObject(hdc, old);
        ReleaseDC(None, hdc);
        let wanted = face.as_wide();
        len > 0
            && buf[..len.min(buf.len())]
                .strip_suffix(&[0])
                .unwrap_or(&buf[..len])
                == wanted
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let Some(shared) = state_of::<Shared>(hwnd) else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };
        match msg {
            WM_ERASEBKGND => {
                paint_background(shared, hwnd, HDC(wparam.0 as *mut c_void));
                LRESULT(1)
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => control_colors(
                shared,
                msg,
                HDC(wparam.0 as *mut c_void),
                HWND(lparam.0 as *mut c_void),
            ),
            WM_DRAWITEM => {
                let item = &*(lparam.0 as *const DRAWITEMSTRUCT);
                match item.CtlType {
                    ODT_BUTTON => paint_control(shared, item),
                    ODT_LISTBOX | ODT_COMBOBOX => paint_item(shared, item),
                    _ => {}
                }
                LRESULT(1)
            }
            WM_MEASUREITEM => {
                let item = &mut *(lparam.0 as *mut MEASUREITEMSTRUCT);
                let s = |v: i32| scaled(v, shared.look.scale);
                // The closed drop-down is the selection field and a 3-pixel frame on each side.
                item.itemHeight = if item.CtlType == ODT_COMBOBOX && item.itemID == u32::MAX {
                    (s(DROP_DOWN_HEIGHT) - 6).max(1) as u32
                } else {
                    s(ITEM_HEIGHT) as u32
                };
                LRESULT(1)
            }
            WM_LBUTTONDOWN => {
                // A click on the padding of a field gives the focus to the control in it.
                let (x, y) = native::point(lparam);
                let hit = shared.controls.borrow().iter().position(|c| {
                    c.field
                        .is_some_and(|f| x >= f.left && x < f.right && y >= f.top && y < f.bottom)
                });
                if let Some(index) = hit
                    && let Ok(child) = GetDlgItem(Some(hwnd), (FIRST_ID + index) as i32)
                {
                    let _ = SetFocus(Some(child));
                }
                LRESULT(0)
            }
            WM_COMMAND => {
                let id = wparam.0 & 0xFFFF;
                let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
                if id >= FIRST_ID {
                    let index = id - FIRST_ID;
                    let kind = shared.controls.borrow().get(index).map(|c| c.kind);
                    match (kind, code) {
                        (Some(Kind::Edit), EN_SETFOCUS | EN_KILLFOCUS)
                        | (Some(Kind::List), LBN_SETFOCUS | LBN_KILLFOCUS) => {
                            let field = shared.controls.borrow().get(index).and_then(|c| c.field);
                            if let Some(field) = field {
                                let _ = InvalidateRect(Some(hwnd), Some(&field), true);
                            }
                        }
                        (
                            Some(Kind::DropDown),
                            CBN_SETFOCUS | CBN_KILLFOCUS | CBN_DROPDOWN | CBN_CLOSEUP,
                        ) => {
                            let _ =
                                InvalidateRect(Some(HWND(lparam.0 as *mut c_void)), None, false);
                        }
                        _ => {}
                    }
                }
                let event = match id {
                    ID_OK => Some(Event::Submit),
                    ID_CANCEL => Some(Event::Close),
                    _ if id >= FIRST_ID => {
                        let index = id - FIRST_ID;
                        let kind = shared.controls.borrow().get(index).map(|c| c.kind);
                        let control = AnyId(index);
                        match (kind, code) {
                            (Some(Kind::Check), BN_CLICKED) => {
                                if let Some(c) = shared.controls.borrow_mut().get_mut(index) {
                                    c.checked = !c.checked;
                                }
                                let _ = InvalidateRect(
                                    Some(HWND(lparam.0 as *mut c_void)),
                                    None,
                                    false,
                                );
                                Some(Event::Clicked(control))
                            }
                            (Some(Kind::Button), BN_CLICKED) => Some(Event::Clicked(control)),
                            (Some(Kind::Edit), EN_CHANGE)
                            | (Some(Kind::DropDown | Kind::List), SELCHANGE)
                                if !shared.muted.get() =>
                            {
                                Some(Event::Changed(control))
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                };
                if let Some(event) = event {
                    shared.push(event);
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                shared.push(Event::Close);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn inset(r: &RECT, dx: i32, dy: i32) -> RECT {
    RECT {
        left: r.left + dx,
        top: r.top + dy,
        right: (r.right - dx).max(r.left + dx),
        bottom: (r.bottom - dy).max(r.top + dy),
    }
}

/// The work area of the primary monitor, in pixels.
fn work_area() -> RECT {
    let mut work = RECT::default();
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut work as *mut _ as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if read.is_err() || work.right <= work.left || work.bottom <= work.top {
        return RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
    }
    work
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_moves_near_values_to_edges_and_center() {
        assert_eq!(snap(0.02), 0.0);
        assert_eq!(snap(0.47), 0.5);
        assert_eq!(snap(0.97), 1.0);
        assert_eq!(snap(0.3), 0.3);
        assert_eq!(snap(0.55), 0.55);
    }
}
