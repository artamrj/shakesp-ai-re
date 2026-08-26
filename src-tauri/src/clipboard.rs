use std::sync::Mutex;

use tauri::{image::Image, AppHandle, Runtime};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::input;

enum SavedClipboard {
    Text(String),
    Image {
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    },
    Empty,
}

static SAVED_CLIPBOARD: Mutex<Option<SavedClipboard>> = Mutex::new(None);

pub fn save_clipboard<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let saved = match app.clipboard().read_text() {
        Ok(text) => SavedClipboard::Text(text),
        Err(_) => match app.clipboard().read_image() {
            Ok(image) => SavedClipboard::Image {
                rgba: image.rgba().to_vec(),
                width: image.width(),
                height: image.height(),
            },
            Err(_) => SavedClipboard::Empty,
        },
    };

    *SAVED_CLIPBOARD.lock().map_err(|error| error.to_string())? = Some(saved);
    Ok(())
}

pub fn restore_clipboard<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let saved = SAVED_CLIPBOARD
        .lock()
        .map_err(|error| error.to_string())?
        .take();

    match saved {
        Some(SavedClipboard::Text(text)) => app
            .clipboard()
            .write_text(text)
            .map_err(|error| error.to_string()),
        Some(SavedClipboard::Image {
            rgba,
            width,
            height,
        }) => app
            .clipboard()
            .write_image(&Image::new_owned(rgba, width, height))
            .map_err(|error| error.to_string()),
        Some(SavedClipboard::Empty) => app.clipboard().clear().map_err(|error| error.to_string()),
        None => Ok(()),
    }
}

pub fn read_current_clipboard<R: Runtime>(app: &AppHandle<R>) -> Result<String, String> {
    app.clipboard()
        .read_text()
        .map_err(|error| format!("clipboard does not contain text: {error}"))
}

pub fn write_to_clipboard<R: Runtime>(app: &AppHandle<R>, text: &str) -> Result<(), String> {
    app.clipboard()
        .write_text(text)
        .map_err(|error| error.to_string())
}

pub async fn capture_selected_text<R: Runtime>(app: &AppHandle<R>) -> Result<String, String> {
    save_clipboard(app)?;

    let sentinel = format!("shakespaire-selection-{}", std::process::id());
    let capture_result = async {
        let modifier_deadline =
            tokio::time::Instant::now() + tokio::time::Duration::from_millis(1200);
        while input::shortcut_modifiers_pressed() {
            if tokio::time::Instant::now() >= modifier_deadline {
                return Err("release the shortcut keys before text capture begins".to_string());
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(25)).await;

        write_to_clipboard(app, &sentinel)?;
        input::simulate_copy()?;

        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(900);
        loop {
            if let Ok(selected) = read_current_clipboard(app) {
                if selected != sentinel && !selected.trim().is_empty() {
                    break Ok(selected);
                }
            }
            if tokio::time::Instant::now() >= deadline {
                break Err("no text selection was captured".to_string());
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(8)).await;
        }
    }
    .await;

    let restore_result = restore_clipboard(app);
    match (capture_result, restore_result) {
        (Ok(selected), Ok(())) => Ok(selected),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(format!(
            "captured text but failed to restore clipboard: {error}"
        )),
        (Err(capture_error), Err(restore_error)) => Err(format!(
            "{capture_error}; also failed to restore clipboard: {restore_error}"
        )),
    }
}
