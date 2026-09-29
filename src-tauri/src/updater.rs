//! Self-update from the menu bar: check, download, verify and install a signed release.
//!
//! Everything is driven from one tray menu item whose text and enabled state follow the update
//! state, so there is no extra window or setting. Installing always waits for the user's click.

use std::{sync::Mutex, time::Duration};

use tauri::{menu::MenuItem, AppHandle, Manager, Wry};
use tauri_plugin_updater::UpdaterExt;

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(10);
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const UP_TO_DATE_VISIBLE: Duration = Duration::from_secs(4);
const RESTART_WAIT_LIMIT: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, PartialEq)]
enum State {
    Idle,
    Checking,
    UpToDate,
    /// A newer version (for example "v0.2.0") is ready to install.
    Available(String),
    Downloading(u8),
    Restarting,
    Failed,
}

fn label(state: &State) -> String {
    match state {
        State::Idle => "Check for Updates…".to_string(),
        State::Checking => "Checking for Updates…".to_string(),
        State::UpToDate => "You’re Up to Date".to_string(),
        State::Available(version) => format!("Install Update {version}…"),
        State::Downloading(percent) => format!("Downloading Update… {percent}%"),
        State::Restarting => "Restarting…".to_string(),
        State::Failed => "Update Failed — Try Again".to_string(),
    }
}

fn clickable(state: &State) -> bool {
    matches!(state, State::Idle | State::Available(_) | State::Failed)
}

/// The tray menu item plus the state it currently shows.
pub struct UpdateMenu {
    item: MenuItem<Wry>,
    state: Mutex<State>,
}

impl UpdateMenu {
    pub fn new(item: MenuItem<Wry>) -> Self {
        Self {
            item,
            state: Mutex::new(State::Idle),
        }
    }
}

fn current(app: &AppHandle) -> State {
    app.try_state::<UpdateMenu>()
        .map(|menu| {
            menu.state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        })
        .unwrap_or(State::Idle)
}

fn set_state(app: &AppHandle, new: State) {
    let Some(menu) = app.try_state::<UpdateMenu>() else {
        return;
    };
    *menu
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = new.clone();
    let _ = menu.item.set_text(label(&new));
    let _ = menu.item.set_enabled(clickable(&new));
    if let Some(tray) = app.tray_by_id("main") {
        let tooltip = if matches!(new, State::Available(_)) {
            "shakespAIre — update available"
        } else {
            "shakespAIre"
        };
        let _ = tray.set_tooltip(Some(tooltip));
    }
}

/// Called when the update menu item is clicked.
pub fn on_click(app: AppHandle) {
    match current(&app) {
        State::Idle | State::Failed => {
            tauri::async_runtime::spawn(check(app, true));
        }
        State::Available(_) => {
            tauri::async_runtime::spawn(install(app));
        }
        _ => {}
    }
}

/// Checks once a day in the background. Release builds only; failures stay silent.
pub fn start_background_checks(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        loop {
            if matches!(current(&app), State::Idle | State::UpToDate) {
                check(app.clone(), false).await;
            }
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

async fn check(app: AppHandle, manual: bool) {
    if manual {
        set_state(&app, State::Checking);
    }
    let result = async {
        let updater = app.updater().map_err(|error| error.to_string())?;
        updater.check().await.map_err(|error| error.to_string())
    }
    .await;

    match result {
        Ok(Some(update)) => set_state(&app, State::Available(format!("v{}", update.version))),
        Ok(None) if manual => {
            set_state(&app, State::UpToDate);
            tokio::time::sleep(UP_TO_DATE_VISIBLE).await;
            if current(&app) == State::UpToDate {
                set_state(&app, State::Idle);
            }
        }
        Ok(None) => {}
        Err(error) => {
            log::warn!("update check failed: {error}");
            if manual {
                set_state(&app, State::Failed);
            }
        }
    }
}

async fn install(app: AppHandle) {
    set_state(&app, State::Downloading(0));
    let progress_app = app.clone();
    let outcome = async {
        let updater = app.updater().map_err(|error| error.to_string())?;
        // Ask again so the install uses the freshest release rather than a stale answer.
        let Some(update) = updater.check().await.map_err(|error| error.to_string())? else {
            return Ok(false);
        };
        let mut downloaded: u64 = 0;
        let mut last_percent: u8 = 0;
        update
            .download_and_install(
                move |chunk, total| {
                    downloaded += chunk as u64;
                    if let Some(total) = total.filter(|total| *total > 0) {
                        let percent = ((downloaded * 100) / total).min(100) as u8;
                        if percent != last_percent {
                            last_percent = percent;
                            set_state(&progress_app, State::Downloading(percent));
                        }
                    }
                },
                || {},
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok::<bool, String>(true)
    }
    .await;

    match outcome {
        Ok(true) => {
            set_state(&app, State::Restarting);
            wait_until_idle(&app).await;
            log::info!("update installed; restarting");
            app.restart();
        }
        Ok(false) => set_state(&app, State::Idle),
        Err(error) => {
            log::warn!("update failed: {error}");
            set_state(&app, State::Failed);
        }
    }
}

/// Never restart in the middle of a proofread or replace: wait (bounded) until the popup is
/// closed and the clipboard is free.
async fn wait_until_idle(app: &AppHandle) {
    let deadline = tokio::time::Instant::now() + RESTART_WAIT_LIMIT;
    while tokio::time::Instant::now() < deadline {
        let popup_open = app
            .get_webview_window("popup")
            .and_then(|popup| popup.is_visible().ok())
            .unwrap_or(false);
        let clipboard_busy = crate::CLIPBOARD_BUSY.load(std::sync::atomic::Ordering::Acquire);
        if !popup_open && !clipboard_busy {
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{clickable, label, State};

    #[test]
    fn labels_follow_the_state() {
        assert_eq!(label(&State::Idle), "Check for Updates…");
        assert_eq!(
            label(&State::Available("v0.2.0".to_string())),
            "Install Update v0.2.0…"
        );
        assert_eq!(label(&State::Downloading(42)), "Downloading Update… 42%");
    }

    #[test]
    fn only_actionable_states_are_clickable() {
        assert!(clickable(&State::Idle));
        assert!(clickable(&State::Available("v0.2.0".to_string())));
        assert!(clickable(&State::Failed));
        assert!(!clickable(&State::Checking));
        assert!(!clickable(&State::Downloading(10)));
        assert!(!clickable(&State::Restarting));
    }
}
