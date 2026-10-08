//! Quota table anchored below the status-bar icon. Kept alive while hidden for instant reopening.

use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Rect, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "menubar";

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window(LABEL).is_some() {
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("Michael Le Profiles · Quota")
        .inner_size(720.0, 540.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .resizable(false)
        .skip_taskbar(true)
        .shadow(true)
        .visible(false)
        .focused(false)
        .accept_first_mouse(true)
        .build()?;
    #[cfg(target_os = "macos")]
    {
        let effects = tauri::window::EffectsBuilder::new()
            .effects([tauri::window::Effect::LiquidGlassRegular, tauri::window::Effect::HudWindow])
            .radius(20.0)
            .interactive(true)
            .build();
        let _ = window.set_effects(effects);
    }
    let _ = window.set_visible_on_all_workspaces(true);
    Ok(())
}

pub fn show(app: &AppHandle, anchor: Option<Rect>) -> tauri::Result<()> {
    create(app)?;
    let Some(window) = app.get_webview_window(LABEL) else { return Ok(()); };
    let anchor = anchor.or_else(|| app.tray_by_id("main-tray").and_then(|tray| tray.rect().ok().flatten()));
    let cursor = app.cursor_position().unwrap_or_default();
    let monitor = app.monitor_from_point(cursor.x, cursor.y)?
        .or(app.primary_monitor()?);
    if let Some(monitor) = monitor {
        let scale = monitor.scale_factor();
        let work = monitor.work_area();
        let origin = work.position.to_logical::<f64>(scale);
        let size = work.size.to_logical::<f64>(scale);
        let (anchor_x, anchor_y, anchor_width, anchor_height) = anchor.map(|rect| {
            let pos = rect.position.to_logical::<f64>(scale);
            let size = rect.size.to_logical::<f64>(scale);
            (pos.x, pos.y, size.width, size.height)
        }).unwrap_or((origin.x + size.width - 32.0, origin.y - 24.0, 24.0, 24.0));
        let (x, y, width, height) = panel_rect(
            (origin.x, origin.y, size.width, size.height),
            (anchor_x, anchor_y, anchor_width, anchor_height),
        );
        window.set_size(LogicalSize::new(width, height))?;
        window.set_position(LogicalPosition::new(x, y))?;
        // Let the arrow continue to point at the icon when the panel is clamped at a screen edge.
        let _ = window.emit("quota-panel-anchor", (anchor_x + anchor_width / 2.0 - x).clamp(24.0, width - 24.0));
    }
    window.show()?;
    window.set_focus()?;
    let _ = window.emit("quota-panel-opened", ());
    Ok(())
}

pub fn toggle(app: &AppHandle, anchor: Rect) -> tauri::Result<()> {
    if app.get_webview_window(LABEL).is_some_and(|window| window.is_visible().unwrap_or(false)) {
        hide(app);
        return Ok(());
    }
    show(app, Some(anchor))
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) { let _ = window.hide(); }
}

/// Clicking the icon itself is handled by `toggle`, so don't hide on that focus transition first.
pub fn dismiss_on_blur(app: &AppHandle) {
    if let (Ok(cursor), Some(tray)) = (app.cursor_position(), app.tray_by_id("main-tray")) {
        if let Ok(Some(rect)) = tray.rect() {
            let scale = app.monitor_from_point(cursor.x, cursor.y).ok().flatten()
                .map(|monitor| monitor.scale_factor()).unwrap_or(1.0);
            let pos = rect.position.to_physical::<f64>(scale);
            let size = rect.size.to_physical::<f64>(scale);
            if cursor.x >= pos.x && cursor.x <= pos.x + size.width
                && cursor.y >= pos.y && cursor.y <= pos.y + size.height { return; }
        }
    }
    hide(app);
}

pub fn open_main(app: &AppHandle, fullscreen: bool) -> tauri::Result<()> {
    hide(app);
    if let Some(window) = app.get_webview_window("main") {
        window.show()?;
        window.unminimize()?;
        window.set_fullscreen(fullscreen)?;
        window.set_focus()?;
    }
    Ok(())
}

/// Place the panel below its anchor and keep its whole surface inside the usable monitor area.
fn panel_rect(work: (f64, f64, f64, f64), anchor: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    let (left, top, screen_width, screen_height) = work;
    let width = 720.0_f64.min((screen_width - 16.0).max(1.0));
    let height = 540.0_f64.min((screen_height - 16.0).max(1.0));
    let x = (anchor.0 + anchor.2 / 2.0 - width / 2.0)
        .clamp(left + 8.0, left + screen_width - width - 8.0);
    let y = (anchor.1 + anchor.3 + 4.0)
        .clamp(top + 4.0, (top + screen_height - height - 8.0).max(top + 4.0));
    (x, y, width, height)
}
