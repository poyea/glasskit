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
        /// `CreateWindowExW` or `drawing the popup`.
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
            Error::InvalidIcon { size, pixels } => write!(
                f,
                "an icon of size {size} needs {} pixels, not {pixels}",
                u64::from(*size) * u64::from(*size)
            ),
        }
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
