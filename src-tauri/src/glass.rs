//! Native translucent backdrop shared by the settings window and the popup.
//!
//! macOS 26+ gets a real Liquid Glass (`NSGlassEffectView`), older macOS gets
//! vibrancy, Windows gets Mica/Acrylic, and Linux keeps the CSS fallback.

use tauri::{Runtime, WebviewWindow};

#[cfg(target_os = "macos")]
/// `radius` rounds the glass for frameless windows; native windows pass 0.
pub fn apply<R: Runtime>(window: &WebviewWindow<R>, radius: f64) -> Result<(), String> {
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

            let options = LiquidGlassOptions::new(NSGlassEffectViewStyle::Regular)
                .radius(radius)
                .opaque(false)
                .content_view(content_view);

            if let Err(glass_error) = window_vibrancy::apply_liquid_glass(&effect_window, options) {
                log::info!("Liquid Glass unavailable ({glass_error}); using vibrancy fallback");
                if let Err(error) = window_vibrancy::apply_vibrancy(
                    &effect_window,
                    NSVisualEffectMaterial::Popover,
                    Some(NSVisualEffectState::Active),
                    (radius > 0.0).then_some(radius),
                ) {
                    log::warn!("could not apply settings window vibrancy: {error}");
                }
            }
        })
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
pub fn apply<R: Runtime>(window: &WebviewWindow<R>, _radius: f64) -> Result<(), String> {
    use window_vibrancy::{
        apply_acrylic, apply_blur, apply_mica, clear_acrylic, clear_blur, clear_mica,
    };

    let _ = clear_mica(window);
    let _ = clear_acrylic(window);
    let _ = clear_blur(window);
    if apply_acrylic(window, Some((244, 246, 250, 176))).is_err()
        && apply_mica(window, Some(false)).is_err()
    {
        let _ = apply_blur(window, Some((244, 246, 250, 150)));
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn apply<R: Runtime>(_window: &WebviewWindow<R>, _radius: f64) -> Result<(), String> {
    Ok(())
}
