use std::process::Command;

#[cfg(target_os = "macos")]
fn run_osascript(script: &str, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new("osascript")
        .args(["-e", script, "--"])
        .args(arguments)
        .output()
        .map_err(|error| format!("failed to run osascript: {error}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if stderr.is_empty() {
            format!("osascript exited with {}", output.status)
        } else {
            format!("osascript failed: {stderr}")
        })
    }
}

#[cfg(target_os = "macos")]
fn simulate_command_key(key: &str) -> Result<(), String> {
    let script =
        format!("tell application \"System Events\" to keystroke \"{key}\" using command down");
    run_osascript(&script, &[]).map(|_| ())
}

#[cfg(not(target_os = "macos"))]
fn simulate_command_key(_key: &str) -> Result<(), String> {
    Err("native copy and paste simulation is currently supported only on macOS".to_string())
}

pub fn simulate_copy() -> Result<(), String> {
    simulate_command_key("c")
}

pub fn simulate_paste() -> Result<(), String> {
    simulate_command_key("v")
}

#[cfg(target_os = "macos")]
pub fn frontmost_application() -> Result<String, String> {
    run_osascript(
        "tell application \"System Events\" to get name of first application process whose frontmost is true",
        &[],
    )
}

#[cfg(not(target_os = "macos"))]
pub fn frontmost_application() -> Result<String, String> {
    Err("foreground application detection is currently supported only on macOS".to_string())
}

#[cfg(target_os = "macos")]
pub fn activate_application(name: &str) -> Result<(), String> {
    run_osascript(
        "on run argv\nset appName to item 1 of argv\ntell application \"System Events\" to set frontmost of first application process whose name is appName to true\nend run",
        &[name],
    )
    .map(|_| ())
}

#[cfg(not(target_os = "macos"))]
pub fn activate_application(_name: &str) -> Result<(), String> {
    Err("foreground application activation is currently supported only on macOS".to_string())
}
