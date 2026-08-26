#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::Command;

#[derive(Clone, Copy, Debug)]
pub struct TextBounds {
    pub x: f64,
    pub y: f64,
    pub height: f64,
}

#[cfg(target_os = "macos")]
pub fn selected_text_bounds() -> Result<TextBounds, String> {
    use std::{ffi::CString, ptr};

    use accessibility_sys::{
        kAXErrorSuccess, kAXValueTypeCGRect, AXUIElementCopyAttributeValue,
        AXUIElementCopyParameterizedAttributeValue, AXUIElementCreateSystemWide, AXValueGetValue,
    };
    use core_foundation_sys::{
        base::{kCFAllocatorDefault, CFRelease, CFTypeRef},
        string::{kCFStringEncodingUTF8, CFStringCreateWithCString, CFStringRef},
    };

    #[repr(C)]
    #[derive(Default)]
    struct Point {
        x: f64,
        y: f64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct Size {
        width: f64,
        height: f64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        origin: Point,
        size: Size,
    }

    unsafe fn cf_string(value: &str) -> Result<CFStringRef, String> {
        let value = CString::new(value).map_err(|error| error.to_string())?;
        let string =
            CFStringCreateWithCString(kCFAllocatorDefault, value.as_ptr(), kCFStringEncodingUTF8);
        if string.is_null() {
            Err("could not create accessibility attribute name".to_string())
        } else {
            Ok(string)
        }
    }

    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return Err("could not access the macOS accessibility system".to_string());
        }

        let focused_name = cf_string("AXFocusedUIElement")?;
        let mut focused: CFTypeRef = ptr::null();
        let focused_error = AXUIElementCopyAttributeValue(system, focused_name, &mut focused);
        CFRelease(focused_name as CFTypeRef);
        CFRelease(system as CFTypeRef);
        if focused_error != kAXErrorSuccess || focused.is_null() {
            return Err("the focused app did not expose its selected text position".to_string());
        }

        let range_name = cf_string("AXSelectedTextRange")?;
        let mut range: CFTypeRef = ptr::null();
        let range_error = AXUIElementCopyAttributeValue(
            focused as accessibility_sys::AXUIElementRef,
            range_name,
            &mut range,
        );
        CFRelease(range_name as CFTypeRef);
        if range_error != kAXErrorSuccess || range.is_null() {
            CFRelease(focused);
            return Err("the focused app did not expose a selected text range".to_string());
        }

        let bounds_name = cf_string("AXBoundsForRange")?;
        let mut bounds_value: CFTypeRef = ptr::null();
        let bounds_error = AXUIElementCopyParameterizedAttributeValue(
            focused as accessibility_sys::AXUIElementRef,
            bounds_name,
            range,
            &mut bounds_value,
        );
        CFRelease(bounds_name as CFTypeRef);
        CFRelease(range);
        CFRelease(focused);
        if bounds_error != kAXErrorSuccess || bounds_value.is_null() {
            return Err("the focused app did not expose bounds for its selection".to_string());
        }

        let mut rect = Rect::default();
        let decoded = AXValueGetValue(
            bounds_value as accessibility_sys::AXValueRef,
            kAXValueTypeCGRect,
            &mut rect as *mut Rect as *mut std::ffi::c_void,
        );
        CFRelease(bounds_value);
        if !decoded || rect.size.width < 0.0 || rect.size.height < 0.0 {
            return Err("the selected text bounds were invalid".to_string());
        }

        Ok(TextBounds {
            x: rect.origin.x,
            y: rect.origin.y,
            height: rect.size.height,
        })
    }
}

#[cfg(not(target_os = "macos"))]
pub fn selected_text_bounds() -> Result<TextBounds, String> {
    Err("selected text positioning is currently supported only on macOS".to_string())
}

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
    use core_graphics::{
        event::{CGEvent, CGEventFlags, CGEventTapLocation, KeyCode},
        event_source::{CGEventSource, CGEventSourceStateID},
    };

    let keycode = match key {
        "c" => KeyCode::ANSI_C,
        "v" => KeyCode::ANSI_V,
        _ => return Err(format!("unsupported simulated key: {key}")),
    };
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "could not create a native keyboard event source".to_string())?;
    let key_down = CGEvent::new_keyboard_event(source.clone(), keycode, true)
        .map_err(|_| "could not create native key-down event".to_string())?;
    let key_up = CGEvent::new_keyboard_event(source, keycode, false)
        .map_err(|_| "could not create native key-up event".to_string())?;

    key_down.set_flags(CGEventFlags::CGEventFlagCommand);
    key_up.set_flags(CGEventFlags::CGEventFlagCommand);
    key_down.post(CGEventTapLocation::Session);
    key_up.post(CGEventTapLocation::Session);
    Ok(())
}

#[cfg(target_os = "windows")]
fn simulate_command_key(key: &str) -> Result<(), String> {
    use std::mem::size_of;

    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VIRTUAL_KEY, VK_C, VK_CONTROL, VK_V,
    };

    fn keyboard_input(key: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        }
    }

    let key = match key {
        "c" => VK_C,
        "v" => VK_V,
        _ => return Err(format!("unsupported simulated key: {key}")),
    };
    let inputs = [
        keyboard_input(VK_CONTROL, KEYBD_EVENT_FLAGS::default()),
        keyboard_input(key, KEYBD_EVENT_FLAGS::default()),
        keyboard_input(key, KEYEVENTF_KEYUP),
        keyboard_input(VK_CONTROL, KEYEVENTF_KEYUP),
    ];
    let inserted = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if inserted == inputs.len() as u32 {
        Ok(())
    } else {
        Err(format!(
            "Windows SendInput inserted {inserted}/{} keyboard events; input may be blocked by another process or by a higher-integrity application",
            inputs.len()
        ))
    }
}

#[cfg(target_os = "windows")]
pub fn shortcut_modifiers_pressed() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };

    [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN]
        .into_iter()
        .any(|key| unsafe { GetAsyncKeyState(key.0 as i32) as u16 & 0x8000 != 0 })
}

#[cfg(not(target_os = "windows"))]
pub fn shortcut_modifiers_pressed() -> bool {
    false
}

#[cfg(target_os = "linux")]
fn simulate_command_key(key: &str) -> Result<(), String> {
    fn run_tool(program: &str, arguments: &[&str]) -> Result<(), String> {
        let output = Command::new(program)
            .args(arguments)
            .output()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    format!("{program} is not installed")
                } else {
                    format!("could not start {program}: {error}")
                }
            })?;
        if output.status.success() {
            return Ok(());
        }

        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if detail.is_empty() {
            format!("{program} exited with {}", output.status)
        } else {
            format!("{program} failed: {detail}")
        })
    }

    if !matches!(key, "c" | "v") {
        return Err(format!("unsupported simulated key: {key}"));
    }

    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE")
            .is_ok_and(|session| session.eq_ignore_ascii_case("wayland"));
    let x11 = std::env::var_os("DISPLAY").is_some();
    let mut failures = Vec::new();

    if wayland {
        match run_tool("wtype", &["-M", "ctrl", key, "-m", "ctrl"]) {
            Ok(()) => return Ok(()),
            Err(error) => failures.push(error),
        }
    }

    if x11 {
        let shortcut = format!("ctrl+{key}");
        match run_tool("xdotool", &["key", "--clearmodifiers", &shortcut]) {
            Ok(()) => return Ok(()),
            Err(error) => failures.push(error),
        }
    }

    if failures.is_empty() {
        Err("no supported Linux display session detected; set WAYLAND_DISPLAY for wtype or DISPLAY for xdotool".to_string())
    } else {
        Err(format!(
            "could not simulate Ctrl+{key}: {}. Install wtype for Wayland or xdotool for X11",
            failures.join("; ")
        ))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn simulate_command_key(_key: &str) -> Result<(), String> {
    Err("copy and paste simulation is not supported on this platform".to_string())
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
