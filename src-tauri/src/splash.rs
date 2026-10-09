//! Welcome-screen photo: the owner picks one of their own photos, and the app takes both the
//! splash image and its accent colour from it. Clearing brings back the bundled photo and amber.

use std::fs;
use std::path::PathBuf;

use image::GenericImageView;
use serde::Serialize;
use tauri::{AppHandle, Manager};

/// The splash lives inside the thumbnail cache so the app's own `thumb` protocol can serve it.
fn splash_file(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let dir = app_handle
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("thumbnails");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("_splash.jpg"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplashInfo {
    /// Absolute path of the cached splash image (served through the `thumb` protocol).
    pub file: String,
    /// CSS colour taken from the photo, e.g. "rgb(198, 142, 110)".
    pub accent: String,
}

/// Dominant colour of the photo, tuned to work as a UI accent: the most common hue among
/// reasonably saturated mid-tones, re-mixed at fixed saturation and lightness so it always
/// reads against dark panels. Falls back to the default amber for flat or grey photos.
fn accent_from_image(image: &image::DynamicImage) -> String {
    const BINS: usize = 24;
    let small = image.resize_exact(64, 64, image::imageops::FilterType::Triangle);
    let mut weights = [0f32; BINS];

    for (_, _, px) in small.pixels() {
        let (r, g, b) = (
            px[0] as f32 / 255.0,
            px[1] as f32 / 255.0,
            px[2] as f32 / 255.0,
        );
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let lightness = (max + min) / 2.0;
        let delta = max - min;
        if delta < 0.08 || !(0.15..0.92).contains(&lightness) {
            continue;
        }
        let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs()).max(0.001);
        if saturation < 0.18 {
            continue;
        }
        let hue = if max == r {
            60.0 * (((g - b) / delta) % 6.0)
        } else if max == g {
            60.0 * ((b - r) / delta + 2.0)
        } else {
            60.0 * ((r - g) / delta + 4.0)
        };
        let hue = (hue + 360.0) % 360.0;
        weights[(hue / (360.0 / BINS as f32)) as usize % BINS] += saturation;
    }

    let best = weights
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap());
    let Some(bin) = best.filter(|(_, w)| **w > 1.0).map(|(i, _)| i) else {
        return "rgb(232, 168, 96)".to_string();
    };

    // Fixed saturation/lightness keeps every accent legible, whatever the photo.
    let hue = (bin as f32 + 0.5) * (360.0 / BINS as f32);
    let (s, l) = (0.55f32, 0.63f32);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match hue as u32 / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to_255 = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    format!("rgb({}, {}, {})", to_255(r), to_255(g), to_255(b))
}

/// Render the chosen photo (with its saved edits, so raw files work) into the splash cache
/// and report the accent colour taken from it.
#[tauri::command]
pub async fn set_splash_image(path: String, app_handle: AppHandle) -> Result<SplashInfo, String> {
    let handle = app_handle.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (_, sidecar_path) = crate::file_management::parse_virtual_path(&path);
        let mut adjustments = crate::exif_processing::load_sidecar(&sidecar_path).adjustments;
        if let Some(obj) = adjustments.as_object_mut() {
            // The welcome screen shows the photo, not a frame or a second exposure.
            obj.remove("doubleExposurePath");
            obj.remove("frameWidth");
            obj.remove("frameRadius");
        } else {
            adjustments = serde_json::json!({});
        }

        let jpeg =
            crate::render_preview_jpeg(&path, &adjustments, &handle, Some(1920), false, None)?;
        let image = image::load_from_memory(&jpeg).map_err(|e| e.to_string())?;
        let accent = accent_from_image(&image);

        let file = splash_file(&handle)?;
        fs::write(&file, &jpeg).map_err(|e| e.to_string())?;
        Ok(SplashInfo {
            file: file.to_string_lossy().to_string(),
            accent,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Drop the custom welcome photo; the app goes back to the bundled one.
#[tauri::command]
pub fn clear_splash_image(app_handle: AppHandle) -> Result<(), String> {
    let file = splash_file(&app_handle)?;
    if file.exists() {
        fs::remove_file(&file).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::accent_from_image;
    use image::{DynamicImage, Rgba, RgbaImage};

    fn flat(color: [u8; 4]) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(64, 64, Rgba(color)))
    }

    #[test]
    fn picks_the_photo_hue_and_falls_back_on_grey() {
        // A blue photo gives a blue accent: blue channel strongest, red weakest.
        let accent = accent_from_image(&flat([30, 90, 200, 255]));
        let rgb: Vec<u32> = accent
            .trim_start_matches("rgb(")
            .trim_end_matches(')')
            .split(", ")
            .map(|v| v.parse().unwrap())
            .collect();
        assert!(rgb[2] > rgb[0], "expected a blue-ish accent, got {accent}");

        // A grey photo has no usable hue, so the default amber stands.
        assert_eq!(
            accent_from_image(&flat([128, 128, 128, 255])),
            "rgb(232, 168, 96)"
        );
    }
}
