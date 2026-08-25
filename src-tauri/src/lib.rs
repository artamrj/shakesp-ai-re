mod ai;
mod clipboard;
mod input;
mod popup;

use std::{
    fs,
    io::ErrorKind,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use ai::{default_system_prompt, stream_chat, AiConfig};
use clipboard::{restore_clipboard, save_clipboard, write_to_clipboard};
use keyring::{Entry, Error as KeyringError};
use popup::PopupWindow;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tokio_stream::StreamExt;

static AI_CONFIG: OnceLock<Mutex<AiConfig>> = OnceLock::new();
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "gpt-4o-mini";
const KEYCHAIN_SERVICE: &str = "com.artamrj.shakespaire";
const KEYCHAIN_ACCOUNT: &str = "ai-api-key";
const SETTINGS_FILE: &str = "ai-settings.json";
const STREAM_MAX_ATTEMPTS: usize = 3;
const STREAM_RETRY_DELAYS_MS: [u64; STREAM_MAX_ATTEMPTS - 1] = [250, 700];

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamRetryEvent {
    attempt: usize,
    max_attempts: usize,
    message: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedAiConfig {
    base_url: String,
    model: String,
}

fn default_ai_config() -> AiConfig {
    AiConfig {
        base_url: DEFAULT_BASE_URL.to_string(),
        api_key: String::new(),
        model: DEFAULT_MODEL.to_string(),
    }
}

fn ai_config() -> &'static Mutex<AiConfig> {
    AI_CONFIG.get_or_init(|| Mutex::new(default_ai_config()))
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|directory| directory.join(SETTINGS_FILE))
        .map_err(|error| format!("could not resolve settings directory: {error}"))
}

fn keychain_entry() -> Result<Entry, String> {
    Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .map_err(|error| format!("could not access macOS Keychain: {error}"))
}

fn load_ai_config(app: &AppHandle) -> Result<(), String> {
    let mut config = default_ai_config();
    let path = settings_path(app)?;

    match fs::read_to_string(&path) {
        Ok(contents) => match serde_json::from_str::<PersistedAiConfig>(&contents) {
            Ok(saved) => {
                if !saved.base_url.trim().is_empty() {
                    config.base_url = saved.base_url;
                }
                if !saved.model.trim().is_empty() {
                    config.model = saved.model;
                }
            }
            Err(error) => log::warn!("could not parse persisted AI settings: {error}"),
        },
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => log::warn!("could not read persisted AI settings: {error}"),
    }

    match keychain_entry() {
        Ok(entry) => match entry.get_password() {
            Ok(api_key) => config.api_key = api_key,
            Err(KeyringError::NoEntry) => {}
            Err(error) => log::warn!("could not read API key from macOS Keychain: {error}"),
        },
        Err(error) => log::warn!("{error}"),
    }

    // Explicit environment variables remain useful for development and automation.
    if let Ok(value) = std::env::var("OPENAI_BASE_URL") {
        config.base_url = value;
    }
    if let Ok(value) = std::env::var("OPENAI_API_KEY") {
        config.api_key = value;
    }
    if let Ok(value) = std::env::var("OPENAI_MODEL") {
        config.model = value;
    }

    *ai_config().lock().map_err(|error| error.to_string())? = config;
    log::info!("persistent AI configuration loaded");
    Ok(())
}

fn persist_ai_config(app: &AppHandle, config: &AiConfig) -> Result<(), String> {
    let entry = keychain_entry()?;
    if config.api_key.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => {}
            Err(error) => {
                return Err(format!(
                    "could not clear API key from macOS Keychain: {error}"
                ))
            }
        }
    } else {
        entry
            .set_password(&config.api_key)
            .map_err(|error| format!("could not save API key in macOS Keychain: {error}"))?;
    }

    let path = settings_path(app)?;
    let directory = path
        .parent()
        .ok_or_else(|| "settings path has no parent directory".to_string())?;
    fs::create_dir_all(directory)
        .map_err(|error| format!("could not create settings directory: {error}"))?;

    let persisted = PersistedAiConfig {
        base_url: config.base_url.clone(),
        model: config.model.clone(),
    };
    let contents = serde_json::to_vec_pretty(&persisted)
        .map_err(|error| format!("could not encode settings: {error}"))?;
    let temporary_path = path.with_extension("json.tmp");
    fs::write(&temporary_path, contents)
        .map_err(|error| format!("could not write settings: {error}"))?;
    fs::rename(&temporary_path, &path)
        .map_err(|error| format!("could not commit settings: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod persistence_tests {
    use super::PersistedAiConfig;

    #[test]
    fn persisted_settings_never_contain_an_api_key() {
        let settings = PersistedAiConfig {
            base_url: "https://example.com/v1".to_string(),
            model: "example-model".to_string(),
        };
        let json = serde_json::to_string(&settings).expect("settings should serialize");

        assert!(!json.contains("api_key"));
    }
}

#[tauri::command]
fn set_ai_config(app: AppHandle, mut config: AiConfig) -> Result<(), String> {
    if config.base_url.trim().is_empty() {
        return Err("API base URL cannot be empty".to_string());
    }
    if config.model.trim().is_empty() {
        return Err("model cannot be empty".to_string());
    }

    config.base_url = config.base_url.trim().to_string();
    config.model = config.model.trim().to_string();
    persist_ai_config(&app, &config)?;
    *ai_config().lock().map_err(|error| error.to_string())? = config;
    log::info!("AI configuration persisted");
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
    let stream_generation = PopupWindow::begin_stream();
    let config = ai_config()
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let system_prompt = default_system_prompt();
    let mut emitted_content = false;

    'attempts: for attempt in 1..=STREAM_MAX_ATTEMPTS {
        let stream_result = tokio::select! {
            _ = PopupWindow::wait_for_stream_cancel(stream_generation) => {
                log::info!("AI request cancelled before streaming began");
                return Ok(());
            }
            result = stream_chat(&config, &system_prompt, &selected_text) => result,
        };

        let mut stream = match stream_result {
            Ok(stream) => stream,
            Err(error) => {
                if error.is_retryable() && attempt < STREAM_MAX_ATTEMPTS {
                    emit_stream_retry(&app, attempt + 1, error.to_string());
                    if wait_for_retry_or_cancel(stream_generation, attempt).await {
                        continue 'attempts;
                    }
                    return Ok(());
                }
                return surface_stream_error(&app, stream_generation, error.to_string());
            }
        };

        loop {
            let result = tokio::select! {
                _ = PopupWindow::wait_for_stream_cancel(stream_generation) => {
                    // Dropping the receiver wakes sender.closed() in ai.rs, which
                    // immediately drops the response body and its network socket.
                    log::info!("in-flight AI stream cancelled");
                    return Ok(());
                }
                result = stream.next() => result,
            };

            match result {
                Some(Ok(content)) if !content.is_empty() => {
                    emitted_content = true;
                    if let Some(popup) = app.get_webview_window("popup") {
                        let _ = popup.emit("ai-stream-chunk", content);
                    } else {
                        return Ok(());
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(error))
                    if error.is_retryable()
                        && !emitted_content
                        && attempt < STREAM_MAX_ATTEMPTS =>
                {
                    emit_stream_retry(&app, attempt + 1, error.to_string());
                    drop(stream);
                    if wait_for_retry_or_cancel(stream_generation, attempt).await {
                        continue 'attempts;
                    }
                    return Ok(());
                }
                Some(Err(error)) => {
                    return surface_stream_error(&app, stream_generation, error.to_string());
                }
                None => break 'attempts,
            }
        }
    }

    if PopupWindow::stream_is_current(stream_generation) {
        if let Some(popup) = app.get_webview_window("popup") {
            let _ = popup.emit("ai-stream-done", ());
        }
    }
    log::info!("AI stream completed");
    Ok(())
}

fn emit_stream_retry(app: &AppHandle, attempt: usize, message: String) {
    log::warn!("AI stream attempt {} failed: {message}", attempt - 1);
    if let Some(popup) = app.get_webview_window("popup") {
        let _ = popup.emit(
            "ai-stream-retry",
            StreamRetryEvent {
                attempt,
                max_attempts: STREAM_MAX_ATTEMPTS,
                message,
            },
        );
    }
}

async fn wait_for_retry_or_cancel(stream_generation: u64, failed_attempt: usize) -> bool {
    let delay = Duration::from_millis(STREAM_RETRY_DELAYS_MS[failed_attempt - 1]);
    tokio::select! {
        _ = PopupWindow::wait_for_stream_cancel(stream_generation) => false,
        _ = tokio::time::sleep(delay) => true,
    }
}

fn surface_stream_error(
    app: &AppHandle,
    stream_generation: u64,
    error: String,
) -> Result<(), String> {
    log::error!("AI stream error: {error}");
    if PopupWindow::stream_is_current(stream_generation) {
        if let Some(popup) = app.get_webview_window("popup") {
            let _ = popup.emit("ai-stream-error", &error);
        }
    }
    Err(error)
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
    log::info!("M2_E2E_REPORT selected={selected_text:?} output={output_text:?} error={error:?}");
}

fn run_shortcut_flow(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let shortcut_started = Instant::now();
        let source_application = input::frontmost_application().ok();
        let result = async {
            let selected = clipboard::capture_selected_text(&app).await?;
            log::info!(
                "captured {} selected characters in {} ms",
                selected.chars().count(),
                shortcut_started.elapsed().as_millis()
            );
            if let Some(main) = app.get_webview_window("main") {
                main.hide().map_err(|error| error.to_string())?;
            }
            PopupWindow::show(&app, &selected, source_application)?;
            log::info!(
                "popup shown in {} ms after shortcut",
                shortcut_started.elapsed().as_millis()
            );
            Ok::<(), String>(())
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
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventType};

    const MACOS_ESCAPE_KEY_CODE: u16 = 53;

    let popup_app = app.clone();
    let handler = RcBlock::new(move |event: NonNull<NSEvent>| {
        let event = unsafe { event.as_ref() };
        let should_dismiss =
            event.r#type() != NSEventType::KeyDown || event.keyCode() == MACOS_ESCAPE_KEY_CODE;
        if !should_dismiss {
            return;
        }
        if let Some(popup) = popup_app.get_webview_window("popup") {
            if popup.is_visible().unwrap_or(false) {
                let _ = PopupWindow::close(&popup_app);
            }
        }
    });

    let mask = NSEventMask::LeftMouseDown
        | NSEventMask::RightMouseDown
        | NSEventMask::OtherMouseDown
        | NSEventMask::KeyDown;
    let monitor = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &handler);
    if let Some(monitor) = monitor {
        // The monitor should live for the duration of this utility process.
        std::mem::forget(monitor);
        log::info!("global click-away and Escape monitor installed");
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
            load_ai_config(app.handle())?;
            PopupWindow::prepare(app.handle())?;
            setup_global_shortcut(app.handle())?;
            setup_click_away(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
