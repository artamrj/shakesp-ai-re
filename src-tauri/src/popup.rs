use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex, OnceLock,
};
use tokio::sync::watch;

use tauri::{
    window::Color, AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Runtime, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

const POPUP_LABEL: &str = "popup";
const POPUP_WIDTH: f64 = 320.0;
const POPUP_HEIGHT: f64 = 170.0;
const POPUP_MAX_HEIGHT: f64 = 250.0;
const POPUP_GAP: f64 = 8.0;
const POPUP_EDGE_MARGIN: f64 = 10.0;
const AUTO_HIDE_DOCK_CLEARANCE: f64 = 88.0;

#[derive(Default)]
struct PopupContext {
    selected_text: String,
    source_application: Option<String>,
    is_test: bool,
    error: String,
}

static POPUP_CONTEXT: Mutex<PopupContext> = Mutex::new(PopupContext {
    selected_text: String::new(),
    source_application: None,
    is_test: false,
    error: String::new(),
});
static STREAM_GENERATION: AtomicU64 = AtomicU64::new(0);
static STREAM_STATE: OnceLock<watch::Sender<u64>> = OnceLock::new();

fn stream_state() -> &'static watch::Sender<u64> {
    STREAM_STATE.get_or_init(|| {
        let (sender, _) = watch::channel(STREAM_GENERATION.load(Ordering::SeqCst));
        sender
    })
}

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
        Self::show_with_mode(app, selected_text, source_application, false, "")
    }

    pub fn show_test<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
        Self::show_with_mode(
            app,
            "i has a apple and ther going to the stor tomorow, it be a good day.",
            None,
            true,
            "",
        )
    }

    pub fn show_error<R: Runtime>(app: &AppHandle<R>, error: &str) -> Result<(), String> {
        Self::show_with_mode(app, "", None, false, error)
    }

    fn show_with_mode<R: Runtime>(
        app: &AppHandle<R>,
        selected_text: &str,
        source_application: Option<String>,
        is_test: bool,
        error: &str,
    ) -> Result<(), String> {
        *POPUP_CONTEXT.lock().map_err(|error| error.to_string())? = PopupContext {
            selected_text: selected_text.to_string(),
            source_application,
            is_test,
            error: error.to_string(),
        };
        Self::cancel_stream();

        let position = popup_position(app)?;
        let window = if let Some(window) = app.get_webview_window(POPUP_LABEL) {
            window
                .set_focusable(true)
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

        // Native backdrop effects can be dropped while a prewarmed window is hidden.
        // Reapply them after every show so Windows composition is reliable.
        apply_glass(&window)?;
        // The popup must be the key window on every platform, or its Enter/Copy/Esc
        // keys never reach the page.
        window.set_focus().map_err(|error| error.to_string())?;
        // Focusing the window alone can leave the web view without keyboard focus after the
        // window was hidden and shown again, so Enter/Copy/Esc would go nowhere.
        if let Err(error) = AsRef::<tauri::Webview<R>>::as_ref(&window).set_focus() {
            log::warn!("could not focus the popup web view: {error}");
        }
        let _ = window.emit("popup-reset", ());
        Ok(())
    }

    /// Grows the popup a little so longer results need less scrolling, keeping
    /// it inside the monitor's work area.
    pub fn resize<R: Runtime>(app: &AppHandle<R>, height: f64) -> Result<(), String> {
        let window = app
            .get_webview_window(POPUP_LABEL)
            .ok_or_else(|| "popup window is not available".to_string())?;
        let height = height.clamp(POPUP_HEIGHT, POPUP_MAX_HEIGHT);
        window
            .set_size(LogicalSize::new(POPUP_WIDTH, height))
            .map_err(|error| error.to_string())?;

        if let (Ok(Some(monitor)), Ok(position)) =
            (window.current_monitor(), window.outer_position())
        {
            let scale = monitor.scale_factor();
            let work_area = monitor.work_area();
            let top = work_area.position.y as f64;
            let limit = top + work_area.size.height as f64 - POPUP_EDGE_MARGIN * scale;
            let physical_height = height * scale;
            if position.y as f64 + physical_height > limit {
                let y = (limit - physical_height).max(top);
                let _ = window.set_position(PhysicalPosition::new(position.x, y.round() as i32));
            }
        }
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
        let generation = STREAM_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        stream_state().send_replace(generation);
        generation
    }

    pub fn stream_is_current(generation: u64) -> bool {
        STREAM_GENERATION.load(Ordering::SeqCst) == generation
    }

    pub async fn wait_for_stream_cancel(generation: u64) {
        let mut receiver = stream_state().subscribe();
        loop {
            if *receiver.borrow_and_update() != generation {
                return;
            }
            if receiver.changed().await.is_err() {
                return;
            }
        }
    }

    fn cancel_stream() {
        let generation = STREAM_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        stream_state().send_replace(generation);
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

    pub fn is_test() -> Result<bool, String> {
        POPUP_CONTEXT
            .lock()
            .map(|context| context.is_test)
            .map_err(|error| error.to_string())
    }

    pub fn error() -> Result<String, String> {
        POPUP_CONTEXT
            .lock()
            .map(|context| context.error.clone())
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
        WebviewUrl::App(format!("index.html?view=popup&platform={}", std::env::consts::OS).into()),
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
    .focusable(true)
    .focused(false)
    .visible(visible)
    .build()
    .map_err(|error| error.to_string())?;

    window
        .set_position(position)
        .map_err(|error| error.to_string())?;
    install_focus_loss_handler(app, &window);
    apply_glass(&window)?;
    Ok(window)
}

fn install_focus_loss_handler<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let popup_app = app.clone();
    let popup = window.clone();
    window.on_window_event(move |event| {
        if !matches!(event, tauri::WindowEvent::Focused(false)) {
            return;
        }

        // Focus events around show/set_focus can arrive out of order. Recheck after
        // a short grace period and dismiss only a visible popup that stayed unfocused.
        let popup_app = popup_app.clone();
        let popup = popup.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(60)).await;
            if popup.is_visible().unwrap_or(false) && !popup.is_focused().unwrap_or(false) {
                let _ = PopupWindow::close(&popup_app);
            }
        });
    });
}

fn apply_glass<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    crate::glass::apply(window, 16.0)
}

fn popup_position<R: Runtime>(app: &AppHandle<R>) -> Result<PhysicalPosition<i32>, String> {
    let cursor = app
        .cursor_position()
        .map_err(|error| {
            log::warn!("global cursor position unavailable; using monitor fallback: {error}");
            error
        })
        .ok();
    let selection = match crate::input::selected_text_bounds() {
        Ok(bounds) => {
            log::info!("selection bounds: {bounds:?}");
            Some(bounds)
        }
        Err(error) => {
            log::info!("no selection bounds ({error}); anchoring to the mouse cursor");
            None
        }
    };
    let cursor_monitor = cursor.and_then(|position| {
        app.monitor_from_point(position.x, position.y)
            .map_err(|error| log::warn!("could not resolve monitor at cursor: {error}"))
            .ok()
            .flatten()
    });
    let window_monitor = app
        .get_webview_window("main")
        .and_then(|window| window.current_monitor().ok().flatten());
    let monitor = cursor_monitor
        .or(window_monitor)
        .or_else(|| app.primary_monitor().ok().flatten());

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
        let monitor_left = monitor_position.x as f64;
        let monitor_top = monitor_position.y as f64;
        let anchor = selection
            .map(|bounds| {
                (
                    bounds.x * scale,
                    bounds.y * scale,
                    (bounds.y + bounds.height) * scale,
                )
            })
            // Some apps report placeholder rectangles (e.g. x = 0 at the screen
            // edge). Only trust bounds that actually sit on this monitor.
            .filter(|&(x, top, bottom)| {
                let inside = x > monitor_left
                    && x < monitor_left + monitor_size.width as f64
                    && top >= monitor_top
                    && bottom <= monitor_top + monitor_size.height as f64;
                if !inside {
                    log::info!("ignoring off-monitor selection bounds; using the mouse cursor");
                }
                inside
            })
            .or_else(|| cursor.map(|position| (position.x, position.y, position.y)));
        if anchor.is_none() {
            return Ok(PhysicalPosition::new(
                (area_x + (area_width - POPUP_WIDTH * scale).max(0.0) / 2.0).round() as i32,
                (area_y + (area_height - POPUP_HEIGHT * scale).max(0.0) / 2.0).round() as i32,
            ));
        }
        let (anchor_x, anchor_top, anchor_bottom) = anchor.expect("anchor checked above");
        log::info!(
            "popup anchor ({anchor_x:.0}, {anchor_top:.0}-{anchor_bottom:.0}) scale {scale} cursor {cursor:?}"
        );
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
        let cursor = cursor.unwrap_or(PhysicalPosition::new(40.0, 40.0));
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
