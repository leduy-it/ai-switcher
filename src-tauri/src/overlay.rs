//! Always-on-top quota overlay: a second, frameless window that floats above every app so the
//! quota bars stay visible while the user is coding in a terminal/IDE.
//!
//! The window is created on demand (tray toggle, Settings switch, or restored at startup when it
//! was left on). It renders the same React bundle as the main window — `main.tsx` branches on the
//! window label — so there is no second HTML entry to keep in sync.
//!
//! Geometry lives in `StoredState.overlay.rect` and is written back from the move/resize handler
//! in `lib.rs`, which is why nothing here tries to remember positions itself.

use crate::app_state::ManagedState;
use crate::models::{OverlayRect, OverlaySettings};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "overlay";

/// Generation counter for the hover watcher below. Bumping it retires the running thread.
static HOVER_WATCH: AtomicU64 = AtomicU64::new(0);

/// How often the click-through hover watcher samples the pointer.
const HOVER_POLL: Duration = Duration::from_millis(140);

/// Opens the overlay (or focuses/reveals it if it already exists).
pub fn show(app: &AppHandle) -> tauri::Result<()> {
    let settings = app
        .state::<ManagedState>()
        .overlay_settings()
        .unwrap_or_default();

    if let Some(window) = app.get_webview_window(LABEL) {
        window.show()?;
        let _ = window.set_always_on_top(true);
        let _ = window.set_ignore_cursor_events(settings.click_through && !settings.minimized);
        return Ok(());
    }

    let rect = sanitize_rect(app, settings.rect);

    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("Quota")
        .inner_size(if settings.minimized { 70.0 } else { rect.width }, if settings.minimized { 70.0 } else { rect.height })
        .position(rect.x, rect.y)
        .min_inner_size(70.0, 70.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .resizable(true)
        .skip_taskbar(true)
        .shadow(true)
        // Don't steal focus from the terminal/editor when the overlay appears.
        .focused(false)
        // First click acts on the control under the cursor instead of only activating the window.
        .accept_first_mouse(true)
        .build()?;

    // Match the main app's native material while keeping the quota panel light enough to read
    // through at its configured idle opacity.
    #[cfg(target_os = "macos")]
    {
        let effects = tauri::window::EffectsBuilder::new()
            .effects([
                tauri::window::Effect::LiquidGlassClear,
                tauri::window::Effect::HudWindow,
            ])
            .radius(16.0)
            .interactive(true)
            .build();
        let _ = window.set_effects(effects);
    }

    // Keep it visible when the user switches Spaces / enters another full-screen app.
    let _ = window.set_visible_on_all_workspaces(true);
    let _ = window.set_ignore_cursor_events(settings.click_through && !settings.minimized);
    Ok(())
}

/// Closes the overlay window if it is open. Does not touch the `enabled` flag.
pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.close();
    }
}

/// Applies settings to the live window and tells both windows to re-render with them.
pub fn apply(app: &AppHandle, settings: &OverlaySettings) {
    if settings.enabled {
        let _ = show(app);
    } else {
        hide(app);
    }
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.set_ignore_cursor_events(settings.click_through && !settings.minimized);
        let rect = sanitize_rect(app, settings.rect);
        let _ = window.set_position(LogicalPosition::new(rect.x, rect.y));
        let size = if settings.minimized { LogicalSize::new(70.0, 70.0) } else { LogicalSize::new(rect.width, rect.height) };
        let _ = window.set_size(size);
        let _ = window.set_resizable(!settings.minimized);
        let _ = window.set_shadow(!settings.minimized);
    }
    sync_hover_watch(app, settings);
    let _ = app.emit("overlay-settings-changed", settings);
}

/// Start or stop the pointer watcher.
///
/// The overlay fades in when the pointer is over it. Normally the webview's own mouse events do
/// that for free — but in click-through mode the window ignores the cursor entirely, so no DOM
/// event ever arrives. There the app samples the pointer position instead and pushes the hover
/// state in, which is what makes "clicks pass through" and "brighten when I point at it" work at
/// the same time.
fn sync_hover_watch(app: &AppHandle, settings: &OverlaySettings) {
    // Any running watcher belongs to the previous settings — retire it.
    let generation = HOVER_WATCH.fetch_add(1, Ordering::SeqCst) + 1;
    if !(settings.enabled && settings.click_through && !settings.minimized) {
        // Leaving click-through: drop any stale "hovered" state the watcher pushed.
        let _ = app.emit("overlay-hover", false);
        return;
    }

    let app = app.clone();
    std::thread::spawn(move || {
        let mut last = None;
        loop {
            if HOVER_WATCH.load(Ordering::SeqCst) != generation {
                return; // superseded by a newer settings change
            }
            let Some(window) = app.get_webview_window(LABEL) else {
                return; // overlay closed
            };
            let hovered = pointer_is_over(&app, &window).unwrap_or(false);
            if last != Some(hovered) {
                last = Some(hovered);
                let _ = app.emit("overlay-hover", hovered);
            }
            std::thread::sleep(HOVER_POLL);
        }
    });
}

/// Whether the pointer currently sits inside the overlay's frame (physical pixels, so no scale
/// conversion is needed — both readings come from the same coordinate space).
fn pointer_is_over(app: &AppHandle, window: &tauri::WebviewWindow) -> tauri::Result<bool> {
    let cursor = app.cursor_position()?;
    let origin = window.outer_position()?;
    let size = window.outer_size()?;
    Ok(rect_contains(
        (origin.x as f64, origin.y as f64),
        (size.width as f64, size.height as f64),
        (cursor.x, cursor.y),
    ))
}

/// Point-in-rect on the screen's physical pixel grid. Kept separate so the containment maths can
/// be tested without a real window (a sign slip here would silently disable hover-to-reveal).
fn rect_contains(origin: (f64, f64), size: (f64, f64), point: (f64, f64)) -> bool {
    point.0 >= origin.0
        && point.1 >= origin.1
        && point.0 < origin.0 + size.0
        && point.1 < origin.1 + size.1
}

#[cfg(test)]
mod tests {
    use super::rect_contains;

    #[test]
    fn hover_rect_covers_the_window_and_nothing_else() {
        let origin = (100.0, 50.0);
        let size = (280.0, 330.0);
        // inside: top-left corner, middle, and just short of the far edge
        assert!(rect_contains(origin, size, (100.0, 50.0)));
        assert!(rect_contains(origin, size, (240.0, 200.0)));
        assert!(rect_contains(origin, size, (379.9, 379.9)));
        // outside on every side, including the exclusive far edge
        assert!(!rect_contains(origin, size, (99.0, 200.0)));
        assert!(!rect_contains(origin, size, (240.0, 49.0)));
        assert!(!rect_contains(origin, size, (380.0, 200.0)));
        assert!(!rect_contains(origin, size, (240.0, 380.0)));
        // a second monitor to the left uses negative coordinates
        assert!(rect_contains((-900.0, -200.0), size, (-800.0, -100.0)));
        assert!(!rect_contains((-900.0, -200.0), size, (-1000.0, -100.0)));
    }
}

/// Keeps a restored position on an actually-connected monitor. Unplugging the external display the
/// overlay was parked on would otherwise reopen it somewhere invisible.
fn sanitize_rect(app: &AppHandle, rect: OverlayRect) -> OverlayRect {
    let mut rect = rect;
    rect.width = rect.width.clamp(190.0, 900.0);
    rect.height = rect.height.clamp(78.0, 900.0);

    let monitors = app.available_monitors().unwrap_or_default();
    if monitors.is_empty() {
        return rect;
    }

    // A remembered panel could be mostly beyond the display edge. Keep its entire expanded
    // surface reachable, even when it is currently a small bubble.
    let target = monitors.iter().max_by(|a, b| {
        let overlap = |monitor: &tauri::Monitor| {
            let scale = monitor.scale_factor();
            let position: LogicalPosition<f64> = monitor.work_area().position.to_logical(scale);
            let size: LogicalSize<f64> = monitor.work_area().size.to_logical(scale);
            let x = ((rect.x + rect.width).min(position.x + size.width) - rect.x.max(position.x)).max(0.0);
            let y = ((rect.y + rect.height).min(position.y + size.height) - rect.y.max(position.y)).max(0.0);
            x * y
        };
        overlap(a).total_cmp(&overlap(b))
    });
    if let Some(monitor) = target {
        let scale = monitor.scale_factor();
        let position: LogicalPosition<f64> = monitor.work_area().position.to_logical(scale);
        let size: LogicalSize<f64> = monitor.work_area().size.to_logical(scale);
        rect.width = rect.width.min((size.width - 16.0).max(70.0));
        rect.height = rect.height.min((size.height - 16.0).max(70.0));
        rect.x = rect.x.clamp(position.x + 8.0, (position.x + size.width - rect.width - 8.0).max(position.x + 8.0));
        rect.y = rect.y.clamp(position.y + 8.0, (position.y + size.height - rect.height - 8.0).max(position.y + 8.0));
    }
    rect
}
