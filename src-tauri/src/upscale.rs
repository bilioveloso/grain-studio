//! Upscaling finished photos: a separate step for files that are already exported, so AI never
//! works on top of edits and effects. Classic is a Lanczos resize with light sharpening; AI is
//! Real-ESRGAN 4x (Clean Mode must be off) scaled to the requested size.

use std::path::{Path, PathBuf};
use std::time::Instant;

use image::{DynamicImage, GenericImageView, imageops};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::app_settings::load_settings;
use crate::app_state::AppState;

/// Above this, 4x would need gigabytes of memory for little gain.
const MAX_AI_SOURCE_EDGE: u32 = 3000;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpscaleResult {
    pub output: String,
    pub width: u32,
    pub height: u32,
    pub method: String,
    pub seconds: f32,
}

fn output_path(source: &Path, method: &str) -> PathBuf {
    let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("photo");
    source.with_file_name(format!("{stem}_upscaled_{method}.jpg"))
}

fn resize_to_long_edge(image: DynamicImage, long_edge: u32) -> DynamicImage {
    let (w, h) = image.dimensions();
    if long_edge == 0 || w.max(h) == long_edge {
        return image;
    }
    let scale = long_edge as f64 / w.max(h) as f64;
    let (tw, th) = ((w as f64 * scale).round() as u32, (h as f64 * scale).round() as u32);
    image.resize_exact(tw.max(1), th.max(1), imageops::FilterType::Lanczos3)
}

/// Upscale one finished photo on disk and write a new file next to it.
pub async fn upscale_photo_inner(
    app_handle: &AppHandle,
    path: String,
    method: String,
    long_edge: u32,
) -> Result<UpscaleResult, String> {
    let started = Instant::now();
    let (source, sidecar) = crate::file_management::parse_virtual_path(&path);
    let source_str = source.to_string_lossy().to_string();
    let settings = load_settings(app_handle.clone()).unwrap_or_default();

    if crate::formats::is_raw_file(&source_str) {
        return Err("This is a RAW file. Export it first, then upscale the exported photo.".into());
    }
    // Edits must be baked into a finished export first, so AI never enhances effects or blur.
    let metadata = crate::exif_processing::load_sidecar(&sidecar);
    let tm_override = crate::image_processing::resolve_tonemapper_override(&settings, false);
    if crate::image_processing::is_image_edited(&metadata.adjustments, false, tm_override) {
        return Err("This photo has Grain Studio edits. Export it first, then upscale the exported file.".into());
    }

    let use_ai = method == "ai";
    if use_ai && settings.clean_mode.unwrap_or(true) {
        return Err("AI upscaling is off while Clean Mode is on. Turn Clean Mode off to use it.".into());
    }

    let image = tauri::async_runtime::spawn_blocking({
        let source = source.clone();
        move || image::open(&source).map_err(|e| format!("Could not open the photo: {e}"))
    })
    .await
    .map_err(|e| e.to_string())??;

    let (w, h) = image.dimensions();
    let target = if long_edge == 0 { w.max(h) * if use_ai { 4 } else { 2 } } else { long_edge };

    let result = if use_ai {
        if w.max(h) > MAX_AI_SOURCE_EDGE {
            return Err(format!(
                "AI upscaling works on photos up to {MAX_AI_SOURCE_EDGE}px on the long edge; this one is {}px.",
                w.max(h)
            ));
        }
        let state = app_handle.state::<AppState>();
        let session = crate::ai_processing::get_or_init_esrgan_model(app_handle, &state.ai_state, &state.ai_init_lock)
            .await
            .map_err(|e| format!("AI upscaler unavailable: {e}"))?;
        tauri::async_runtime::spawn_blocking(move || {
            crate::ai_processing::upscale_esrgan_x4(&image, &session)
                .map(|big| resize_to_long_edge(big, target))
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??
    } else {
        tauri::async_runtime::spawn_blocking(move || resize_to_long_edge(image, target).unsharpen(1.0, 2))
            .await
            .map_err(|e| e.to_string())?
    };

    let out = output_path(&source, if use_ai { "ai" } else { "classic" });
    let (ow, oh) = result.dimensions();
    tauri::async_runtime::spawn_blocking({
        let out = out.clone();
        move || -> Result<(), String> {
            let file = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::BufWriter::new(file), 95);
            encoder.encode_image(&result.to_rgb8()).map_err(|e| e.to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(UpscaleResult {
        output: out.to_string_lossy().to_string(),
        width: ow,
        height: oh,
        method: if use_ai { "ai".into() } else { "classic".into() },
        seconds: started.elapsed().as_secs_f32(),
    })
}

#[tauri::command]
pub async fn upscale_photo(
    path: String,
    method: String,
    long_edge: Option<u32>,
    app_handle: AppHandle,
) -> Result<UpscaleResult, String> {
    upscale_photo_inner(&app_handle, path, method, long_edge.unwrap_or(3840)).await
}

/// A JPEG of any image file as it is on disk, optionally a crop given as fractions [x, y, w, h].
pub fn view_image_jpeg(path: &str, crop: Option<[f64; 4]>, max_edge: u32) -> Result<Vec<u8>, String> {
    let (source, _) = crate::file_management::parse_virtual_path(path);
    let mut image = image::open(&source).map_err(|e| format!("Could not open {path}: {e}"))?;
    if let Some([x, y, cw, ch]) = crop {
        let (w, h) = image.dimensions();
        let px = (x.clamp(0.0, 1.0) * w as f64) as u32;
        let py = (y.clamp(0.0, 1.0) * h as f64) as u32;
        let pw = ((cw.clamp(0.0, 1.0) * w as f64) as u32).max(1).min(w - px.min(w - 1));
        let ph = ((ch.clamp(0.0, 1.0) * h as f64) as u32).max(1).min(h - py.min(h - 1));
        image = image.crop_imm(px, py, pw, ph);
    }
    let (w, h) = image.dimensions();
    if w.max(h) > max_edge {
        image = image.resize(max_edge, max_edge, imageops::FilterType::Triangle);
    }
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 92)
        .encode_image(&image.to_rgb8())
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}
