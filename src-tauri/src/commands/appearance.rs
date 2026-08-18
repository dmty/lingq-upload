//! macOS system accent colours.
//!
//! CSS `AccentColor` is a dead end here: WebKit parses it but resolves it to a
//! fixed `rgb(0, 122, 255)` regardless of the user's accent, and
//! `AccentColorText` to black rather than the white AppKit actually draws. The
//! only way to follow the real setting is to ask AppKit.

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// sRGB hex strings (`#rrggbb`) ready to drop into CSS custom properties.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct SystemAccent {
    /// Tint for controls and filled buttons (`NSColor.controlAccentColor`).
    pub accent: String,
    /// Text drawn on top of `accent` (`NSColor.alternateSelectedControlTextColor`).
    pub accent_fg: String,
}

#[tauri::command]
#[specta::specta]
pub fn cmd_system_accent(app: tauri::AppHandle) -> Result<Option<SystemAccent>, AppError> {
    platform::read(&app)
}

/// NSColor components are 0.0-1.0 floats; CSS wants `#rrggbb`.
///
/// Only the AppKit path and its tests call this; without the gate it is dead
/// code on every other platform, which `-D warnings` rejects.
#[cfg(any(target_os = "macos", test))]
fn to_hex(red: f64, green: f64, blue: f64) -> String {
    let channel = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(red),
        channel(green),
        channel(blue)
    )
}

#[cfg(test)]
mod tests {
    use super::to_hex;

    #[test]
    fn formats_components_as_css_hex() {
        assert_eq!(to_hex(0.0, 0.0, 0.0), "#000000");
        assert_eq!(to_hex(1.0, 1.0, 1.0), "#ffffff");
        // macOS red accent: rgb(255, 82, 87)
        assert_eq!(to_hex(1.0, 82.0 / 255.0, 87.0 / 255.0), "#ff5257");
        // Every channel stays two digits, so the string is always parseable.
        assert_eq!(to_hex(0.04, 0.04, 0.04), "#0a0a0a");
    }

    #[test]
    fn clamps_out_of_range_components() {
        // Wide-gamut colours converted to sRGB can land slightly outside 0-1.
        assert_eq!(to_hex(1.4, -0.2, 0.5), "#ff0080");
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::SystemAccent;
    use crate::error::AppError;

    use objc2_app_kit::{NSColor, NSColorSpace};

    /// AppKit colours are dynamic: they only resolve to real components once
    /// converted into a concrete colour space.
    fn hex(color: &NSColor) -> String {
        let Some(srgb) = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace()) else {
            return "#000000".to_string();
        };
        super::to_hex(
            srgb.redComponent(),
            srgb.greenComponent(),
            srgb.blueComponent(),
        )
    }

    fn read_on_main() -> SystemAccent {
        SystemAccent {
            accent: hex(&NSColor::controlAccentColor()),
            accent_fg: hex(&NSColor::alternateSelectedControlTextColor()),
        }
    }

    /// Synchronous tauri commands already run on the main thread, and blocking
    /// there on a closure queued *for* the main thread deadlocks the app. Only
    /// marshal when we are genuinely somewhere else.
    pub fn read(app: &tauri::AppHandle) -> Result<Option<SystemAccent>, AppError> {
        if objc2::MainThreadMarker::new().is_some() {
            return Ok(Some(read_on_main()));
        }
        let (tx, rx) = std::sync::mpsc::channel();
        app.run_on_main_thread(move || {
            let _ = tx.send(read_on_main());
        })
        .map_err(|e| AppError::Other(format!("run_on_main_thread: {e}")))?;
        rx.recv()
            .map(Some)
            .map_err(|e| AppError::Other(format!("system accent: {e}")))
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::SystemAccent;
    use crate::error::AppError;

    pub fn read(_app: &tauri::AppHandle) -> Result<Option<SystemAccent>, AppError> {
        Ok(None)
    }
}
