// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! A small modal window built from standard Windows controls, in the same colors as the popup.
//! The app places the controls and handles the events. `glasskit` knows nothing about the content.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::fmt;
use std::marker::PhantomData;

use crate::error::{Call, Error, Result};
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
    DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWINDOWATTRIBUTE,
    DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush,
    DEFAULT_CHARSET, DeleteObject, EndPaint, FW_NORMAL, FW_SEMIBOLD, FillRect, GetDC, GetTextFaceW,
    HBRUSH, HDC, HFONT, HGDIOBJ, InvalidateRect, OUT_DEFAULT_PRECIS, PAINTSTRUCT, ReleaseDC,
    SelectObject, SetBkColor, SetTextColor,
};
use windows::Win32::UI::Controls::{
    DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_COMBOBOXEDIT, ODS_DISABLED, ODS_FOCUS, ODS_NOFOCUSRECT,
    ODS_SELECTED, ODT_BUTTON, ODT_COMBOBOX, ODT_LISTBOX, SetWindowTheme, WM_MOUSELEAVE,
};
use windows::Win32::UI::HiDpi::AdjustWindowRectExForDpi;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, GetFocus, IsWindowEnabled, ReleaseCapture, SetCapture, SetFocus, TME_LEAVE,
    TRACKMOUSEEVENT, TrackMouseEvent, VIRTUAL_KEY, VK_DOWN, VK_LEFT, VK_RIGHT, VK_UP,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    BS_OWNERDRAW, CB_ADDSTRING, CB_GETCURSEL, CB_GETDROPPEDSTATE, CB_GETLBTEXT, CB_GETLBTEXTLEN,
    CB_RESETCONTENT, CB_SETCURSEL, CBS_DROPDOWNLIST, CBS_HASSTRINGS, CBS_OWNERDRAWFIXED,
    CW_USEDEFAULT, CreateWindowExW, DLGC_WANTARROWS, DefWindowProcW, DestroyWindow,
    DispatchMessageW, ES_AUTOHSCROLL, ES_AUTOVSCROLL, ES_MULTILINE, ES_NUMBER, ES_WANTRETURN,
    GWLP_USERDATA, GetClientRect, GetDlgCtrlID, GetDlgItem, GetMessageW, GetParent,
    GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, HMENU, IDC_ARROW, IDC_SIZEALL,
    LB_ADDSTRING, LB_DELETESTRING, LB_GETCURSEL, LB_GETTEXT, LB_GETTEXTLEN, LB_INSERTSTRING,
    LB_RESETCONTENT, LB_SETCURSEL, LBS_HASSTRINGS, LBS_NOINTEGRALHEIGHT, LBS_NOTIFY,
    LBS_OWNERDRAWFIXED, LoadCursorW, MB_ICONWARNING, MB_OK, MSG, MessageBoxW, PostQuitMessage,
    SPI_GETWORKAREA, SW_SHOWNORMAL, SWP_NOSIZE, SWP_NOZORDER, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow,
    SystemParametersInfoW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CAPTURECHANGED,
    WM_CLOSE, WM_COMMAND, WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC, WM_DRAWITEM,
    WM_ENABLE, WM_ERASEBKGND, WM_GETDLGCODE, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MEASUREITEM, WM_MOUSEMOVE, WM_NCDESTROY, WM_PAINT, WM_SETFOCUS, WM_SETFONT,
    WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_EX_APPWINDOW, WS_EX_CONTROLPARENT, WS_EX_DLGMODALFRAME,
    WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};
use windows::core::{HSTRING, PCWSTR, w};
use windows_numerics::Vector2;

use crate::Ui;
use crate::events::DialogId;
use crate::placement::{Area, Placement, SCREEN_MARGIN};
use crate::theme::{Theme, colorref, over, rgba};
use crate::ui::Notice;

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
const PICKER_LABEL_HEIGHT: f32 = 20.0;
const PICKER_SCREEN_RADIUS: f32 = 6.0;
const PICKER_WINDOW_RADIUS: f32 = 3.0;
/// A fraction this near to an edge or to the center snaps to it.
const PICKER_SNAP: f32 = 0.04;
const PICKER_KEY_STEP: f32 = 0.05;
/// The faces to try, the first that Windows has is used. Segoe UI Variable is the popup font.
const FONT_FACES: [PCWSTR; 2] = [w!("Segoe UI Variable Text"), w!("Segoe UI")];

/// A position and size in pixels at 100% display scale. The dialog scales them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    /// The left edge, from the left of the client area.
    pub x: i32,
    /// The top edge, from the top of the client area.
    pub y: i32,
    /// The width.
    pub width: i32,
    /// The height.
    pub height: i32,
}

impl Bounds {
    /// Bounds with this top-left corner and size.
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

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

/// The kinds of controls. They are the type parameter of [`Id`], and they decide which
/// [`Dialog`] methods an id can go to.
pub mod kind {
    /// A text that the user cannot change.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Label {}
    /// A card with a heading, behind the controls that are on it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Group {}
    /// A box for text, on one line or on many.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Edit {}
    /// A closed list that opens when the user clicks it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DropDown {}
    /// An open list of items.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum List {}
    /// A check box with a text.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Check {}
    /// A push button.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Button {}
    /// A picker for where a window shows on the screen. See [`Dialog::add_placement_picker`].
    ///
    /// [`Dialog::add_placement_picker`]: super::Dialog::add_placement_picker
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PlacementPicker {}
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::kind::Label {}
    impl Sealed for super::kind::Edit {}
    impl Sealed for super::kind::DropDown {}
    impl Sealed for super::kind::List {}
}

/// The kinds of controls that show a text that the app can read and change.
pub trait HasText: sealed::Sealed {}
impl HasText for kind::Label {}
impl HasText for kind::Edit {}

/// The kinds of controls that hold a list of items, of which the user can select one.
pub trait HasItems: sealed::Sealed {}
impl HasItems for kind::DropDown {}
impl HasItems for kind::List {}

/// A control in a [`Dialog`]. The kind `K` decides which methods take it.
pub struct Id<K> {
    index: usize,
    kind: PhantomData<fn() -> K>,
}

impl<K> Id<K> {
    fn new(index: usize) -> Self {
        Self {
            index,
            kind: PhantomData,
        }
    }
}

impl<K> Clone for Id<K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K> Copy for Id<K> {}

impl<K> PartialEq for Id<K> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl<K> Eq for Id<K> {}

impl<K> std::hash::Hash for Id<K> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}

impl<K> fmt::Debug for Id<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self.index)
    }
}

/// A control of any kind, as an [`Event`] names it. Compare it with an [`Id`] of any kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnyId(usize);

impl<K> From<Id<K>> for AnyId {
    fn from(id: Id<K>) -> Self {
        AnyId(id.index)
    }
}

impl<K> PartialEq<Id<K>> for AnyId {
    fn eq(&self, other: &Id<K>) -> bool {
        self.0 == other.index
    }
}

impl<K> PartialEq<AnyId> for Id<K> {
    fn eq(&self, other: &AnyId) -> bool {
        self.index == other.0
    }
}

/// How an edit box takes text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditStyle {
    /// The box has many lines. Enter starts a new line, and a scroll bar shows when the text
    /// is longer than the box.
    pub multiline: bool,
    /// The box takes only the digits 0 to 9.
    pub digits_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// What the user did. [`Dialog::run`] gives events to its handler one at a time.
pub enum Event {
    /// A button or a check box was clicked.
    Clicked(AnyId),
    /// The user changed the text of an edit box or the selection of a list.
    /// Changes made through [`Dialog`] methods do not send this event.
    Changed(AnyId),
    /// The user pressed Enter outside a multiline edit box.
    Submit,
    /// The user pressed Esc or the close button.
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// What the handler of [`Dialog::run`] wants next.
pub enum Flow {
    /// Keep the dialog open and wait for the next event.
    Continue,
    /// Close the dialog. [`Dialog::run`] then returns [`Outcome::Closed`].
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// How [`Dialog::run`] ended.
pub enum Outcome {
    /// The handler returned [`Flow::Close`].
    Closed,
    /// The thread got WM_QUIT. The dialog posts WM_QUIT again for the caller.
    Quit,
}

/// The colors, brushes, and drawing objects of one dialog.
struct Look {
    theme: Theme,
    scale: f32,
    window: D2D1_COLOR_F,
    card: D2D1_COLOR_F,
    field: D2D1_COLOR_F,
    window_brush: HBRUSH,
    card_brush: HBRUSH,
    field_brush: HBRUSH,
    painter: Option<Painter>,
}

impl Look {
    fn new(theme: Theme, scale: f32) -> Self {
        let window = theme.background();
        let card = over(theme.button, window);
        let field = over(theme.field, card);
        let brush = |c| unsafe { CreateSolidBrush(colorref(c)) };
        Self {
            theme,
            scale,
            window,
            card,
            field,
            window_brush: brush(window),
            card_brush: brush(card),
            field_brush: brush(field),
            painter: Painter::new(scale).ok(),
        }
    }

    fn surface(&self, surface: Surface) -> (D2D1_COLOR_F, HBRUSH) {
        match surface {
            Surface::Window => (self.window, self.window_brush),
            Surface::Card => (self.card, self.card_brush),
        }
    }

    fn text(&self) -> COLORREF {
        colorref(over(self.theme.text, self.card))
    }

    fn dim_text(&self, behind: D2D1_COLOR_F) -> COLORREF {
        let t = self.theme.text;
        colorref(over(rgba(t.r, t.g, t.b, t.a * 0.45), behind))
    }
}

impl Drop for Look {
    fn drop(&mut self) {
        unsafe {
            for brush in [self.window_brush, self.card_brush, self.field_brush] {
                let _ = DeleteObject(HGDIOBJ(brush.0));
            }
        }
    }
}

/// Draws the cards and the owner-drawn buttons with Direct2D, so their edges are smooth.
struct Painter {
    target: ID2D1DCRenderTarget,
    centered: IDWriteTextFormat,
    leading: IDWriteTextFormat,
    _factory: ID2D1Factory,
}

impl Painter {
    fn new(scale: f32) -> windows::core::Result<Self> {
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let target = factory.CreateDCRenderTarget(&D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_IGNORE,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            })?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let format = |alignment| -> windows::core::Result<IDWriteTextFormat> {
                let format = dwrite.CreateTextFormat(
                    FONT_FACES[0],
                    None,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    FONT_SIZE * scale,
                    w!(""),
                )?;
                format.SetTextAlignment(alignment)?;
                format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
                // All drawn texts are one line. A long text ends with an ellipsis.
                format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
                let ellipsis = dwrite.CreateEllipsisTrimmingSign(&format)?;
                format.SetTrimming(
                    &DWRITE_TRIMMING {
                        granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                        delimiter: 0,
                        delimiterCount: 0,
                    },
                    &ellipsis,
                )?;
                Ok(format)
            };
            Ok(Self {
                target,
                centered: format(DWRITE_TEXT_ALIGNMENT_CENTER)?,
                leading: format(DWRITE_TEXT_ALIGNMENT_LEADING)?,
                _factory: factory,
            })
        }
    }

    fn paint(&self, hdc: HDC, rect: &RECT, draw: impl FnOnce(&ID2D1RenderTarget)) {
        unsafe {
            if self.target.BindDC(hdc, rect).is_err() {
                return;
            }
            self.target.BeginDraw();
            draw(&self.target);
            let _ = self.target.EndDraw(None, None);
        }
    }
}

fn rounded(left: f32, top: f32, right: f32, bottom: f32, radius: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left,
            top,
            right,
            bottom,
        },
        radiusX: radius,
        radiusY: radius,
    }
}

/// The window procedure only puts events in the queue. [`Dialog::run`] gives them to the handler
/// outside the window procedure, so the handler can change the controls safely.
struct Shared {
    id: u32,
    kinds: RefCell<Vec<Kind>>,
    surfaces: RefCell<Vec<Surface>>,
    checked: RefCell<Vec<bool>>,
    /// The cards in client pixels.
    cards: RefCell<Vec<RECT>>,
    /// For each control: the field that the dialog paints around it, in client pixels.
    fields: RefCell<Vec<Option<RECT>>>,
    /// For each control: the placement that a placement picker shows.
    placements: RefCell<Vec<Placement>>,
    /// For each control: the window size that a placement picker shows, at 100% scale.
    window_sizes: RefCell<Vec<(i32, i32)>>,
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
        (index < self.kinds.borrow().len()).then_some(index)
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
    hwnd: HWND,
    font: HFONT,
    heading_font: HFONT,
    scale: f32,
    controls: Vec<HWND>,
    shared: *const Shared,
}

impl Dialog {
    /// `width` and `height` are the client size at 100% display scale.
    /// The colors follow the Windows light or dark app mode, as the popup does.
    pub fn new(ui: &Ui, title: &str, width: i32, height: i32) -> Result<Self> {
        let hinstance = ui.hinstance();
        unsafe {
            crate::register_class(&WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wndproc),
                hInstance: hinstance,
                hCursor: LoadCursorW(None, IDC_ARROW).call("LoadCursorW")?,
                lpszClassName: CLASS_NAME,
                ..Default::default()
            })?;
            crate::register_class(&WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(placement_proc),
                hInstance: hinstance,
                hCursor: LoadCursorW(None, IDC_SIZEALL).call("LoadCursorW")?,
                lpszClassName: PLACEMENT_CLASS,
                ..Default::default()
            })?;

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
            set_dwm_attribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                i32::from(theme.dark) as u32,
            );
            set_dwm_attribute(hwnd, DWMWA_CAPTION_COLOR, colorref(look.window).0);
            set_dwm_attribute(hwnd, DWMWA_TEXT_COLOR, look.text().0);

            let id = ui.new_id();
            let shared = Box::into_raw(Box::new(Shared {
                kinds: RefCell::new(Vec::new()),
                surfaces: RefCell::new(Vec::new()),
                checked: RefCell::new(Vec::new()),
                cards: RefCell::new(Vec::new()),
                fields: RefCell::new(Vec::new()),
                placements: RefCell::new(Vec::new()),
                window_sizes: RefCell::new(Vec::new()),
                grab: Cell::new(None),
                hover: Cell::new(None),
                id,
                events: RefCell::new(VecDeque::new()),
                muted: Cell::new(false),
                look,
            }));
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, shared as isize);

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
                hwnd,
                font: ui_font(scale, FW_NORMAL.0 as i32),
                heading_font: ui_font(scale, FW_SEMIBOLD.0 as i32),
                scale,
                controls: Vec::new(),
                shared,
            })
        }
    }

    fn shared(&self) -> &Shared {
        unsafe { &*self.shared }
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
        {
            let shared = self.shared();
            shared.kinds.borrow_mut().push(kind);
            shared.surfaces.borrow_mut().push(surface);
            shared.checked.borrow_mut().push(false);
            shared.placements.borrow_mut().push(Placement::default());
            shared.window_sizes.borrow_mut().push(match control {
                Control::Placement {
                    window_width,
                    window_height,
                } => (window_width, window_height),
                _ => (0, 0),
            });
            shared
                .fields
                .borrow_mut()
                .push(matches!(kind, Kind::Edit | Kind::List).then_some(rect));
        }
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
                Some(self.hwnd),
                Some(HMENU(id as *mut c_void)),
                None,
                None,
            )
        };
        let hwnd = match hwnd {
            Ok(hwnd) => hwnd,
            Err(err) => {
                let shared = self.shared();
                shared.kinds.borrow_mut().pop();
                shared.surfaces.borrow_mut().pop();
                shared.checked.borrow_mut().pop();
                shared.fields.borrow_mut().pop();
                shared.placements.borrow_mut().pop();
                shared.window_sizes.borrow_mut().pop();
                return Err(err).call("CreateWindowExW(control)");
            }
        };
        let font = match control {
            Control::Group(_) => self.heading_font,
            _ => self.font,
        };
        unsafe {
            SendMessageW(
                hwnd,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
            // Dark scroll bars, and a dark open list of a drop-down.
            if self.shared().look.theme.dark
                && matches!(kind, Kind::Edit | Kind::List | Kind::DropDown)
            {
                let _ = SetWindowTheme(hwnd, w!("DarkMode_Explorer"), PCWSTR::null());
            }
            if matches!(kind, Kind::Button | Kind::Check | Kind::DropDown) {
                let _ = SetWindowSubclass(hwnd, Some(themed_control), index, self.shared as usize);
            }
        }
        self.controls.push(hwnd);
        Ok(index)
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
        self.shared().kinds.borrow()[index]
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
        self.shared().checked.borrow()[id.index]
    }

    /// Check or clear a check box. It does not give [`Event::Clicked`].
    pub fn set_checked(&self, id: Id<kind::Check>, checked: bool) {
        self.shared().checked.borrow_mut()[id.index] = checked;
        unsafe {
            let _ = InvalidateRect(Some(self.control(id.index)), None, false);
        }
    }

    /// The place that a placement picker shows.
    pub fn placement(&self, id: Id<kind::PlacementPicker>) -> Placement {
        self.shared().placements.borrow()[id.index]
    }

    /// Change the place that a placement picker shows. It does not give [`Event::Changed`].
    pub fn set_placement(&self, id: Id<kind::PlacementPicker>, placement: Placement) {
        self.shared().placements.borrow_mut()[id.index] = placement;
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
                Some(self.hwnd),
                &HSTRING::from(message),
                &HSTRING::from(title),
                MB_OK | MB_ICONWARNING,
            );
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
        self.ui.add_dialog(self.hwnd);
        unsafe {
            // The first ShowWindow of a process can use the show state from the shortcut or
            // the parent process (for example, hidden or minimized). The second call is exact.
            let _ = ShowWindow(self.hwnd, SW_SHOWNORMAL);
            let _ = ShowWindow(self.hwnd, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(self.hwnd);
        }
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
    pub fn run(&mut self, mut handler: impl FnMut(&mut Dialog, Event) -> Flow) -> Result<Outcome> {
        self.show();
        let mut msg = MSG::default();
        loop {
            while let Some(event) = self.next_event() {
                if handler(self, event) == Flow::Close {
                    return Ok(Outcome::Closed);
                }
            }
            unsafe {
                match GetMessageW(&mut msg, None, 0, 0).0 {
                    0 => {
                        PostQuitMessage(msg.wParam.0 as i32);
                        return Ok(Outcome::Quit);
                    }
                    -1 => {
                        return Err(Error::last("GetMessageW"));
                    }
                    _ => {}
                }
                if !self.ui.dialog_message(&msg) {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }
}

impl Drop for Dialog {
    fn drop(&mut self) {
        self.ui.remove_dialog(self.hwnd);
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            for font in [self.font, self.heading_font] {
                let _ = DeleteObject(HGDIOBJ(font.0));
            }
            drop(Box::from_raw(self.shared as *mut Shared));
        }
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

fn set_dwm_attribute(hwnd: HWND, attribute: DWMWINDOWATTRIBUTE, value: u32) {
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            attribute,
            &value as *const u32 as *const c_void,
            size_of::<u32>() as u32,
        );
    }
}

/// The popup font at 14 px, or Segoe UI on Windows versions without it.
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

/// Hover colors for buttons and drop-downs, and a closed drop-down that looks like a button.
extern "system" fn themed_control(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    index: usize,
    data: usize,
) -> LRESULT {
    unsafe {
        let shared = &*(data as *const Shared);
        let drop_down = shared.kinds.borrow().get(index) == Some(&Kind::DropDown);
        match msg {
            WM_MOUSEMOVE if shared.hover.get() != Some(index) => {
                shared.hover.set(Some(index));
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut tme);
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            WM_MOUSELEAVE if shared.hover.get() == Some(index) => {
                shared.hover.set(None);
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            WM_ERASEBKGND if drop_down => return LRESULT(1),
            WM_PAINT if drop_down => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                paint_drop_down(shared, index, hwnd, hdc);
                let _ = EndPaint(hwnd, &ps);
                return LRESULT(0);
            }
            WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE if drop_down => {
                let result = DefSubclassProc(hwnd, msg, wparam, lparam);
                let _ = InvalidateRect(Some(hwnd), None, false);
                return result;
            }
            WM_NCDESTROY => {
                let _ = RemoveWindowSubclass(hwnd, Some(themed_control), index);
            }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

fn faded(c: D2D1_COLOR_F, factor: f32) -> D2D1_COLOR_F {
    rgba(c.r, c.g, c.b, c.a * factor)
}

/// The text of a control, or of one item of a list or a drop-down.
fn text_of(hwnd: HWND, item: Option<(u32, u32, usize)>) -> Vec<u16> {
    unsafe {
        match item {
            None => {
                let len = GetWindowTextLengthW(hwnd).max(0) as usize;
                let mut buf = vec![0u16; len + 1];
                let copied = GetWindowTextW(hwnd, &mut buf).max(0) as usize;
                buf.truncate(copied);
                buf
            }
            Some((get_len, get_text, index)) => {
                let len = SendMessageW(hwnd, get_len, Some(WPARAM(index)), None).0;
                let Ok(len) = usize::try_from(len) else {
                    return Vec::new();
                };
                let mut buf = vec![0u16; len + 1];
                let copied = SendMessageW(
                    hwnd,
                    get_text,
                    Some(WPARAM(index)),
                    Some(LPARAM(buf.as_mut_ptr() as isize)),
                )
                .0;
                buf.truncate(usize::try_from(copied).unwrap_or(0).min(len));
                buf
            }
        }
    }
}

fn paint_background(shared: &Shared, hwnd: HWND, hdc: HDC) {
    let look = &shared.look;
    unsafe {
        let mut client = RECT::default();
        let _ = GetClientRect(hwnd, &mut client);
        FillRect(hdc, &client, look.window_brush);
        let Some(painter) = &look.painter else {
            return;
        };
        let focused = shared.index_of(GetFocus());
        let cards = shared.cards.borrow();
        let fields = shared.fields.borrow();
        painter.paint(hdc, &client, |t| {
            t.Clear(Some(&look.window));
            let theme = &look.theme;
            let (Ok(card), Ok(field), Ok(border), Ok(ring)) = (
                t.CreateSolidColorBrush(&look.card, None),
                t.CreateSolidColorBrush(&look.field, None),
                t.CreateSolidColorBrush(&theme.button_border, None),
                t.CreateSolidColorBrush(&faded(theme.text, 0.7), None),
            ) else {
                return;
            };
            let s = look.scale;
            let shape = |c: &RECT, radius: f32| {
                rounded(
                    c.left as f32 + 0.5,
                    c.top as f32 + 0.5,
                    c.right as f32 - 0.5,
                    c.bottom as f32 - 0.5,
                    radius * s,
                )
            };
            for c in cards.iter() {
                let card_shape = shape(c, CARD_RADIUS);
                t.FillRoundedRectangle(&card_shape, &card);
                t.DrawRoundedRectangle(&card_shape, &border, 1.0, None);
            }
            for (i, f) in fields.iter().enumerate() {
                let Some(f) = f else {
                    continue;
                };
                let field_shape = shape(f, FIELD_RADIUS);
                t.FillRoundedRectangle(&field_shape, &field);
                if focused == Some(i) {
                    t.DrawRoundedRectangle(&field_shape, &ring, 1.5 * s, None);
                } else {
                    t.DrawRoundedRectangle(&field_shape, &border, 1.0, None);
                }
            }
        });
    }
}

/// Draw a closed drop-down like a popup button, with the selected text and a chevron.
fn paint_drop_down(shared: &Shared, index: usize, hwnd: HWND, hdc: HDC) {
    let look = &shared.look;
    let Some(painter) = &look.painter else {
        return;
    };
    let theme = &look.theme;
    let (surface, _) = look.surface(shared.surfaces.borrow()[index]);
    unsafe {
        let mut client = RECT::default();
        let _ = GetClientRect(hwnd, &mut client);
        let dropped = SendMessageW(hwnd, CB_GETDROPPEDSTATE, None, None).0 != 0;
        let enabled = IsWindowEnabled(hwnd).as_bool();
        let focused = GetFocus() == hwnd;
        let hovered = shared.hover.get() == Some(index) && enabled;
        let selected = SendMessageW(hwnd, CB_GETCURSEL, None, None).0;
        let label = usize::try_from(selected)
            .map(|i| text_of(hwnd, Some((CB_GETLBTEXTLEN, CB_GETLBTEXT, i))))
            .unwrap_or_default();
        let fill_color = if dropped {
            theme.button_pressed
        } else if hovered {
            theme.button_hover
        } else {
            theme.button
        };
        let text_color = if enabled {
            theme.text
        } else {
            faded(theme.text, 0.45)
        };
        let s = look.scale;
        let (w, h) = (
            (client.right - client.left) as f32,
            (client.bottom - client.top) as f32,
        );
        painter.paint(hdc, &client, |t| {
            t.Clear(Some(&surface));
            let (Ok(fill), Ok(border), Ok(ring), Ok(text)) = (
                t.CreateSolidColorBrush(&fill_color, None),
                t.CreateSolidColorBrush(&theme.button_border, None),
                t.CreateSolidColorBrush(&faded(theme.text, 0.7), None),
                t.CreateSolidColorBrush(&text_color, None),
            ) else {
                return;
            };
            let shape = rounded(0.5, 0.5, w - 0.5, h - 0.5, BUTTON_RADIUS * s);
            t.FillRoundedRectangle(&shape, &fill);
            if focused {
                t.DrawRoundedRectangle(&shape, &ring, 1.5 * s, None);
            } else {
                t.DrawRoundedRectangle(&shape, &border, 1.0, None);
            }
            let area = D2D_RECT_F {
                left: ITEM_TEXT_INSET * s,
                top: 0.0,
                right: (w - CHEVRON_WIDTH * s).max(0.0),
                bottom: h,
            };
            t.DrawText(
                &label,
                &painter.leading,
                &area,
                &text,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            let (cx, cy) = (w - CHEVRON_WIDTH * s / 2.0, h / 2.0);
            let (dx, dy) = (4.0 * s, 2.0 * s);
            let point = |x: f32, y: f32| Vector2 { X: x, Y: y };
            t.DrawLine(
                point(cx - dx, cy - dy),
                point(cx, cy + dy),
                &text,
                1.2 * s,
                None,
            );
            t.DrawLine(
                point(cx, cy + dy),
                point(cx + dx, cy - dy),
                &text,
                1.2 * s,
                None,
            );
        });
    }
}

/// Draw one item of a list or of an open drop-down, with a rounded highlight when selected.
fn paint_item(shared: &Shared, item: &DRAWITEMSTRUCT) {
    let Some(index) = shared.index_of(item.hwndItem) else {
        return;
    };
    if item.itemState.0 & ODS_COMBOBOXEDIT.0 != 0 {
        paint_drop_down(shared, index, item.hwndItem, item.hDC);
        return;
    }
    let look = &shared.look;
    let Some(painter) = &look.painter else {
        return;
    };
    let theme = &look.theme;
    let messages = if item.CtlType == ODT_COMBOBOX {
        (CB_GETLBTEXTLEN, CB_GETLBTEXT)
    } else {
        (LB_GETTEXTLEN, LB_GETTEXT)
    };
    let label = match item.itemID {
        u32::MAX => Vec::new(),
        id => text_of(item.hwndItem, Some((messages.0, messages.1, id as usize))),
    };
    let selected = item.itemState.0 & ODS_SELECTED.0 != 0 && item.itemID != u32::MAX;
    let rect = item.rcItem;
    let (w, h) = (
        (rect.right - rect.left) as f32,
        (rect.bottom - rect.top) as f32,
    );
    let s = look.scale;
    painter.paint(item.hDC, &rect, |t| unsafe {
        t.Clear(Some(&look.field));
        let (Ok(highlight), Ok(text)) = (
            t.CreateSolidColorBrush(&theme.button_hover, None),
            t.CreateSolidColorBrush(&theme.text, None),
        ) else {
            return;
        };
        if selected {
            let shape = rounded(2.0, 1.0, w - 2.0, h - 1.0, ITEM_RADIUS * s);
            t.FillRoundedRectangle(&shape, &highlight);
        }
        let area = D2D_RECT_F {
            left: ITEM_TEXT_INSET * s,
            top: 0.0,
            right: w - 2.0,
            bottom: h,
        };
        t.DrawText(
            &label,
            &painter.leading,
            &area,
            &text,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    });
}

/// Draw a button like a popup button, or a check box in the same style.
fn paint_control(shared: &Shared, item: &DRAWITEMSTRUCT) {
    let look = &shared.look;
    let Some(painter) = &look.painter else {
        return;
    };
    let Some(index) = shared.index_of(item.hwndItem) else {
        return;
    };
    let kind = shared.kinds.borrow()[index];
    let (surface, _) = look.surface(shared.surfaces.borrow()[index]);
    let checked = shared.checked.borrow()[index];
    let state = item.itemState;
    let pressed = state.0 & ODS_SELECTED.0 != 0;
    let disabled = state.0 & ODS_DISABLED.0 != 0;
    let focused = state.0 & ODS_FOCUS.0 != 0 && state.0 & ODS_NOFOCUSRECT.0 == 0;
    let hovered = shared.hover.get() == Some(index) && !disabled;
    let theme = &look.theme;
    let label = unsafe {
        let len = GetWindowTextLengthW(item.hwndItem).max(0) as usize;
        let mut buf = vec![0u16; len + 1];
        let copied = GetWindowTextW(item.hwndItem, &mut buf).max(0) as usize;
        buf.truncate(copied);
        buf
    };
    let rect = item.rcItem;
    let (w, h) = (
        (rect.right - rect.left) as f32,
        (rect.bottom - rect.top) as f32,
    );
    let s = look.scale;
    let text_color = if disabled {
        rgba(
            theme.text.r,
            theme.text.g,
            theme.text.b,
            theme.text.a * 0.45,
        )
    } else {
        theme.text
    };
    let fill_color = if pressed {
        theme.button_pressed
    } else if hovered {
        theme.button_hover
    } else {
        theme.button
    };
    painter.paint(item.hDC, &rect, |t| unsafe {
        t.Clear(Some(&surface));
        let (Ok(text), Ok(fill), Ok(border), Ok(ring), Ok(mark)) = (
            t.CreateSolidColorBrush(&text_color, None),
            t.CreateSolidColorBrush(&fill_color, None),
            t.CreateSolidColorBrush(&theme.button_border, None),
            t.CreateSolidColorBrush(&rgba(theme.text.r, theme.text.g, theme.text.b, 0.7), None),
            t.CreateSolidColorBrush(&surface, None),
        ) else {
            return;
        };
        match kind {
            Kind::Check => {
                let size = CHECK_BOX * s;
                let top = ((h - size) / 2.0).max(0.0);
                let shape = rounded(1.5, top + 0.5, 1.5 + size, top + size, CHECK_RADIUS * s);
                if checked {
                    t.FillRoundedRectangle(&shape, &text);
                    let at = |x: f32, y: f32| Vector2 {
                        X: 1.5 + x * size,
                        Y: top + y * size,
                    };
                    let width = 1.8 * s;
                    t.DrawLine(at(0.24, 0.52), at(0.43, 0.71), &mark, width, None);
                    t.DrawLine(at(0.43, 0.71), at(0.77, 0.31), &mark, width, None);
                } else {
                    t.FillRoundedRectangle(&shape, &fill);
                    t.DrawRoundedRectangle(&shape, &ring, 1.0, None);
                }
                if focused {
                    let outline = rounded(
                        0.5,
                        top - 0.5,
                        size + 2.5,
                        top + size + 1.0,
                        CHECK_RADIUS * s,
                    );
                    t.DrawRoundedRectangle(&outline, &ring, 1.0, None);
                }
                let left = 1.5 + size + CHECK_GAP * s;
                let area = D2D_RECT_F {
                    left,
                    top: 0.0,
                    right: w,
                    bottom: h,
                };
                t.DrawText(
                    &label,
                    &painter.leading,
                    &area,
                    &text,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
            _ => {
                let shape = rounded(0.5, 0.5, w - 0.5, h - 0.5, BUTTON_RADIUS * s);
                t.FillRoundedRectangle(&shape, &fill);
                if focused {
                    t.DrawRoundedRectangle(&shape, &ring, 1.5 * s, None);
                } else {
                    t.DrawRoundedRectangle(&shape, &border, 1.0, None);
                }
                let area = D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: w,
                    bottom: h,
                };
                t.DrawText(
                    &label,
                    &painter.centered,
                    &area,
                    &text,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }
    });
}

/// Give a static text, an edit box, or a list the dialog colors.
fn control_colors(shared: &Shared, msg: u32, hdc: HDC, child: HWND) -> LRESULT {
    let look = &shared.look;
    let index = shared.index_of(child);
    let kind = index.map(|i| shared.kinds.borrow()[i]);
    let (back, brush, text) = match (msg, kind) {
        (WM_CTLCOLORSTATIC, Some(Kind::Edit)) => {
            (look.field, look.field_brush, look.dim_text(look.field))
        }
        (WM_CTLCOLORSTATIC, _) => {
            let surface = index.map_or(Surface::Window, |i| shared.surfaces.borrow()[i]);
            let (back, brush) = look.surface(surface);
            (back, brush, look.text())
        }
        _ => (look.field, look.field_brush, look.text()),
    };
    unsafe {
        SetTextColor(hdc, text);
        SetBkColor(hdc, colorref(back));
    }
    LRESULT(brush.0 as isize)
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let shared = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Shared;
        if shared.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let shared = &*shared;
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
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                let hit = shared.fields.borrow().iter().position(|f| {
                    f.is_some_and(|f| x >= f.left && x < f.right && y >= f.top && y < f.bottom)
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
                    let kind = shared.kinds.borrow().get(index).copied();
                    match (kind, code) {
                        (Some(Kind::Edit), EN_SETFOCUS | EN_KILLFOCUS)
                        | (Some(Kind::List), LBN_SETFOCUS | LBN_KILLFOCUS) => {
                            let field = shared.fields.borrow().get(index).copied().flatten();
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
                        let kind = shared.kinds.borrow().get(index).copied();
                        let control = AnyId(index);
                        match (kind, code) {
                            (Some(Kind::Check), BN_CLICKED) => {
                                if let Some(checked) = shared.checked.borrow_mut().get_mut(index) {
                                    *checked = !*checked;
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
            WM_NCDESTROY => {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                DefWindowProcW(hwnd, msg, wparam, lparam)
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

/// Where the parts of a placement picker are, in its client pixels. The picker draws the work
/// area at a smaller scale, with the window in it at the same scale.
struct Picker {
    screen: Area,
    window_width: f32,
    window_height: f32,
    margin: f32,
    work: RECT,
}

impl Picker {
    fn new(shared: &Shared, index: usize, hwnd: HWND) -> Self {
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

    fn window(&self, placement: Placement) -> D2D_RECT_F {
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
    fn placement_at(&self, corner: (f32, f32)) -> Placement {
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
fn snap(fraction: f32) -> f32 {
    [0.0, 0.5, 1.0]
        .into_iter()
        .find(|target| (fraction - target).abs() < PICKER_SNAP)
        .unwrap_or(fraction)
}

fn paint_placement(shared: &Shared, index: usize, hwnd: HWND, hdc: HDC) {
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
fn user_placement(shared: &Shared, index: usize, hwnd: HWND, placement: Placement) {
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

extern "system" fn placement_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let parent = GetParent(hwnd).unwrap_or_default();
        let shared = GetWindowLongPtrW(parent, GWLP_USERDATA) as *const Shared;
        if shared.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let shared = &*shared;
        let Some(index) = shared.index_of(hwnd) else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };
        let point = || {
            (
                (lparam.0 & 0xFFFF) as i16 as f32,
                ((lparam.0 >> 16) & 0xFFFF) as i16 as f32,
            )
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
                // Drag from the point that was clicked, or bring the center of the window to it.
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
