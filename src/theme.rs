// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The colors of all glasskit windows. They follow the Windows light or dark app mode.

use windows::UI::Color;
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::w;

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
    pub dark: bool,
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
        dark: false,
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
        dark: true,
    };

    pub fn current() -> Theme {
        let mut value: u32 = 1;
        let mut size = size_of::<u32>() as u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
                w!("AppsUseLightTheme"),
                RRF_RT_REG_DWORD,
                None,
                Some(&mut value as *mut u32 as *mut _),
                Some(&mut size),
            )
        };
        if status.is_ok() && value == 0 {
            Theme::DARK
        } else {
            Theme::LIGHT
        }
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
