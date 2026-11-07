// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The error type of `glasskit`.

use std::fmt;

/// An error from `glasskit`.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A Windows API call failed.
    Windows {
        /// The Windows function that failed, or what `glasskit` was doing, for example
        /// `CreateWindowExW` or `drawing`.
        context: &'static str,
        /// The error that Windows gave.
        source: windows::core::Error,
    },
    /// The path of the running executable is not available.
    CurrentExe(std::io::Error),
    /// Windows did not add the tray icon. This can occur while Explorer is not running.
    TrayIconNotAdded,
    /// [`ChoicePopup::show`](crate::popup::ChoicePopup::show) was called while a prompt of
    /// the same popup showed.
    PopupShowing,
    /// An image for a popup has no pixels, a side larger than 4096, or a number of pixels
    /// that is not its width times its height.
    InvalidImage {
        /// The width of the image.
        width: u32,
        /// The height of the image.
        height: u32,
        /// The number of pixels given.
        pixels: usize,
    },
    /// The number of pixels of an icon is not the square of its size.
    InvalidIcon {
        /// The width and height of the icon.
        size: u32,
        /// The number of pixels given.
        pixels: usize,
    },
}

/// A result with [`Error`] as the error type.
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// The last error of the thread, for a Windows function that tells failure only by its
    /// return value.
    pub(crate) fn last(context: &'static str) -> Self {
        Error::Windows {
            context,
            source: windows::core::Error::from_thread(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Windows { context, source } => write!(f, "{context} failed: {source}"),
            Error::CurrentExe(_) => f.write_str("the path of the executable is not available"),
            Error::TrayIconNotAdded => f.write_str("Windows did not add the tray icon"),
            Error::PopupShowing => f.write_str("the popup shows a prompt already"),
            Error::InvalidImage {
                width,
                height,
                pixels,
            } => write!(
                f,
                "an image of {width} x {height} pixels (each side 1 to 4096) cannot have {pixels} pixels"
            ),
            Error::InvalidIcon { size, pixels } => write!(
                f,
                "an icon of size {size} needs {} pixels, not {pixels}",
                u64::from(*size) * u64::from(*size)
            ),
        }
    }
}

impl Error {
    /// The GPU device is gone, for example after a driver update or a GPU reset. The
    /// devices must be made again.
    pub(crate) fn device_lost(&self) -> bool {
        const DXGI_ERROR_DEVICE_REMOVED: i32 = 0x887A_0005_u32 as i32;
        const DXGI_ERROR_DEVICE_RESET: i32 = 0x887A_0007_u32 as i32;
        const D2DERR_RECREATE_TARGET: i32 = 0x8899_000C_u32 as i32;
        matches!(
            self,
            Error::Windows { source, .. }
                if matches!(
                    source.code().0,
                    DXGI_ERROR_DEVICE_REMOVED | DXGI_ERROR_DEVICE_RESET | D2DERR_RECREATE_TARGET
                )
        )
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Windows { source, .. } => Some(source),
            Error::CurrentExe(source) => Some(source),
            _ => None,
        }
    }
}

/// Name the Windows function of a failed call.
pub(crate) trait Call<T> {
    fn call(self, name: &'static str) -> Result<T>;
}

impl<T> Call<T> for windows::core::Result<T> {
    fn call(self, name: &'static str) -> Result<T> {
        self.map_err(|source| Error::Windows {
            context: name,
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows(code: u32) -> Error {
        Error::Windows {
            context: "drawing",
            source: windows::core::Error::from_hresult(windows::core::HRESULT(code as i32)),
        }
    }

    #[test]
    fn only_gpu_loss_counts_as_device_lost() {
        assert!(windows(0x887A_0005).device_lost());
        assert!(windows(0x887A_0007).device_lost());
        assert!(windows(0x8899_000C).device_lost());
        assert!(!windows(0x8007_0005).device_lost());
        assert!(!Error::PopupShowing.device_lost());
    }

    #[test]
    fn windows_error_names_the_call() {
        let err = Error::Windows {
            context: "CreateWindowExW",
            source: windows::core::Error::from_hresult(windows::core::HRESULT(-2147024891)),
        };
        assert!(err.to_string().starts_with("CreateWindowExW failed: "));
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn invalid_icon_shows_the_expected_count() {
        let err = Error::InvalidIcon {
            size: 16,
            pixels: 10,
        };
        assert_eq!(
            err.to_string(),
            "an icon of size 16 needs 256 pixels, not 10"
        );
    }

    #[test]
    fn error_can_cross_threads() {
        fn send_sync<T: Send + Sync + 'static>() {}
        send_sync::<Error>();
    }
}
