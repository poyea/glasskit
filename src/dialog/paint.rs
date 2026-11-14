// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The colors and the drawing of the controls, so they follow the light or dark mode.

use super::*;

/// The colors, brushes, and drawing objects of one dialog.
pub(super) struct Look {
    pub(super) theme: Theme,
    pub(super) scale: f32,
    pub(super) window: D2D1_COLOR_F,
    pub(super) card: D2D1_COLOR_F,
    pub(super) field: D2D1_COLOR_F,
    pub(super) window_brush: HBRUSH,
    pub(super) card_brush: HBRUSH,
    pub(super) field_brush: HBRUSH,
    pub(super) painter: Option<Painter>,
}

impl Look {
    pub(super) fn new(theme: Theme, scale: f32) -> Self {
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

    pub(super) fn surface(&self, surface: Surface) -> (D2D1_COLOR_F, HBRUSH) {
        match surface {
            Surface::Window => (self.window, self.window_brush),
            Surface::Card => (self.card, self.card_brush),
        }
    }

    pub(super) fn text(&self) -> COLORREF {
        colorref(over(self.theme.text, self.card))
    }

    pub(super) fn dim_text(&self, behind: D2D1_COLOR_F) -> COLORREF {
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
pub(super) struct Painter {
    pub(super) target: ID2D1DCRenderTarget,
    pub(super) centered: IDWriteTextFormat,
    pub(super) leading: IDWriteTextFormat,
    pub(super) icons: IDWriteTextFormat,
    pub(super) _factory: ID2D1Factory,
}

impl Painter {
    pub(super) fn new(scale: f32) -> windows::core::Result<Self> {
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
            let icons = dwrite.CreateTextFormat(
                crate::render::icon_family(&dwrite),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                ICON_SIZE * scale,
                w!(""),
            )?;
            icons.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            icons.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            Ok(Self {
                target,
                icons,
                centered: format(DWRITE_TEXT_ALIGNMENT_CENTER)?,
                leading: format(DWRITE_TEXT_ALIGNMENT_LEADING)?,
                _factory: factory,
            })
        }
    }

    pub(super) fn paint(&self, hdc: HDC, rect: &RECT, draw: impl FnOnce(&ID2D1RenderTarget)) {
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

pub(super) fn rounded(
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    radius: f32,
) -> D2D1_ROUNDED_RECT {
    crate::render::rounded(
        D2D_RECT_F {
            left,
            top,
            right,
            bottom,
        },
        0.0,
        radius,
    )
}

/// Hover colors for buttons and drop-downs, and a closed drop-down that looks like a button.
pub(super) extern "system" fn themed_control(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    index: usize,
    data: usize,
) -> LRESULT {
    unsafe {
        let shared = &*(data as *const Shared);
        let drop_down = shared
            .controls
            .borrow()
            .get(index)
            .is_some_and(|c| c.kind == Kind::DropDown);
        match msg {
            WM_MOUSEMOVE if shared.hover.get() != Some(index) => {
                shared.hover.set(Some(index));
                native::track_leave(hwnd);
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

pub(super) fn faded(c: D2D1_COLOR_F, factor: f32) -> D2D1_COLOR_F {
    rgba(c.r, c.g, c.b, c.a * factor)
}

/// The text of a control, or of one item of a list or a drop-down.
pub(super) fn text_of(hwnd: HWND, item: Option<(u32, u32, usize)>) -> Vec<u16> {
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

pub(super) fn paint_background(shared: &Shared, hwnd: HWND, hdc: HDC) {
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
        let controls = shared.controls.borrow();
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
            for (i, c) in controls.iter().enumerate() {
                let Some(f) = &c.field else {
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
pub(super) fn paint_drop_down(shared: &Shared, index: usize, hwnd: HWND, hdc: HDC) {
    let look = &shared.look;
    let Some(painter) = &look.painter else {
        return;
    };
    let theme = &look.theme;
    let (surface, _) = look.surface(shared.controls.borrow()[index].surface);
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
pub(super) fn paint_item(shared: &Shared, item: &DRAWITEMSTRUCT) {
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
pub(super) fn paint_control(shared: &Shared, item: &DRAWITEMSTRUCT) {
    let look = &shared.look;
    let Some(painter) = &look.painter else {
        return;
    };
    let Some(index) = shared.index_of(item.hwndItem) else {
        return;
    };
    let kind = shared.controls.borrow()[index].kind;
    let (surface, _) = look.surface(shared.controls.borrow()[index].surface);
    let checked = shared.controls.borrow()[index].checked;
    let glyph = shared.controls.borrow()[index].glyph.clone();
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
                let (label, format) = match &glyph {
                    Some(glyph) => (glyph, &painter.icons),
                    None => (&label, &painter.centered),
                };
                t.DrawText(
                    label,
                    format,
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
pub(super) fn control_colors(shared: &Shared, msg: u32, hdc: HDC, child: HWND) -> LRESULT {
    let look = &shared.look;
    let index = shared.index_of(child);
    let kind = index.map(|i| shared.controls.borrow()[i].kind);
    let (back, brush, text) = match (msg, kind) {
        (WM_CTLCOLORSTATIC, Some(Kind::Edit)) => {
            (look.field, look.field_brush, look.dim_text(look.field))
        }
        (WM_CTLCOLORSTATIC, _) => {
            let surface = index.map_or(Surface::Window, |i| shared.controls.borrow()[i].surface);
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
