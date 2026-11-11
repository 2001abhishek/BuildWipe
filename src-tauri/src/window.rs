//! Opens the main window at a size that fits the screen it's on, centred in
//! the area not covered by the taskbar or dock.
//!
//! The window starts hidden (`visible: false` in tauri.conf.json), so the
//! user never sees it jump from its default size to the fitted one.

use tauri::{LogicalSize, PhysicalPosition, WebviewWindow};

/// The size the app is designed around, in logical pixels.
const PREFERRED: (f64, f64) = (1120.0, 720.0);
/// The smallest size the layout supports without clipping.
const MINIMUM: (f64, f64) = (760.0, 520.0);
/// How much of the screen's usable area a new window may take up.
const SCREEN_SHARE: f64 = 0.9;

#[derive(Debug, PartialEq)]
pub struct Fit {
    pub size: (f64, f64),
    pub min_size: (f64, f64),
}

/// Window size and minimum size for a screen whose usable area is `work`, in
/// logical pixels. A small screen gets a smaller window rather than one that
/// spills off the edge, and the minimum never exceeds what the screen can show.
pub fn fit(work: (f64, f64)) -> Fit {
    let size = (
        PREFERRED.0.min((work.0 * SCREEN_SHARE).floor()),
        PREFERRED.1.min((work.1 * SCREEN_SHARE).floor()),
    );
    Fit {
        size,
        min_size: (MINIMUM.0.min(size.0), MINIMUM.1.min(size.1)),
    }
}

/// Sizes and centres `window` on its screen, then shows it. Any failure still
/// shows the window at its configured size, so the app always opens.
pub fn show_fitted(window: &WebviewWindow) {
    if let Err(error) = place(window) {
        eprintln!("Couldn't fit the window to the screen: {error}");
        let _ = window.center();
    }
    let _ = window.show();
    let _ = window.set_focus();
}

fn place(window: &WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = window.current_monitor()?.or(window.primary_monitor()?) else {
        return window.center();
    };
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let fitted = fit((
        f64::from(area.size.width) / scale,
        f64::from(area.size.height) / scale,
    ));

    window.set_min_size(Some(LogicalSize::new(fitted.min_size.0, fitted.min_size.1)))?;
    window.set_size(LogicalSize::new(fitted.size.0, fitted.size.1))?;

    // Centre within the work area rather than the whole screen, so the taskbar
    // or dock never covers the bottom of the window.
    let outer = window.outer_size()?;
    let x = area.position.x + (i64::from(area.size.width) - i64::from(outer.width)).max(0) as i32 / 2;
    let y = area.position.y + (i64::from(area.size.height) - i64::from(outer.height)).max(0) as i32 / 2;
    window.set_position(PhysicalPosition::new(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_large_screen_gets_the_preferred_size() {
        // 1920×1080 at 100%, taskbar 40 px.
        assert_eq!(fit((1920.0, 1040.0)), Fit { size: PREFERRED, min_size: MINIMUM });
        // 4K at 150%.
        assert_eq!(fit((2560.0, 1400.0)).size, PREFERRED);
    }

    #[test]
    fn a_common_laptop_screen_gets_a_window_that_fits() {
        // 1366×768 at 125% leaves about 1093×566 usable.
        let laptop = fit((1093.0, 566.0));
        assert_eq!(laptop.size, (983.0, 509.0));
        assert!(laptop.size.1 <= 566.0);
        // The minimum height shrinks so the window can still be made to fit.
        assert_eq!(laptop.min_size, (MINIMUM.0, 509.0));
    }

    #[test]
    fn a_tiny_screen_never_gets_a_minimum_larger_than_itself() {
        // 1280×720 at 150%.
        let tiny = fit((853.0, 440.0));
        assert!(tiny.size.0 <= 853.0 && tiny.size.1 <= 440.0);
        assert!(tiny.min_size.0 <= tiny.size.0 && tiny.min_size.1 <= tiny.size.1);
    }
}
