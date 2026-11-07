// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The colors of all glasskit windows. They follow the Windows light or dark app mode, the
//! accent color, high contrast, and the setting for transparency effects.

use windows::UI::Color;
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::Gdi::{
    COLOR_BTNFACE, COLOR_BTNTEXT, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT,
    COLOR_WINDOW, COLOR_WINDOWTEXT, GetSysColor, SYS_COLOR_INDEX,
};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
};
use windows::core::{PCWSTR, w};

#[derive(Clone, Copy)]
pub(crate) struct Theme {
    /// The window color. The popup shows it with alpha over the blurred desktop.
    pub tint: Color,
    pub text: D2D1_COLOR_F,
    pub button: D2D1_COLOR_F,
    pub button_hover: D2D1_COLOR_F,
    pub button_pressed: D2D1_COLOR_F,
    pub button_border: D2D1_COLOR_F,
    pub window_border: D2D1_COLOR_F,
    /// The fill of a button with [`Mark::Good`](crate::popup::Mark::Good).
    pub good: D2D1_COLOR_F,
    /// The fill of a button with [`Mark::Bad`](crate::popup::Mark::Bad).
    pub bad: D2D1_COLOR_F,
    /// The background of an edit box or a list, over a card.
    pub field: D2D1_COLOR_F,
    /// The accent color of the user, for a main button and the focus ring.
    pub accent: D2D1_COLOR_F,
    /// Text on the accent color.
    pub on_accent: D2D1_COLOR_F,
    /// A panel (card) on a window.
    pub card: D2D1_COLOR_F,
    pub card_border: D2D1_COLOR_F,
    /// Text that is less important, or that is turned off.
    pub subtle_text: D2D1_COLOR_F,
    pub dark: bool,
    /// High contrast is on. Then all colors are system colors, and nothing is see-through.
    pub high_contrast: bool,
    /// The user allows transparency effects. Without them, a window has no glass.
    pub transparency: bool,
}

pub(crate) const fn rgba(r: f32, g: f32, b: f32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r, g, b, a }
}

impl Theme {
    const LIGHT: Theme = Theme {
        tint: Color {
            A: 161,
            R: 243,
            G: 243,
            B: 243,
        },
        text: rgba(0.0, 0.0, 0.0, 0.9),
        button: rgba(1.0, 1.0, 1.0, 0.45),
        button_hover: rgba(1.0, 1.0, 1.0, 0.75),
        button_pressed: rgba(1.0, 1.0, 1.0, 0.3),
        button_border: rgba(0.0, 0.0, 0.0, 0.08),
        window_border: rgba(0.0, 0.0, 0.0, 0.12),
        good: rgba(0.30, 0.78, 0.42, 0.85),
        bad: rgba(0.94, 0.40, 0.40, 0.85),
        field: rgba(1.0, 1.0, 1.0, 1.0),
        accent: rgba(0.0, 0.37, 0.72, 1.0),
        on_accent: rgba(1.0, 1.0, 1.0, 1.0),
        card: rgba(1.0, 1.0, 1.0, 0.7),
        card_border: rgba(0.0, 0.0, 0.0, 0.06),
        subtle_text: rgba(0.0, 0.0, 0.0, 0.6),
        dark: false,
        high_contrast: false,
        transparency: true,
    };

    const DARK: Theme = Theme {
        tint: Color {
            A: 161,
            R: 32,
            G: 32,
            B: 32,
        },
        text: rgba(1.0, 1.0, 1.0, 0.95),
        button: rgba(1.0, 1.0, 1.0, 0.08),
        button_hover: rgba(1.0, 1.0, 1.0, 0.16),
        button_pressed: rgba(1.0, 1.0, 1.0, 0.04),
        button_border: rgba(1.0, 1.0, 1.0, 0.1),
        window_border: rgba(1.0, 1.0, 1.0, 0.12),
        good: rgba(0.13, 0.55, 0.27, 0.85),
        bad: rgba(0.72, 0.18, 0.18, 0.85),
        field: rgba(0.0, 0.0, 0.0, 0.3),
        accent: rgba(0.38, 0.8, 1.0, 1.0),
        on_accent: rgba(0.0, 0.0, 0.0, 1.0),
        card: rgba(1.0, 1.0, 1.0, 0.05),
        card_border: rgba(0.0, 0.0, 0.0, 0.1),
        subtle_text: rgba(1.0, 1.0, 1.0, 0.6),
        dark: true,
        high_contrast: false,
        transparency: true,
    };

    pub fn current() -> Theme {
        if high_contrast_on() {
            return Theme::high_contrast();
        }
        let mut theme = if personalize("AppsUseLightTheme") == Some(0) {
            Theme::DARK
        } else {
            Theme::LIGHT
        };
        theme.transparency = personalize("EnableTransparency") != Some(0);
        if let Some(accent) = accent_color(theme.dark) {
            theme.accent = accent;
        }
        theme
    }

    /// The system colors of high contrast. Nothing is see-through.
    fn high_contrast() -> Theme {
        let system = |index: SYS_COLOR_INDEX| {
            let c = unsafe { GetSysColor(index) };
            rgba(
                (c & 0xFF) as f32 / 255.0,
                ((c >> 8) & 0xFF) as f32 / 255.0,
                ((c >> 16) & 0xFF) as f32 / 255.0,
                1.0,
            )
        };
        let window = system(COLOR_WINDOW);
        let text = system(COLOR_WINDOWTEXT);
        let button = system(COLOR_BTNFACE);
        let highlight = system(COLOR_HIGHLIGHT);
        let byte = |v: f32| (v * 255.0).round() as u8;
        Theme {
            tint: Color {
                A: 255,
                R: byte(window.r),
                G: byte(window.g),
                B: byte(window.b),
            },
            text,
            button,
            button_hover: highlight,
            button_pressed: highlight,
            button_border: system(COLOR_BTNTEXT),
            window_border: text,
            good: highlight,
            bad: highlight,
            field: window,
            accent: highlight,
            on_accent: system(COLOR_HIGHLIGHTTEXT),
            card: window,
            card_border: text,
            subtle_text: system(COLOR_GRAYTEXT),
            dark: (window.r + window.g + window.b) < 1.5,
            high_contrast: true,
            transparency: false,
        }
    }

    /// Whether windows can show glass with this theme.
    pub fn glass(&self) -> bool {
        self.transparency && !self.high_contrast
    }

    /// The window color without alpha, for a window that has no blur behind it.
    pub fn background(&self) -> D2D1_COLOR_F {
        let c = self.tint;
        rgba(
            f32::from(c.R) / 255.0,
            f32::from(c.G) / 255.0,
            f32::from(c.B) / 255.0,
            1.0,
        )
    }
}

/// A value of the Personalize key of the current user.
fn personalize(name: &str) -> Option<u32> {
    let name: Vec<u16> = name.encode_utf16().chain([0]).collect();
    registry_dword(
        w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
        PCWSTR(name.as_ptr()),
    )
}

fn registry_dword(key: PCWSTR, name: PCWSTR) -> Option<u32> {
    let mut value: u32 = 0;
    let mut size = size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            name,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    status.is_ok().then_some(value)
}

/// The accent color of the user. Windows keeps it as `0xAABBGGRR`. In dark mode, a lighter
/// accent is easier to read, as Windows does.
fn accent_color(dark: bool) -> Option<D2D1_COLOR_F> {
    let abgr = registry_dword(w!("Software\\Microsoft\\Windows\\DWM"), w!("AccentColor"))?;
    let channel = |shift: u32| ((abgr >> shift) & 0xFF) as f32 / 255.0;
    let (r, g, b) = (channel(0), channel(8), channel(16));
    let lift = |v: f32| if dark { v + (1.0 - v) * 0.45 } else { v };
    Some(rgba(lift(r), lift(g), lift(b), 1.0))
}

fn high_contrast_on() -> bool {
    let mut hc = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            hc.cbSize,
            Some(&mut hc as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    ok.is_ok() && hc.dwFlags.contains(HCF_HIGHCONTRASTON)
}

/// `top` with its alpha over the opaque `bottom`.
pub(crate) fn over(top: D2D1_COLOR_F, bottom: D2D1_COLOR_F) -> D2D1_COLOR_F {
    let mix = |t: f32, b: f32| t * top.a + b * (1.0 - top.a);
    rgba(
        mix(top.r, bottom.r),
        mix(top.g, bottom.g),
        mix(top.b, bottom.b),
        1.0,
    )
}

pub(crate) fn colorref(c: D2D1_COLOR_F) -> COLORREF {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    COLORREF(byte(c.r) | (byte(c.g) << 8) | (byte(c.b) << 16))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn over_mixes_by_alpha() {
        let white_half = rgba(1.0, 1.0, 1.0, 0.5);
        let black = rgba(0.0, 0.0, 0.0, 1.0);
        let mixed = over(white_half, black);
        assert!((mixed.r - 0.5).abs() < 1e-6 && mixed.a == 1.0);
    }

    #[test]
    fn colorref_is_red_green_blue_from_low_byte() {
        assert_eq!(colorref(rgba(1.0, 0.0, 0.0, 1.0)).0, 0x0000FF);
        assert_eq!(colorref(rgba(0.0, 0.0, 1.0, 1.0)).0, 0xFF0000);
    }

    #[test]
    fn background_is_the_opaque_tint() {
        let bg = Theme::DARK.background();
        assert_eq!(colorref(bg).0, 0x202020);
    }
}
