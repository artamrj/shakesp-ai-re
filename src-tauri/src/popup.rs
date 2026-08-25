use std::sync::Mutex;

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindowBuilder,
};

const POPUP_LABEL: &str = "popup";
const POPUP_WIDTH: f64 = 420.0;
const POPUP_HEIGHT: f64 = 240.0;
const POPUP_GAP: f64 = 8.0;

#[derive(Default)]
struct PopupContext {
    selected_text: String,
    source_application: Option<String>,
}

static POPUP_CONTEXT: Mutex<PopupContext> = Mutex::new(PopupContext {
    selected_text: String::new(),
    source_application: None,
});

pub struct PopupWindow;

impl PopupWindow {
    pub fn show<R: Runtime>(
        app: &AppHandle<R>,
        selected_text: &str,
        source_application: Option<String>,
    ) -> Result<(), String> {
        *POPUP_CONTEXT.lock().map_err(|error| error.to_string())? = PopupContext {
            selected_text: selected_text.to_string(),
            source_application,
        };

        let position = popup_position(app)?;
        let window = if let Some(window) = app.get_webview_window(POPUP_LABEL) {
            window
                .set_position(position)
                .map_err(|error| error.to_string())?;
            window.show().map_err(|error| error.to_string())?;
            window
        } else {
            let window = WebviewWindowBuilder::new(
                app,
                POPUP_LABEL,
                WebviewUrl::App("index.html?view=popup".into()),
            )
            .title("shakespAIre")
            .inner_size(POPUP_WIDTH, POPUP_HEIGHT)
            .min_inner_size(340.0, 180.0)
            .position(position.x as f64, position.y as f64)
            .decorations(false)
            .transparent(true)
            .resizable(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .shadow(false)
            .focused(true)
            .build()
            .map_err(|error| error.to_string())?;
            window
                .set_position(position)
                .map_err(|error| error.to_string())?;
            window
        };

        window.set_focus().map_err(|error| error.to_string())?;
        let _ = window.emit("popup-reset", ());
        Ok(())
    }

    pub fn close<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
        if let Some(window) = app.get_webview_window(POPUP_LABEL) {
            window.close().map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn selected_text() -> Result<String, String> {
        POPUP_CONTEXT
            .lock()
            .map(|context| context.selected_text.clone())
            .map_err(|error| error.to_string())
    }

    pub fn source_application() -> Result<Option<String>, String> {
        POPUP_CONTEXT
            .lock()
            .map(|context| context.source_application.clone())
            .map_err(|error| error.to_string())
    }
}

fn popup_position<R: Runtime>(app: &AppHandle<R>) -> Result<PhysicalPosition<i32>, String> {
    let cursor = app.cursor_position().map_err(|error| error.to_string())?;
    let selection = crate::input::selected_text_bounds().ok();
    let monitor = app
        .monitor_from_point(cursor.x, cursor.y)
        .map_err(|error| error.to_string())?;

    if let Some(monitor) = monitor {
        let work_area = monitor.work_area();
        let scale = monitor.scale_factor();
        let (anchor_x, anchor_top, anchor_bottom) = selection
            .map(|bounds| {
                (
                    bounds.x * scale,
                    bounds.y * scale,
                    (bounds.y + bounds.height) * scale,
                )
            })
            .unwrap_or((cursor.x, cursor.y, cursor.y));
        let (x, y) = anchored_position(
            anchor_x,
            anchor_top,
            anchor_bottom,
            work_area.position.x as f64,
            work_area.position.y as f64,
            work_area.size.width as f64,
            work_area.size.height as f64,
            POPUP_WIDTH * scale,
            POPUP_HEIGHT * scale,
        );
        Ok(PhysicalPosition::new(x.round() as i32, y.round() as i32))
    } else {
        Ok(PhysicalPosition::new(
            (cursor.x + POPUP_GAP).round() as i32,
            (cursor.y + POPUP_GAP).round() as i32,
        ))
    }
}

#[allow(clippy::too_many_arguments)]
fn anchored_position(
    anchor_x: f64,
    anchor_top: f64,
    anchor_bottom: f64,
    area_x: f64,
    area_y: f64,
    area_width: f64,
    area_height: f64,
    popup_width: f64,
    popup_height: f64,
) -> (f64, f64) {
    let max_x = area_x + (area_width - popup_width).max(0.0);
    let max_y = area_y + (area_height - popup_height).max(0.0);
    let below = anchor_bottom + POPUP_GAP;
    let y = if below <= max_y {
        below
    } else {
        (anchor_top - POPUP_GAP - popup_height).clamp(area_y, max_y)
    };
    (anchor_x.clamp(area_x, max_x), y)
}

#[cfg(test)]
mod tests {
    use super::anchored_position;

    #[test]
    fn positions_popup_below_and_right_of_cursor() {
        assert_eq!(
            anchored_position(100.0, 80.0, 100.0, 0.0, 0.0, 1440.0, 900.0, 420.0, 240.0),
            (100.0, 108.0)
        );
    }

    #[test]
    fn keeps_popup_inside_monitor_work_area() {
        assert_eq!(
            anchored_position(1400.0, 880.0, 900.0, 0.0, 0.0, 1440.0, 900.0, 420.0, 240.0),
            (1020.0, 632.0)
        );
    }

    #[test]
    fn handles_negative_monitor_coordinates() {
        assert_eq!(
            anchored_position(-100.0, 50.0, 70.0, -1920.0, 0.0, 1920.0, 1080.0, 420.0, 240.0),
            (-420.0, 78.0)
        );
    }
}
