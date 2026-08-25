use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};

use tauri::{
    window::Color, AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Runtime, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

const POPUP_LABEL: &str = "popup";
const POPUP_WIDTH: f64 = 320.0;
const POPUP_HEIGHT: f64 = 170.0;
const POPUP_GAP: f64 = 8.0;
const POPUP_EDGE_MARGIN: f64 = 10.0;
const AUTO_HIDE_DOCK_CLEARANCE: f64 = 88.0;

#[derive(Default)]
struct PopupContext {
    selected_text: String,
    source_application: Option<String>,
}

static POPUP_CONTEXT: Mutex<PopupContext> = Mutex::new(PopupContext {
    selected_text: String::new(),
    source_application: None,
});
static STREAM_GENERATION: AtomicU64 = AtomicU64::new(0);

pub struct PopupWindow;

impl PopupWindow {
    pub fn prepare<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
        if app.get_webview_window(POPUP_LABEL).is_none() {
            create_window(app, PhysicalPosition::new(0, 0), false)?;
            log::info!("popup webview prewarmed");
        }
        Ok(())
    }

    pub fn show<R: Runtime>(
        app: &AppHandle<R>,
        selected_text: &str,
        source_application: Option<String>,
    ) -> Result<(), String> {
        *POPUP_CONTEXT.lock().map_err(|error| error.to_string())? = PopupContext {
            selected_text: selected_text.to_string(),
            source_application,
        };
        Self::cancel_stream();

        let position = popup_position(app)?;
        let window = if let Some(window) = app.get_webview_window(POPUP_LABEL) {
            window
                .set_focusable(false)
                .map_err(|error| error.to_string())?;
            window
                .set_resizable(false)
                .map_err(|error| error.to_string())?;
            window
                .set_size(LogicalSize::new(POPUP_WIDTH, POPUP_HEIGHT))
                .map_err(|error| error.to_string())?;
            window
                .set_position(position)
                .map_err(|error| error.to_string())?;
            window.show().map_err(|error| error.to_string())?;
            window
        } else {
            create_window(app, position, true)?
        };

        let _ = window.emit("popup-reset", ());
        Ok(())
    }

    pub fn close<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
        Self::cancel_stream();
        if let Some(window) = app.get_webview_window(POPUP_LABEL) {
            window.hide().map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn begin_stream() -> u64 {
        STREAM_GENERATION.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn stream_is_current(generation: u64) -> bool {
        STREAM_GENERATION.load(Ordering::SeqCst) == generation
    }

    fn cancel_stream() {
        STREAM_GENERATION.fetch_add(1, Ordering::SeqCst);
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

fn create_window<R: Runtime>(
    app: &AppHandle<R>,
    position: PhysicalPosition<i32>,
    visible: bool,
) -> Result<WebviewWindow<R>, String> {
    let window = WebviewWindowBuilder::new(
        app,
        POPUP_LABEL,
        WebviewUrl::App("index.html?view=popup".into()),
    )
    .title("shakespAIre")
    .inner_size(POPUP_WIDTH, POPUP_HEIGHT)
    .min_inner_size(280.0, 145.0)
    .position(position.x as f64, position.y as f64)
    .decorations(false)
    .transparent(true)
    .background_color(Color(0, 0, 0, 0))
    .resizable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(true)
    .focusable(false)
    .focused(false)
    .visible(visible)
    .build()
    .map_err(|error| error.to_string())?;

    window
        .set_position(position)
        .map_err(|error| error.to_string())?;
    apply_glass(&window)?;
    Ok(window)
}

#[cfg(target_os = "macos")]
fn apply_glass<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    let effect_window = window.clone();
    window
        .with_webview(move |webview| unsafe {
            use objc2_app_kit::NSView;
            use window_vibrancy::{
                LiquidGlassOptions, NSGlassEffectViewStyle, NSVisualEffectMaterial,
                NSVisualEffectState,
            };

            let content_view: &NSView = &*webview.inner().cast();
            let _ = window_vibrancy::clear_liquid_glass(&effect_window);
            let _ = window_vibrancy::clear_vibrancy(&effect_window);

            let options = LiquidGlassOptions::new(NSGlassEffectViewStyle::Clear)
                .radius(16.0)
                .opaque(false)
                .content_view(content_view);

            if let Err(glass_error) = window_vibrancy::apply_liquid_glass(&effect_window, options) {
                log::info!(
                    "native Liquid Glass unavailable ({glass_error}); using vibrancy fallback"
                );
                if let Err(vibrancy_error) = window_vibrancy::apply_vibrancy(
                    &effect_window,
                    NSVisualEffectMaterial::Popover,
                    Some(NSVisualEffectState::Active),
                    Some(16.0),
                ) {
                    log::warn!("could not apply popup glass fallback: {vibrancy_error}");
                }
            }
        })
        .map_err(|error| error.to_string())
}

#[cfg(not(target_os = "macos"))]
fn apply_glass<R: Runtime>(_window: &WebviewWindow<R>) -> Result<(), String> {
    Ok(())
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
        let monitor_position = monitor.position();
        let monitor_size = monitor.size();
        let monitor_bottom = monitor_position.y as f64 + monitor_size.height as f64;
        let (area_x, area_y, area_width, area_height) = adjusted_work_area(
            work_area.position.x as f64,
            work_area.position.y as f64,
            work_area.size.width as f64,
            work_area.size.height as f64,
            monitor_bottom,
            scale,
        );
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
            area_x,
            area_y,
            area_width,
            area_height,
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

fn adjusted_work_area(
    area_x: f64,
    area_y: f64,
    area_width: f64,
    area_height: f64,
    monitor_bottom: f64,
    scale: f64,
) -> (f64, f64, f64, f64) {
    let margin = POPUP_EDGE_MARGIN * scale;
    let work_area_bottom = area_y + area_height;
    let reaches_screen_bottom = (monitor_bottom - work_area_bottom).abs() <= 4.0 * scale;
    let dock_clearance = if reaches_screen_bottom {
        AUTO_HIDE_DOCK_CLEARANCE * scale
    } else {
        0.0
    };

    (
        area_x + margin,
        area_y + margin,
        (area_width - margin * 2.0).max(0.0),
        (area_height - margin * 2.0 - dock_clearance).max(0.0),
    )
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
    use super::{adjusted_work_area, anchored_position};

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

    #[test]
    fn reserves_space_for_an_auto_hidden_dock() {
        assert_eq!(
            adjusted_work_area(0.0, 0.0, 1440.0, 900.0, 900.0, 1.0),
            (10.0, 10.0, 1420.0, 792.0)
        );
    }

    #[test]
    fn does_not_double_reserve_space_for_a_visible_dock() {
        assert_eq!(
            adjusted_work_area(0.0, 24.0, 1440.0, 796.0, 900.0, 1.0),
            (10.0, 34.0, 1420.0, 776.0)
        );
    }
}
