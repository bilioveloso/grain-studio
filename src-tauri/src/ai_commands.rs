use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Cursor;

use base64::{Engine as _, engine::general_purpose};
use image::{GrayImage, ImageFormat};

use crate::ai_connector;
use crate::ai_processing::{
    AiDepthMaskParameters, AiForegroundMaskParameters, AiSkyMaskParameters,
    AiSubjectMaskParameters, CachedDepthMap, generate_image_embeddings, get_or_init_ai_models,
    get_or_init_clip_models,
    run_depth_anything_model, run_sam_decoder, run_sky_seg_model, run_u2netp_model,
};
use crate::app_settings::load_settings;
use crate::app_state::AppState;
use crate::cache_utils::GEOMETRY_KEYS;
use crate::get_cached_full_warped_image;

fn encode_to_base64_png(image: &GrayImage) -> Result<String, String> {
    let mut buf = Cursor::new(Vec::new());
    image
        .write_to(&mut buf, ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let base64_str = general_purpose::STANDARD.encode(buf.get_ref());
    Ok(format!("data:image/png;base64,{}", base64_str))
}

#[tauri::command]
pub async fn generate_ai_foreground_mask(
    js_adjustments: serde_json::Value,
    rotation: f32,
    flip_horizontal: bool,
    flip_vertical: bool,
    orientation_steps: u8,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<AiForegroundMaskParameters, String> {
    let models = get_or_init_ai_models(&app_handle, &state.ai_state, &state.ai_init_lock)
        .await
        .map_err(|e| e.to_string())?;

    let warped_image = get_cached_full_warped_image(&state, &js_adjustments)?;

    let full_mask_image =
        run_u2netp_model(warped_image.as_ref(), &models.u2netp).map_err(|e| e.to_string())?;
    let base64_data = encode_to_base64_png(&full_mask_image)?;

    Ok(AiForegroundMaskParameters {
        mask_data_base64: Some(base64_data),
        rotation: Some(rotation),
        flip_horizontal: Some(flip_horizontal),
        flip_vertical: Some(flip_vertical),
        orientation_steps: Some(orientation_steps),
    })
}

#[tauri::command]
pub async fn generate_ai_sky_mask(
    js_adjustments: serde_json::Value,
    path: Option<String>,
    rotation: f32,
    flip_horizontal: bool,
    flip_vertical: bool,
    orientation_steps: u8,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<AiSkyMaskParameters, String> {
    // A ProRAW DNG carries Apple's own sky matte, made on the phone at capture: use it when it is there. It is
    // turned and sized to the decoded raw, then warped like the photo, so it lands in the same space the sky
    // model's output would.
    let apple = path.as_deref().and_then(|p| {
        let (source, _) = crate::file_management::parse_virtual_path(p);
        let (base, _) = crate::get_original_image(&state).ok()?;
        let matte = crate::apple_matte::sky_matte(&source, base.width(), base.height())?;
        let as_rgb = image::DynamicImage::ImageRgb32F(image::DynamicImage::ImageLuma8(matte).to_rgb32f());
        let warped = crate::image_processing::apply_geometry_warp(as_rgb, &js_adjustments).into_owned();
        log::info!("Sky mask from Apple's own matte in {}", source.display());
        Some(warped.to_luma8())
    });

    let full_mask_image = match apple {
        Some(m) => m,
        None => {
            let models = get_or_init_ai_models(&app_handle, &state.ai_state, &state.ai_init_lock)
                .await
                .map_err(|e| e.to_string())?;
            let warped_image = get_cached_full_warped_image(&state, &js_adjustments)?;
            run_sky_seg_model(warped_image.as_ref(), &models.sky_seg).map_err(|e| e.to_string())?
        }
    };
    let base64_data = encode_to_base64_png(&full_mask_image)?;

    Ok(AiSkyMaskParameters {
        mask_data_base64: Some(base64_data),
        rotation: Some(rotation),
        flip_horizontal: Some(flip_horizontal),
        flip_vertical: Some(flip_vertical),
        orientation_steps: Some(orientation_steps),
    })
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn generate_ai_depth_mask(
    js_adjustments: serde_json::Value,
    path: String,
    min_depth: f32,
    max_depth: f32,
    min_fade: f32,
    max_fade: f32,
    feather: f32,
    rotation: f32,
    flip_horizontal: bool,
    flip_vertical: bool,
    orientation_steps: u8,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<AiDepthMaskParameters, String> {
    let models = get_or_init_ai_models(&app_handle, &state.ai_state, &state.ai_init_lock)
        .await
        .map_err(|e| e.to_string())?;

    let path_hash = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(path.as_bytes());
        let mut geo_hasher = DefaultHasher::new();
        for key in GEOMETRY_KEYS {
            if let Some(val) = js_adjustments.get(key) {
                key.hash(&mut geo_hasher);
                val.to_string().hash(&mut geo_hasher);
            }
        }
        hasher.update(&geo_hasher.finish().to_le_bytes());
        hasher.finalize().to_hex().to_string()
    };

    let cached_depth = {
        let mut ai_state_lock = state.ai_state.lock().unwrap();
        let ai_state = ai_state_lock.as_mut().unwrap();

        if let Some(cached) = &ai_state.depth_map {
            if cached.path_hash == path_hash {
                cached.clone()
            } else {
                let warped_image = get_cached_full_warped_image(&state, &js_adjustments)?;
                let depth_img =
                    run_depth_anything_model(warped_image.as_ref(), &models.depth_anything)
                        .map_err(|e| e.to_string())?;
                let new_cache = CachedDepthMap {
                    path_hash: path_hash.clone(),
                    depth_image: depth_img,
                    original_size: (warped_image.width(), warped_image.height()),
                };
                ai_state.depth_map = Some(new_cache.clone());
                new_cache
            }
        } else {
            let warped_image = get_cached_full_warped_image(&state, &js_adjustments)?;
            let depth_img = run_depth_anything_model(warped_image.as_ref(), &models.depth_anything)
                .map_err(|e| e.to_string())?;
            let new_cache = CachedDepthMap {
                path_hash: path_hash.clone(),
                depth_image: depth_img,
                original_size: (warped_image.width(), warped_image.height()),
            };
            ai_state.depth_map = Some(new_cache.clone());
            new_cache
        }
    };

    let raw_depth_fullres = image::imageops::resize(
        &cached_depth.depth_image,
        cached_depth.original_size.0,
        cached_depth.original_size.1,
        image::imageops::FilterType::Triangle,
    );

    let base64_data = encode_to_base64_png(&raw_depth_fullres)?;

    Ok(AiDepthMaskParameters {
        min_depth,
        max_depth,
        min_fade,
        max_fade,
        feather,
        mask_data_base64: Some(base64_data),
        rotation: Some(rotation),
        flip_horizontal: Some(flip_horizontal),
        flip_vertical: Some(flip_vertical),
        orientation_steps: Some(orientation_steps),
    })
}

#[tauri::command]
pub async fn generate_full_image_depth_map(
    js_adjustments: serde_json::Value,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    let models = crate::ai_processing::get_or_init_ai_models(
        &app_handle,
        &state.ai_state,
        &state.ai_init_lock,
    )
    .await
    .map_err(|e| e.to_string())?;

    let warped_image = crate::get_cached_full_warped_image(&state, &js_adjustments)?;

    let depth_img = crate::ai_processing::run_depth_anything_model(
        warped_image.as_ref(),
        &models.depth_anything,
    )
    .map_err(|e| e.to_string())?;

    let mut buf = std::io::Cursor::new(Vec::new());
    depth_img
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let base64_str = base64::engine::general_purpose::STANDARD.encode(buf.get_ref());

    Ok(format!("data:image/png;base64,{}", base64_str))
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn generate_ai_subject_mask(
    js_adjustments: serde_json::Value,
    path: String,
    start_point: (f64, f64),
    end_point: (f64, f64),
    stroke_points: Option<Vec<Vec<f64>>>,
    rotation: f32,
    flip_horizontal: bool,
    flip_vertical: bool,
    orientation_steps: u8,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<AiSubjectMaskParameters, String> {
    let models = get_or_init_ai_models(&app_handle, &state.ai_state, &state.ai_init_lock)
        .await
        .map_err(|e| e.to_string())?;

    let path_hash = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(path.as_bytes());
        let mut geo_hasher = DefaultHasher::new();
        for key in GEOMETRY_KEYS {
            if let Some(val) = js_adjustments.get(key) {
                key.hash(&mut geo_hasher);
                val.to_string().hash(&mut geo_hasher);
            }
        }
        hasher.update(&geo_hasher.finish().to_le_bytes());
        hasher.finalize().to_hex().to_string()
    };

    let warped_image = get_cached_full_warped_image(&state, &js_adjustments)?;

    let embeddings = {
        let mut ai_state_lock = state.ai_state.lock().unwrap();
        let ai_state = ai_state_lock.as_mut().unwrap();

        if let Some(cached_embeddings) = &ai_state.embeddings {
            if cached_embeddings.path_hash == path_hash {
                cached_embeddings.clone()
            } else {
                let mut new_embeddings =
                    generate_image_embeddings(warped_image.as_ref(), &models.sam_encoder)
                        .map_err(|e| e.to_string())?;
                new_embeddings.path_hash = path_hash.clone();
                ai_state.embeddings = Some(new_embeddings.clone());
                new_embeddings
            }
        } else {
            let mut new_embeddings =
                generate_image_embeddings(warped_image.as_ref(), &models.sam_encoder)
                    .map_err(|e| e.to_string())?;
            new_embeddings.path_hash = path_hash.clone();
            ai_state.embeddings = Some(new_embeddings.clone());
            new_embeddings
        }
    };

    let (img_w, img_h) = embeddings.original_size;

    let (coarse_rotated_w, coarse_rotated_h) = if orientation_steps % 2 == 1 {
        (img_h as f64, img_w as f64)
    } else {
        (img_w as f64, img_h as f64)
    };

    let center = (coarse_rotated_w / 2.0, coarse_rotated_h / 2.0);

    let p1 = start_point;
    let p2 = (start_point.0, end_point.1);
    let p3 = end_point;
    let p4 = (end_point.0, start_point.1);

    let angle_rad = (rotation as f64).to_radians();
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();

    let unrotate = |p: (f64, f64)| {
        let px = p.0 - center.0;
        let py = p.1 - center.1;
        let new_px = px * cos_a + py * sin_a + center.0;
        let new_py = -px * sin_a + py * cos_a + center.1;
        (new_px, new_py)
    };

    let up1 = unrotate(p1);
    let up2 = unrotate(p2);
    let up3 = unrotate(p3);
    let up4 = unrotate(p4);

    let unflip = |p: (f64, f64)| {
        let mut new_px = p.0;
        let mut new_py = p.1;
        if flip_horizontal {
            new_px = coarse_rotated_w - p.0;
        }
        if flip_vertical {
            new_py = coarse_rotated_h - p.1;
        }
        (new_px, new_py)
    };

    let ufp1 = unflip(up1);
    let ufp2 = unflip(up2);
    let ufp3 = unflip(up3);
    let ufp4 = unflip(up4);

    let un_coarse_rotate = |p: (f64, f64)| -> (f64, f64) {
        match orientation_steps {
            0 => p,
            1 => (p.1, img_h as f64 - p.0),
            2 => (img_w as f64 - p.0, img_h as f64 - p.1),
            3 => (img_w as f64 - p.1, p.0),
            _ => p,
        }
    };

    let ucrp1 = un_coarse_rotate(ufp1);
    let ucrp2 = un_coarse_rotate(ufp2);
    let ucrp3 = un_coarse_rotate(ufp3);
    let ucrp4 = un_coarse_rotate(ufp4);

    let min_x = ucrp1.0.min(ucrp2.0).min(ucrp3.0).min(ucrp4.0);
    let min_y = ucrp1.1.min(ucrp2.1).min(ucrp3.1).min(ucrp4.1);
    let max_x = ucrp1.0.max(ucrp2.0).max(ucrp3.0).max(ucrp4.0);
    let max_y = ucrp1.1.max(ucrp2.1).max(ucrp3.1).max(ucrp4.1);

    let unrotated_start_point = (min_x, min_y);
    let unrotated_end_point = (max_x, max_y);

    // Each prompt is [x, y] or [x, y, label]: label 1 include (default), 0 exclude.
    let source_stroke: Option<Vec<(f64, f64, f32)>> = stroke_points.map(|pts| {
        pts.into_iter()
            .filter(|p| p.len() >= 2)
            .map(|p| {
                let (x, y) = un_coarse_rotate(unflip(unrotate((p[0], p[1]))));
                (x, y, p.get(2).copied().unwrap_or(1.0) as f32)
            })
            .collect()
    });

    // Crop pass + colour matte, and the learned edge matte if the user installed one: the mask lands on the
    // photograph's own edge instead of a blurred envelope.
    let matte = crate::ai_processing::get_models_dir(&app_handle)
        .ok()
        .and_then(|dir| crate::crisp_mask::installed_matte(&dir));
    let mask_bitmap = crate::crisp_mask::crisp_subject_mask(
        &models.sam_encoder,
        &models.sam_decoder,
        &embeddings,
        warped_image.as_ref(),
        unrotated_start_point,
        unrotated_end_point,
        source_stroke.as_deref(),
        matte.as_deref(),
    )
    .map_err(|e| e.to_string())?;

    let base64_data = encode_to_base64_png(&mask_bitmap)?;

    Ok(AiSubjectMaskParameters {
        start_x: start_point.0,
        start_y: start_point.1,
        end_x: end_point.0,
        end_y: end_point.1,
        mask_data_base64: Some(base64_data),
        rotation: Some(rotation),
        flip_horizontal: Some(flip_horizontal),
        flip_vertical: Some(flip_vertical),
        orientation_steps: Some(orientation_steps),
    })
}

#[tauri::command]
pub async fn precompute_ai_subject_mask(
    js_adjustments: serde_json::Value,
    path: String,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let models = get_or_init_ai_models(&app_handle, &state.ai_state, &state.ai_init_lock)
        .await
        .map_err(|e| e.to_string())?;

    let path_hash = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(path.as_bytes());
        let mut geo_hasher = DefaultHasher::new();
        for key in GEOMETRY_KEYS {
            if let Some(val) = js_adjustments.get(key) {
                key.hash(&mut geo_hasher);
                val.to_string().hash(&mut geo_hasher);
            }
        }
        hasher.update(&geo_hasher.finish().to_le_bytes());
        hasher.finalize().to_hex().to_string()
    };

    let mut ai_state_lock = state.ai_state.lock().unwrap();
    let ai_state = ai_state_lock.as_mut().unwrap();

    if let Some(cached_embeddings) = &ai_state.embeddings
        && cached_embeddings.path_hash == path_hash
    {
        return Ok(());
    }

    let warped_image = get_cached_full_warped_image(&state, &js_adjustments)?;
    let mut new_embeddings = generate_image_embeddings(warped_image.as_ref(), &models.sam_encoder)
        .map_err(|e| e.to_string())?;

    new_embeddings.path_hash = path_hash.clone();
    ai_state.embeddings = Some(new_embeddings);

    Ok(())
}

#[tauri::command]
pub async fn check_ai_connector_status(app_handle: tauri::AppHandle) {
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let is_connected = if let Some(address) = settings.ai_connector_address {
        ai_connector::check_status(&address).await.unwrap_or(false)
    } else {
        false
    };
    use tauri::Emitter;
    let _ = app_handle.emit(
        "ai-connector-status-update",
        serde_json::json!({ "connected": is_connected }),
    );
}

#[tauri::command]
pub async fn test_ai_connector_connection(address: String) -> Result<(), String> {
    match ai_connector::check_status(&address).await {
        Ok(true) => Ok(()),
        Ok(false) => Err("Server reachable but returned bad health status".to_string()),
        Err(e) => Err(e.to_string()),
    }
}


/// One-click people removal, like Lightroom: SAM proposes objects from a grid of clicks, CLIP keeps the
/// ones that look like people, and each person comes back as a Quick Erase mask the app fills from the
/// surroundings (local models only, no prompt). Works on photos without rotation or flips.
#[tauri::command]
pub async fn detect_people_for_removal(
    js_adjustments: serde_json::Value,
    path: String,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<Vec<AiSubjectMaskParameters>, String> {
    let turned = |k: &str| js_adjustments.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0).abs() > 1e-6;
    let flipped = |k: &str| js_adjustments.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    if turned("rotation") || turned("orientationSteps") || flipped("flipHorizontal") || flipped("flipVertical") {
        return Err("Remove people works on photos without rotation or flips for now.".to_string());
    }

    let models = get_or_init_ai_models(&app_handle, &state.ai_state, &state.ai_init_lock)
        .await
        .map_err(|e| e.to_string())?;
    let clip = get_or_init_clip_models(&app_handle, &state.ai_state, &state.ai_init_lock)
        .await
        .map_err(|e| e.to_string())?;

    let path_hash = {
        let mut hasher = blake3::Hasher::new();
        hasher.update(path.as_bytes());
        let mut geo_hasher = DefaultHasher::new();
        for key in GEOMETRY_KEYS {
            if let Some(val) = js_adjustments.get(key) {
                key.hash(&mut geo_hasher);
                val.to_string().hash(&mut geo_hasher);
            }
        }
        hasher.update(&geo_hasher.finish().to_le_bytes());
        hasher.finalize().to_hex().to_string()
    };
    let warped_image = get_cached_full_warped_image(&state, &js_adjustments)?;
    let embeddings = {
        let mut ai_state_lock = state.ai_state.lock().unwrap();
        let ai_state = ai_state_lock.as_mut().unwrap();
        match &ai_state.embeddings {
            Some(cached) if cached.path_hash == path_hash => cached.clone(),
            _ => {
                let mut fresh = generate_image_embeddings(warped_image.as_ref(), &models.sam_encoder)
                    .map_err(|e| e.to_string())?;
                fresh.path_hash = path_hash.clone();
                ai_state.embeddings = Some(fresh.clone());
                fresh
            }
        }
    };
    let (w, h) = embeddings.original_size;

    // Object proposals from a grid of SAM clicks. Each new proposal of a plausible size is checked by CLIP
    // right away, so a person's full-size mask is kept without decoding it twice.
    // ponytail: fixed grids of clicks (10x8 everywhere, 18x7 in the band where people stand); people smaller
    // than the gap between clicks can still be missed. A click-to-remove fallback exists in Quick Eraser.
    const COLS: u32 = 10;
    const ROWS: u32 = 8;
    const BAND_COLS: u32 = 18;
    const BAND_ROWS: u32 = 7;
    const LABELS: [&str; 10] = [
        "a photo of a person",
        "a photo of people walking",
        "a photo of a building",
        "a photo of the sky",
        "a photo of a street or pavement",
        "a photo of a tree or plants",
        "a photo of a car",
        "a photo of a lamp post or sign",
        "a photo of water",
        "a photo of the ground",
    ];
    let small_w = 160u32;
    let small_h = ((160.0 * h as f64 / w as f64).round() as u32).max(1);
    let sx = w as f64 / small_w as f64;
    let sy = h as f64 / small_h as f64;
    let mut seen: Vec<Vec<bool>> = Vec::new();
    let mut people: Vec<(GrayImage, (f64, f64, f64, f64))> = Vec::new();

    // Coarse grid over the whole photo, then a denser band from 35% to 95% of the height.
    let mut points: Vec<(f64, f64)> = Vec::new();
    for r in 0..ROWS {
        for c in 0..COLS {
            points.push(((c as f64 + 0.5) / COLS as f64 * w as f64, (r as f64 + 0.5) / ROWS as f64 * h as f64));
        }
    }
    for r in 0..BAND_ROWS {
        for c in 0..BAND_COLS {
            points.push((
                (c as f64 + 0.5) / BAND_COLS as f64 * w as f64,
                (0.35 + 0.60 * (r as f64 + 0.5) / BAND_ROWS as f64) * h as f64,
            ));
        }
    }

    {
        for point in points {
            // A click inside an object already seen would only find that object again.
            let px = ((point.0 / sx) as usize).min(small_w as usize - 1);
            let py = ((point.1 / sy) as usize).min(small_h as usize - 1);
            if seen.iter().any(|bits| bits[py * small_w as usize + px]) {
                continue;
            }
            let mask = run_sam_decoder(&models.sam_decoder, &embeddings, point, point, None, None)
                .map_err(|e| e.to_string())?;
            let small = image::imageops::resize(&mask, small_w, small_h, image::imageops::FilterType::Nearest);
            let bits: Vec<bool> = small.pixels().map(|p| p.0[0] > 127).collect();
            let share = bits.iter().filter(|b| **b).count() as f64 / bits.len() as f64;
            if !(0.0015..=0.06).contains(&share) {
                continue;
            }
            let duplicate = seen.iter().any(|other| {
                let inter = other.iter().zip(&bits).filter(|(a, b)| **a && **b).count();
                let union = other.iter().zip(&bits).filter(|(a, b)| **a || **b).count();
                union > 0 && inter as f64 / union as f64 > 0.6
            });
            if duplicate {
                continue;
            }
            let (mut x0, mut y0, mut x1, mut y1) = (small_w, small_h, 0, 0);
            for (i, b) in bits.iter().enumerate() {
                if *b {
                    let (x, y) = (i as u32 % small_w, i as u32 / small_w);
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
            seen.push(bits);

            let bbox = (x0 as f64 * sx, y0 as f64 * sy, (x1 + 1) as f64 * sx, (y1 + 1) as f64 * sy);
            let pad_x = (bbox.2 - bbox.0) * 0.2;
            let pad_y = (bbox.3 - bbox.1) * 0.2;
            let cx0 = (bbox.0 - pad_x).max(0.0) as u32;
            let cy0 = (bbox.1 - pad_y).max(0.0) as u32;
            let cx1 = ((bbox.2 + pad_x) as u32).min(w);
            let cy1 = ((bbox.3 + pad_y) as u32).min(h);
            if cx1 <= cx0 + 16 || cy1 <= cy0 + 16 {
                continue;
            }
            let crop = warped_image.crop_imm(cx0, cy0, cx1 - cx0, cy1 - cy0);
            let probs = crate::tagging::clip_label_probabilities(&crop, &clip.model, &clip.tokenizer, &LABELS)
                .map_err(|e| e.to_string())?;
            let person = probs[0] + probs[1];
            let best_other = probs[2..].iter().cloned().fold(0.0f32, f32::max);
            if person < 0.6 || person < best_other * 2.0 {
                continue;
            }

            // A click often catches only part of a person (a lit shirt, not dark trousers): add SAM's mask for the
            // box around them, unless it fills most of that box, which means it grabbed the ground instead.
            let mut mask = mask;
            let (px0, py0) = ((bbox.0 - pad_x * 0.5).max(0.0), (bbox.1 - pad_y * 0.5).max(0.0));
            let (px1, py1) = ((bbox.2 + pad_x * 0.5).min(w as f64), (bbox.3 + pad_y * 0.5).min(h as f64));
            let whole = run_sam_decoder(&models.sam_decoder, &embeddings, (px0, py0), (px1, py1), None, None)
                .map_err(|e| e.to_string())?;
            let (bx0, by0, bx1, by1) = (px0 as u32, py0 as u32, (px1 as u32).min(w), (py1 as u32).min(h));
            let box_area = ((bx1 - bx0) as u64 * (by1 - by0) as u64).max(1);
            let mut inside = 0u64;
            let mut outside = 0u64;
            for (x, y, px) in whole.enumerate_pixels() {
                if px.0[0] > 127 {
                    if x >= bx0 && x < bx1 && y >= by0 && y < by1 {
                        inside += 1;
                    } else {
                        outside += 1;
                    }
                }
            }
            if (inside as f64) < box_area as f64 * 0.7 && (outside as f64) < inside as f64 * 0.25 {
                for (dst, src) in mask.pixels_mut().zip(whole.pixels()) {
                    dst.0[0] = dst.0[0].max(src.0[0]);
                }
            }

            // One person often comes back as several pieces (legs, coat, head): merge into the person whose
            // box overlaps most of the smaller box, so each person ends up as one erase layer.
            let overlap = |a: &(f64, f64, f64, f64), b: &(f64, f64, f64, f64)| {
                let iw = (a.2.min(b.2) - a.0.max(b.0)).max(0.0);
                let ih = (a.3.min(b.3) - a.1.max(b.1)).max(0.0);
                let smaller = ((a.2 - a.0) * (a.3 - a.1)).min((b.2 - b.0) * (b.3 - b.1)).max(1.0);
                iw * ih / smaller
            };
            let fits_person = |a: &(f64, f64, f64, f64)| {
                let merged = (a.0.min(bbox.0), a.1.min(bbox.1), a.2.max(bbox.2), a.3.max(bbox.3));
                (merged.2 - merged.0) * (merged.3 - merged.1) <= w as f64 * h as f64 * 0.08
            };
            if let Some(person) = people.iter_mut().find(|(_, b)| overlap(b, &bbox) > 0.5 && fits_person(b)) {
                for (dst, src) in person.0.pixels_mut().zip(mask.pixels()) {
                    dst.0[0] = dst.0[0].max(src.0[0]);
                }
                person.1 = (person.1.0.min(bbox.0), person.1.1.min(bbox.1), person.1.2.max(bbox.2), person.1.3.max(bbox.3));
            } else {
                people.push((mask, bbox));
            }
        }
    }

    log::info!("Remove people: {} people found among {} proposals", people.len(), seen.len());
    people
        .into_iter()
        .map(|(mask, (x0, y0, x1, y1))| {
            log::info!(
                "Remove people: person box {:.0}x{:.0} at ({:.0}, {:.0}), {:.2}% of the photo",
                x1 - x0,
                y1 - y0,
                x0,
                y0,
                (x1 - x0) * (y1 - y0) / (w as f64 * h as f64) * 100.0
            );
            let mask = grow_person_mask(&mask, y1 - y0);
            Ok(AiSubjectMaskParameters {
                start_x: x0,
                start_y: y0,
                end_x: x1,
                end_y: y1,
                mask_data_base64: Some(encode_to_base64_png(&mask)?),
                rotation: Some(0.0),
                flip_horizontal: Some(false),
                flip_vertical: Some(false),
                orientation_steps: Some(0),
            })
        })
        .collect()
}


/// Grows a person's mask so motion blur and soft edges around them are erased too: a max filter sized to the
/// person (wider sideways, where motion trails usually run), done at quarter resolution to stay fast.
fn grow_person_mask(mask: &GrayImage, box_height: f64) -> GrayImage {
    let (w, h) = mask.dimensions();
    let (sw, sh) = ((w / 4).max(1), (h / 4).max(1));
    let small = image::imageops::resize(mask, sw, sh, image::imageops::FilterType::Triangle);
    let ry = ((box_height * 0.03) / 4.0).round().clamp(1.0, 8.0) as i64;
    let rx = (ry as f64 * 1.5).round() as i64;
    let (sw_i, sh_i) = (sw as i64, sh as i64);
    let mut across = vec![0u8; (sw * sh) as usize];
    for y in 0..sh_i {
        for x in 0..sw_i {
            let mut m = 0u8;
            for xx in (x - rx).max(0)..=(x + rx).min(sw_i - 1) {
                m = m.max(small.get_pixel(xx as u32, y as u32).0[0]);
            }
            across[(y * sw_i + x) as usize] = m;
        }
    }
    let mut grown = GrayImage::new(sw, sh);
    for y in 0..sh_i {
        for x in 0..sw_i {
            let mut m = 0u8;
            for yy in (y - ry).max(0)..=(y + ry).min(sh_i - 1) {
                m = m.max(across[(yy * sw_i + x) as usize]);
            }
            grown.put_pixel(x as u32, y as u32, image::Luma([m]));
        }
    }
    let full = image::imageops::resize(&grown, w, h, image::imageops::FilterType::Triangle);
    GrayImage::from_fn(w, h, |x, y| image::Luma([if full.get_pixel(x, y).0[0] > 96 { 255 } else { 0 }]))
}
