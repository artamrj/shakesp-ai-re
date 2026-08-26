mod ai;
mod clipboard;
mod input;
mod popup;

use std::{
    fs,
    io::ErrorKind,
    path::PathBuf,
    str::FromStr,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use ai::{default_system_prompt, stream_chat, AiConfig};
use clipboard::{restore_clipboard, save_clipboard, write_to_clipboard};
use keyring::{Entry, Error as KeyringError};
use popup::PopupWindow;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tokio_stream::StreamExt;

static AI_CONFIG: OnceLock<Mutex<AiConfig>> = OnceLock::new();
static ACTIVE_SHORTCUT: OnceLock<Mutex<String>> = OnceLock::new();
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "gpt-4o-mini";
const KEYCHAIN_SERVICE: &str = "com.artamrj.shakespaire";
const KEYCHAIN_ACCOUNT: &str = "ai-api-key";
const SETTINGS_FILE: &str = "ai-settings.json";
const FALLBACK_KEY_FILE: &str = "ai-api-key.txt";
const STREAM_MAX_ATTEMPTS: usize = 3;
const STREAM_RETRY_DELAYS_MS: [u64; STREAM_MAX_ATTEMPTS - 1] = [250, 700];

/// Human-readable name of the platform credential store used in log/user messages.
fn credential_store_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS Keychain"
    } else if cfg!(target_os = "windows") {
        "Windows Credential Manager"
    } else {
        "Secret Service (keyring)"
    }
}

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
    #[serde(default = "default_shortcut")]
    shortcut: String,
}

fn default_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Shift+Super+Space".to_string()
    } else {
        "Shift+Control+Space".to_string()
    }
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

fn active_shortcut() -> &'static Mutex<String> {
    ACTIVE_SHORTCUT.get_or_init(|| Mutex::new(default_shortcut()))
}

fn parse_shortcut(value: &str) -> Result<Shortcut, String> {
    let shortcut = Shortcut::from_str(value.trim()).map_err(|_| {
        "Press a modifier and one other key (for example, Command + Shift + Space).".to_string()
    })?;
    if shortcut.mods.is_empty() {
        return Err("The shortcut must include Command, Control, Option, or Shift.".to_string());
    }
    Ok(shortcut)
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|directory| directory.join(SETTINGS_FILE))
        .map_err(|error| format!("could not resolve settings directory: {error}"))
}

fn keychain_entry() -> Result<Entry, String> {
    Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .map_err(|error| format!("could not access {}: {error}", credential_store_name()))
}

fn fallback_key_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|directory| directory.join(FALLBACK_KEY_FILE))
        .map_err(|error| format!("could not resolve fallback key directory: {error}"))
}

/// Reads the API key from the encrypted platform keychain, falling back to a
/// local file when the keychain is unavailable (common on Linux without
/// gnome-keyring/kwallet and on some Windows configurations).
fn load_api_key(app: &AppHandle) -> String {
    if let Ok(entry) = keychain_entry() {
        match entry.get_password() {
            Ok(api_key) => return api_key,
            Err(KeyringError::NoEntry) => {}
            Err(error) => log::warn!(
                "could not read API key from {}: {error}",
                credential_store_name()
            ),
        }
    }

    // Fallback: read from a local file in the app config directory.
    if let Ok(path) = fallback_key_path(app) {
        if let Ok(api_key) = fs::read_to_string(&path) {
            log::info!(
                "loaded API key from fallback file ({} unavailable)",
                credential_store_name()
            );
            return api_key.trim().to_string();
        }
    }
    String::new()
}

/// Persists the API key to the platform keychain, falling back to a local
/// file when the keychain is unavailable. Returns Ok(()) as long as the key
/// is stored in at least one location.
fn save_api_key(app: &AppHandle, api_key: &str) -> Result<(), String> {
    if api_key.is_empty() {
        // Best-effort deletion from both stores; ignore failures.
        if let Ok(entry) = keychain_entry() {
            match entry.delete_credential() {
                Ok(()) | Err(KeyringError::NoEntry) => {}
                Err(error) => log::warn!(
                    "could not clear API key from {}: {error}",
                    credential_store_name()
                ),
            }
        }
        if let Ok(path) = fallback_key_path(app) {
            let _ = fs::remove_file(&path);
        }
        return Ok(());
    }

    // Try the keychain first.
    let keychain_ok = match keychain_entry() {
        Ok(entry) => match entry.set_password(api_key) {
            Ok(()) => true,
            Err(error) => {
                log::warn!(
                    "could not save API key to {}: {error}; using fallback file",
                    credential_store_name()
                );
                false
            }
        },
        Err(error) => {
            log::warn!("{error}; using fallback file");
            false
        }
    };

    if keychain_ok {
        // Clean up any stale fallback file so the keychain is the source of truth.
        if let Ok(path) = fallback_key_path(app) {
            let _ = fs::remove_file(&path);
        }
        return Ok(());
    }

    // Fallback: write to a local file in the app config directory.
    let path = fallback_key_path(app)?;
    let directory = path
        .parent()
        .ok_or_else(|| "fallback key path has no parent directory".to_string())?;
    fs::create_dir_all(directory)
        .map_err(|error| format!("could not create fallback key directory: {error}"))?;

    // Write atomically to avoid corrupting the key on crash.
    let temporary_path = path.with_extension("txt.tmp");
    fs::write(&temporary_path, api_key)
        .map_err(|error| format!("could not write fallback API key: {error}"))?;
    fs::rename(&temporary_path, &path)
        .map_err(|error| format!("could not commit fallback API key: {error}"))?;

    log::info!(
        "API key saved to fallback file ({} unavailable)",
        credential_store_name()
    );
    Ok(())
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
                match parse_shortcut(&saved.shortcut) {
                    Ok(shortcut) => {
                        *active_shortcut()
                            .lock()
                            .map_err(|error| error.to_string())? = shortcut.to_string();
                    }
                    Err(error) => log::warn!("ignoring invalid saved shortcut: {error}"),
                }
            }
            Err(error) => log::warn!("could not parse persisted AI settings: {error}"),
        },
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => log::warn!("could not read persisted AI settings: {error}"),
    }

    // Try the platform keychain first, then a local fallback file.
    config.api_key = load_api_key(app);

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
    // Store the API key in the platform keychain with a local file fallback.
    // This never fails as long as we can write to the config directory, so the
    // app remains usable on Linux without gnome-keyring/kwallet and on Windows
    // configurations where the Credential Manager is locked down.
    save_api_key(app, &config.api_key)?;

    let path = settings_path(app)?;
    let directory = path
        .parent()
        .ok_or_else(|| "settings path has no parent directory".to_string())?;
    fs::create_dir_all(directory)
        .map_err(|error| format!("could not create settings directory: {error}"))?;

    let persisted = PersistedAiConfig {
        base_url: config.base_url.clone(),
        model: config.model.clone(),
        shortcut: active_shortcut()
            .lock()
            .map_err(|error| error.to_string())?
            .clone(),
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
    use super::{default_shortcut, parse_shortcut, PersistedAiConfig};

    #[test]
    fn persisted_settings_never_contain_an_api_key() {
        let settings = PersistedAiConfig {
            base_url: "https://example.com/v1".to_string(),
            model: "example-model".to_string(),
            shortcut: "Control+Shift+Space".to_string(),
        };
        let json = serde_json::to_string(&settings).expect("settings should serialize");

        assert!(!json.contains("api_key"));
    }

    #[test]
    fn old_settings_receive_the_default_shortcut() {
        let settings: PersistedAiConfig = serde_json::from_str(
            r#"{"base_url":"https://example.com/v1","model":"example-model"}"#,
        )
        .expect("old settings should remain compatible");

        assert_eq!(settings.shortcut, default_shortcut());
    }

    #[test]
    fn shortcut_requires_a_modifier() {
        assert!(parse_shortcut("KeyK").is_err());
        assert!(parse_shortcut("Control+KeyK").is_ok());
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
fn get_shortcut() -> Result<String, String> {
    active_shortcut()
        .lock()
        .map(|shortcut| shortcut.clone())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_shortcut(app: AppHandle, shortcut: String) -> Result<String, String> {
    let new_shortcut = parse_shortcut(&shortcut)?;
    let canonical = new_shortcut.to_string();
    let previous = active_shortcut()
        .lock()
        .map_err(|error| error.to_string())?
        .clone();

    if canonical == previous {
        return Ok(canonical);
    }

    register_global_shortcut(&app, new_shortcut).map_err(|error| {
        format!(
            "That shortcut could not be registered. It may already be used by another app: {error}"
        )
    })?;

    if let Err(error) = app.global_shortcut().unregister(previous.as_str()) {
        if app.global_shortcut().is_registered(previous.as_str()) {
            let _ = app.global_shortcut().unregister(new_shortcut);
            return Err(format!("Could not replace the current shortcut: {error}"));
        }
        log::warn!("previous shortcut was not registered: {error}");
    }

    *active_shortcut()
        .lock()
        .map_err(|error| error.to_string())? = canonical.clone();
    let config = ai_config()
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    if let Err(error) = persist_ai_config(&app, &config) {
        let _ = app.global_shortcut().unregister(new_shortcut);
        if let Ok(old_shortcut) = parse_shortcut(&previous) {
            let _ = register_global_shortcut(&app, old_shortcut);
        }
        *active_shortcut()
            .lock()
            .map_err(|lock_error| lock_error.to_string())? = previous;
        return Err(format!("Could not save the shortcut: {error}"));
    }

    log::info!("global shortcut changed to {canonical}");
    Ok(canonical)
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
fn get_popup_test_mode() -> Result<bool, String> {
    PopupWindow::is_test()
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
fn test_popup(app: AppHandle) -> Result<(), String> {
    PopupWindow::show_test(&app)
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

fn register_global_shortcut(app: &AppHandle, shortcut: Shortcut) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(shortcut, move |app, _shortcut, event| {
            if event.state() == ShortcutState::Released {
                log::info!("global shortcut triggered");
                run_shortcut_flow(app.clone());
            }
        })
        .map_err(|error| error.to_string())?;

    Ok(())
}

fn setup_global_shortcut(app: &AppHandle) -> Result<(), String> {
    let configured = active_shortcut()
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let shortcut = parse_shortcut(&configured)?;
    register_global_shortcut(app, shortcut)?;

    log::info!("global shortcut registered: {configured}");
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

/// Initialises `env_logger` and additionally tees log output to a file in the
/// user's data directory so crashes can be diagnosed when the app exits
/// before the window appears (common on Windows/Linux). Falls back to
/// stderr-only if the log directory cannot be resolved or written to.
fn init_logger() {
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    builder.format_timestamp_secs();

    // Try to also write logs to a file for post-mortem debugging on Windows/Linux.
    // `data_local_dir` is appropriate for log files on all platforms.
    let log_dir = dirs_next::data_local_dir()
        .or_else(dirs_next::config_dir)
        .map(|d| d.join("shakespaire").join("logs"));

    if let Some(log_dir) = log_dir {
        let _ = fs::create_dir_all(&log_dir);
        let log_path = log_dir.join("shakespaire.log");
        match fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            Ok(file) => {
                builder.target(env_logger::Target::Pipe(Box::new(file)));
                eprintln!("logging to {}", log_path.display());
            }
            Err(error) => {
                eprintln!("could not open log file {}: {error}", log_path.display());
            }
        }
    }

    builder.init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logger();
    log::info!("shakespAIre starting on {}", std::env::consts::OS);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            set_ai_config,
            get_ai_config,
            get_shortcut,
            set_shortcut,
            capture_selected_text,
            get_popup_selection,
            get_popup_test_mode,
            replace_text,
            stream_ai_text,
            show_popup,
            test_popup,
            close_popup,
            debug_e2e_enabled,
            debug_trigger_shortcut,
            debug_e2e_report,
        ])
        .setup(|app| {
            // Loading config is essential but already logs warnings on failure
            // and never returns an error for keychain/file issues, so the `?`
            // here only surfaces genuinely fatal config problems.
            load_ai_config(app.handle())?;

            // Prewarming the popup window and registering the global shortcut
            // are best-effort: they commonly fail on Linux (no keyring, X11 vs
            // Wayland) and on Windows (WebView2 issues, permissions). The app
            // must still open the settings window so the user can troubleshoot.
            if let Err(error) = PopupWindow::prepare(app.handle()) {
                log::warn!("could not prewarm popup window: {error}");
            }
            if let Err(error) = setup_global_shortcut(app.handle()) {
                log::warn!("could not register global shortcut: {error}");
            }
            setup_click_away(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
