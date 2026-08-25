mod ai;
mod clipboard;
mod input;
mod popup;

use std::sync::{Mutex, OnceLock};

use ai::{default_system_prompt, stream_chat, AiConfig};
use clipboard::{restore_clipboard, save_clipboard, write_to_clipboard};
use popup::PopupWindow;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tokio_stream::StreamExt;

static AI_CONFIG: OnceLock<Mutex<AiConfig>> = OnceLock::new();

fn ai_config() -> &'static Mutex<AiConfig> {
    AI_CONFIG.get_or_init(|| {
        Mutex::new(AiConfig {
            base_url: std::env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
            model: std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string()),
        })
    })
}

#[tauri::command]
fn set_ai_config(config: AiConfig) -> Result<(), String> {
    if config.base_url.trim().is_empty() {
        return Err("API base URL cannot be empty".to_string());
    }
    if config.model.trim().is_empty() {
        return Err("model cannot be empty".to_string());
    }

    *ai_config().lock().map_err(|error| error.to_string())? = config;
    log::info!("AI configuration updated");
    Ok(())
}

#[tauri::command]
fn get_ai_config() -> Result<AiConfig, String> {
    ai_config()
        .lock()
        .map(|config| config.clone())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn capture_selected_text(app: AppHandle) -> Result<String, String> {
    clipboard::capture_selected_text(&app).await
}

#[tauri::command]
fn get_popup_selection() -> Result<String, String> {
    PopupWindow::selected_text()
}

#[tauri::command]
async fn replace_text(app: AppHandle, text: String) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("replacement text is empty".to_string());
    }

    save_clipboard(&app)?;
    let source_application = PopupWindow::source_application()?;
    let paste_result = async {
        PopupWindow::close(&app)?;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if let Some(source_application) = source_application {
            input::activate_application(&source_application)?;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        write_to_clipboard(&app, &text)?;
        tokio::time::sleep(tokio::time::Duration::from_millis(40)).await;
        input::simulate_paste()?;
        tokio::time::sleep(tokio::time::Duration::from_millis(180)).await;
        Ok::<(), String>(())
    }
    .await;

    let restore_result = restore_clipboard(&app);
    match (paste_result, restore_result) {
        (Ok(()), Ok(())) => {
            log::info!("replacement pasted and clipboard restored");
            Ok(())
        }
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(format!(
            "pasted text but failed to restore clipboard: {error}"
        )),
        (Err(paste_error), Err(restore_error)) => Err(format!(
            "{paste_error}; also failed to restore clipboard: {restore_error}"
        )),
    }
}

#[tauri::command]
async fn stream_ai_text(app: AppHandle, selected_text: String) -> Result<(), String> {
    let config = ai_config()
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let mut stream = stream_chat(&config, &default_system_prompt(), &selected_text).await?;

    while let Some(result) = stream.next().await {
        match result {
            Ok(content) if !content.is_empty() => {
                if let Some(popup) = app.get_webview_window("popup") {
                    let _ = popup.emit("ai-stream-chunk", content);
                } else {
                    break;
                }
            }
            Ok(_) => {}
            Err(error) => {
                log::error!("AI stream error: {error}");
                if let Some(popup) = app.get_webview_window("popup") {
                    let _ = popup.emit("ai-stream-error", &error);
                }
                return Err(error);
            }
        }
    }

    if let Some(popup) = app.get_webview_window("popup") {
        let _ = popup.emit("ai-stream-done", ());
    }
    log::info!("AI stream completed");
    Ok(())
}

#[tauri::command]
fn show_popup(app: AppHandle, selected_text: String) -> Result<(), String> {
    let source_application = input::frontmost_application().ok();
    PopupWindow::show(&app, &selected_text, source_application)
}

#[tauri::command]
fn close_popup(app: AppHandle) -> Result<(), String> {
    PopupWindow::close(&app)
}

#[tauri::command]
fn debug_e2e_enabled() -> bool {
    cfg!(debug_assertions) && std::env::var("SHAKESPAIRE_E2E").as_deref() == Ok("1")
}

#[tauri::command]
fn debug_trigger_shortcut(app: AppHandle) -> Result<(), String> {
    if !debug_e2e_enabled() {
        return Err("debug E2E mode is disabled".to_string());
    }
    if let Some(main) = app.get_webview_window("main") {
        main.set_focus().map_err(|error| error.to_string())?;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        run_shortcut_flow(app);
    });
    Ok(())
}

#[tauri::command]
fn debug_e2e_report(selected_text: String, output_text: String, error: String) {
    log::info!(
        "M2_E2E_REPORT selected={selected_text:?} output={output_text:?} error={error:?}"
    );
}

fn run_shortcut_flow(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let source_application = input::frontmost_application().ok();
        let result = async {
            let selected = clipboard::capture_selected_text(&app).await?;
            log::info!("captured {} selected characters", selected.chars().count());
            if let Some(main) = app.get_webview_window("main") {
                main.hide().map_err(|error| error.to_string())?;
            }
            PopupWindow::show(&app, &selected, source_application)
        }
        .await;

        match result {
            Ok(()) => {
                let _ = app.emit("shortcut-triggered", ());
            }
            Err(error) => {
                log::error!("shortcut flow failed: {error}");
                let _ = app.emit("shortcut-error", &error);
            }
        }
    });
}

fn setup_global_shortcut(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space);
    app.global_shortcut()
        .on_shortcut(shortcut, move |app, _shortcut, event| {
            if event.state() == ShortcutState::Released {
                log::info!("global shortcut triggered");
                run_shortcut_flow(app.clone());
            }
        })?;

    log::info!("global shortcut registered: Cmd+Shift+Space");
    Ok(())
}

#[cfg(target_os = "macos")]
fn setup_click_away(app: &AppHandle) {
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2_app_kit::{NSEvent, NSEventMask};

    let popup_app = app.clone();
    let handler = RcBlock::new(move |_event: NonNull<NSEvent>| {
        if popup_app.get_webview_window("popup").is_some() {
            let _ = PopupWindow::close(&popup_app);
        }
    });

    let mask = NSEventMask::LeftMouseDown
        | NSEventMask::RightMouseDown
        | NSEventMask::OtherMouseDown;
    let monitor = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &handler);
    if let Some(monitor) = monitor {
        // The monitor should live for the duration of this utility process.
        std::mem::forget(monitor);
        log::info!("global click-away monitor installed");
    } else {
        log::warn!("could not install global click-away monitor");
    }
}

#[cfg(not(target_os = "macos"))]
fn setup_click_away(_app: &AppHandle) {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("shakespAIre starting");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            set_ai_config,
            get_ai_config,
            capture_selected_text,
            get_popup_selection,
            replace_text,
            stream_ai_text,
            show_popup,
            close_popup,
            debug_e2e_enabled,
            debug_trigger_shortcut,
            debug_e2e_report,
        ])
        .setup(|app| {
            setup_global_shortcut(app.handle())?;
            setup_click_away(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
