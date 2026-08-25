use std::sync::Mutex;

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindowBuilder,
};

const POPUP_LABEL: &str = "popup";
const POPUP_WIDTH: f64 = 520.0;
const POPUP_HEIGHT: f64 = 360.0;
const POPUP_GAP: f64 = 16.0;

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
            .min_inner_size(360.0, 220.0)
            .position(position.x as f64, position.y as f64)
            .decorations(false)
            .resizable(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .shadow(true)
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
    let monitor = app
        .monitor_from_point(cursor.x, cursor.y)
        .map_err(|error| error.to_string())?;

    if let Some(monitor) = monitor {
        let work_area = monitor.work_area();
        let scale = monitor.scale_factor();
        let (x, y) = clamped_position(
            cursor.x,
            cursor.y,
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
fn clamped_position(
    cursor_x: f64,
    cursor_y: f64,
    area_x: f64,
    area_y: f64,
    area_width: f64,
    area_height: f64,
    popup_width: f64,
    popup_height: f64,
) -> (f64, f64) {
    let max_x = area_x + (area_width - popup_width).max(0.0);
    let max_y = area_y + (area_height - popup_height).max(0.0);
    (
        (cursor_x + POPUP_GAP).clamp(area_x, max_x),
        (cursor_y + POPUP_GAP).clamp(area_y, max_y),
    )
}

#[cfg(test)]
mod tests {
    use super::clamped_position;

    #[test]
    fn positions_popup_below_and_right_of_cursor() {
        assert_eq!(
            clamped_position(100.0, 80.0, 0.0, 0.0, 1440.0, 900.0, 520.0, 360.0),
            (116.0, 96.0)
        );
    }

    #[test]
    fn keeps_popup_inside_monitor_work_area() {
        assert_eq!(
            clamped_position(1400.0, 880.0, 0.0, 0.0, 1440.0, 900.0, 520.0, 360.0),
            (920.0, 540.0)
        );
    }

    #[test]
    fn handles_negative_monitor_coordinates() {
        assert_eq!(
            clamped_position(-100.0, 50.0, -1920.0, 0.0, 1920.0, 1080.0, 520.0, 360.0),
            (-520.0, 66.0)
        );
    }
}
