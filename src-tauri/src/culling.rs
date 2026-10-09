use crate::app_settings::load_settings;
use image::{DynamicImage, GenericImageView, GrayImage, imageops};
use image_hasher::{HashAlg, HasherConfig};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::{AppHandle, Emitter};

use crate::image_loader;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CullingSettings {
    pub similarity_threshold: u32,
    pub blur_threshold: f64,
    pub group_similar: bool,
    pub filter_blurry: bool,
    /// Flag frames where someone's eyes are closed. Needs the Faces models (fetched when this is on).
    #[serde(default)]
    pub filter_closed_eyes: bool,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImageAnalysisResult {
    pub path: String,
    pub quality_score: f64,
    pub sharpness_metric: f64,
    pub center_focus_metric: f64,
    pub exposure_metric: f64,
    pub width: u32,
    pub height: u32,
    /// Faces found, and how many of them have their eyes closed (0 / 0 when the check was off).
    pub faces: u32,
    pub eyes_closed: u32,
    /// The least-open face's eye aspect ratio (0 when no face was judged): ~0.3 wide open, under 0.15 closed.
    pub eye_openness: f32,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CullGroup {
    pub representative: ImageAnalysisResult,
    pub duplicates: Vec<ImageAnalysisResult>,
}

#[derive(Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct CullingSuggestions {
    pub similar_groups: Vec<CullGroup>,
    pub blurry_images: Vec<ImageAnalysisResult>,
    pub closed_eyes_images: Vec<ImageAnalysisResult>,
    pub failed_paths: Vec<String>,
}

#[derive(Serialize, Clone)]
struct CullingProgress {
    current: usize,
    total: usize,
    stage: String,
}

struct ImageAnalysisData {
    hash: image_hasher::ImageHash,
    result: ImageAnalysisResult,
    focus_tiles: Vec<f64>,
    captured_ms: Option<i64>,
}

const WEIGHT_SHARPNESS: f64 = 0.40;
const WEIGHT_CENTER_FOCUS: f64 = 0.35;
const WEIGHT_EXPOSURE: f64 = 0.25;

/// Frames judged against each other at this size: enough to tell a sharp eye from a soft one.
const FOCUS_DIM: u32 = 1600;
const FOCUS_GRID: u32 = 8;
/// ponytail: one burst = frames shot within this gap of the previous one; make it a setting if bursts split wrongly.
const BURST_GAP_MS: i64 = 5000;
/// Frames this close are one burst unless the picture changed beyond FAST_LOOK_FACTOR x the similarity
/// threshold. Measured: a 10 fps burst panning with a stork changes its background by a hash distance of 36-69
/// frame to frame (the default threshold is 28) and is still one burst; a swing to a different scene flips about
/// half the bits (100+) and still splits.
const FAST_GAP_MS: i64 = 1000;
const FAST_LOOK_FACTOR: u32 = 3;

fn calculate_laplacian_variance(image: &GrayImage) -> f64 {
    let (width, height) = image.dimensions();
    if width < 3 || height < 3 {
        return 0.0;
    }

    let mut laplacian_values = Vec::with_capacity(((width - 2) * (height - 2)) as usize);
    let mut sum = 0.0;

    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let p_center = image.get_pixel(x, y)[0] as i32;
            let p_north = image.get_pixel(x, y - 1)[0] as i32;
            let p_south = image.get_pixel(x, y + 1)[0] as i32;
            let p_west = image.get_pixel(x - 1, y)[0] as i32;
            let p_east = image.get_pixel(x + 1, y)[0] as i32;
            let conv_val = (p_north + p_south + p_west + p_east - 4 * p_center) as f64;
            laplacian_values.push(conv_val);
            sum += conv_val;
        }
    }

    if laplacian_values.is_empty() {
        return 0.0;
    }
    let mean = sum / laplacian_values.len() as f64;

    laplacian_values
        .iter()
        .map(|v| (v - mean).powi(2))
        .sum::<f64>()
        / laplacian_values.len() as f64
}

/// Sharpness of each cell in a grid x grid split of the image, row by row.
fn tile_sharpness(image: &GrayImage, grid: u32) -> Vec<f64> {
    let (w, h) = image.dimensions();
    let (tw, th) = (w / grid, h / grid);
    if tw < 3 || th < 3 {
        return vec![calculate_laplacian_variance(image)];
    }
    (0..grid * grid)
        .map(|i| {
            let tile = imageops::crop_imm(image, (i % grid) * tw, (i / grid) * th, tw, th).to_image();
            calculate_laplacian_variance(&tile)
        })
        .collect()
}

/// Focus of each frame relative to the best one (1.0), measured on the few tiles the whole burst is
/// sharpest in. That is where the photographer focused, so a sharper background never wins.
fn focus_scores(tiles: &[&[f64]]) -> Vec<f64> {
    let n = tiles.iter().map(|t| t.len()).min().unwrap_or(0);
    let mut totals: Vec<(usize, f64)> = (0..n).map(|k| (k, tiles.iter().map(|t| t[k]).sum())).collect();
    totals.sort_by(|a, b| b.1.total_cmp(&a.1));
    let top: Vec<usize> = totals.iter().take(3).map(|(k, _)| *k).collect();
    let raw: Vec<f64> = tiles.iter().map(|t| top.iter().map(|&k| t[k]).sum()).collect();
    let best = raw.iter().cloned().fold(0.0, f64::max);
    raw.iter().map(|r| if best > 0.0 { r / best } else { 0.0 }).collect()
}

/// Walks frames in capture order and starts a new burst when the gap to the previous frame is too
/// long or the picture changed too much.
fn split_bursts(
    order: &[usize],
    times: &[Option<i64>],
    dist: impl Fn(usize, usize) -> u32,
    threshold: u32,
) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for &i in order {
        let joins = groups.last().is_some_and(|g| {
            let prev = g[g.len() - 1];
            let Some(gap) = times[prev].zip(times[i]).map(|(a, b)| b - a) else { return false };
            let d = dist(prev, i);
            gap <= BURST_GAP_MS && (d <= threshold || (gap <= FAST_GAP_MS && d <= threshold * FAST_LOOK_FACTOR))
        });
        match groups.last_mut() {
            Some(g) if joins => g.push(i),
            _ => groups.push(vec![i]),
        }
    }
    groups
}

fn calculate_exposure_metric(image: &GrayImage) -> f64 {
    let histogram = imageproc::stats::histogram(image);
    let total_pixels = (image.width() * image.height()) as f64;
    if total_pixels == 0.0 {
        return 0.0;
    }

    let clip_threshold_dark = 5;
    let clip_threshold_bright = 250;

    let dark_pixels = histogram.channels[0][0..clip_threshold_dark]
        .iter()
        .sum::<u32>() as f64;
    let bright_pixels = histogram.channels[0][clip_threshold_bright..256]
        .iter()
        .sum::<u32>() as f64;

    let dark_clip_ratio = dark_pixels / total_pixels;
    let bright_clip_ratio = bright_pixels / total_pixels;

    let penalty = (dark_clip_ratio * 5.0) + (bright_clip_ratio * 5.0);

    (1.0f64 - penalty).max(0.0)
}

fn analyze_image(
    path: &str,
    hasher: &image_hasher::Hasher,
    settings: &crate::app_settings::AppSettings,
    faces: Option<&crate::eyes::FaceModels>,
) -> Result<ImageAnalysisData, String> {
    const ANALYSIS_DIM: u32 = 720;

    if crate::file_management::is_cloud_placeholder(Path::new(path)) {
        return Err(format!("'{}' is stored in iCloud and not downloaded", path));
    }

    // The JPEG preview inside a RAW is enough to judge focus and far faster than decoding the RAW.
    let embedded = if crate::formats::is_raw_file(path) {
        crate::file_management::try_load_embedded_raw_preview(Path::new(path), FOCUS_DIM)
    } else {
        None
    };
    let img: DynamicImage = match embedded {
        Some(img) => img,
        None => {
            let file_bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            image_loader::load_base_image_from_bytes(&file_bytes, path, true, settings, None)
                .map_err(|e| e.to_string())?
        }
    };

    let (width, height) = img.dimensions();
    let thumbnail = img.thumbnail(ANALYSIS_DIM, ANALYSIS_DIM);
    let gray_thumbnail = thumbnail.to_luma8();

    // Sharpest region rather than the whole frame, so shallow depth of field isn't called blurry.
    let sharpness_metric = tile_sharpness(&gray_thumbnail, 4)
        .into_iter()
        .fold(0.0, f64::max);
    let exposure_metric = calculate_exposure_metric(&gray_thumbnail);

    let (thumb_w, thumb_h) = gray_thumbnail.dimensions();
    let center_crop = imageops::crop_imm(
        &gray_thumbnail,
        thumb_w / 4,
        thumb_h / 4,
        thumb_w / 2,
        thumb_h / 2,
    )
    .to_image();
    let center_focus_metric = calculate_laplacian_variance(&center_crop);

    let normalized_sharpness = ((sharpness_metric + 1.0).log10() / 3.5).min(1.0);
    let normalized_center_focus = ((center_focus_metric + 1.0).log10() / 3.5).min(1.0);

    let quality_score = (normalized_sharpness * WEIGHT_SHARPNESS)
        + (normalized_center_focus * WEIGHT_CENTER_FOCUS)
        + (exposure_metric * WEIGHT_EXPOSURE);

    let hash = hasher.hash_image(&thumbnail);
    let focus_img = img.thumbnail(FOCUS_DIM, FOCUS_DIM);
    let focus_tiles = tile_sharpness(&focus_img.to_luma8(), FOCUS_GRID);
    // Eyes are judged on the 1600 px copy: enough pixels for an eyelid on a face a tenth of the frame wide.
    let eyes = faces.and_then(|m| match crate::eyes::eye_state(m, &focus_img) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("Eye check failed on {path}: {e}");
            None
        }
    });
    let captured_ms = crate::exif_processing::try_get_exif_creation_date(Path::new(path))
        .map(|date| date.timestamp_millis());

    Ok(ImageAnalysisData {
        hash,
        focus_tiles,
        captured_ms,
        result: ImageAnalysisResult {
            path: path.to_string(),
            quality_score,
            sharpness_metric,
            center_focus_metric,
            exposure_metric,
            width,
            height,
            faces: eyes.map_or(0, |e| e.faces),
            eyes_closed: eyes.map_or(0, |e| e.closed),
            eye_openness: eyes.map_or(0.0, |e| if e.min_openness.is_finite() { e.min_openness } else { 0.0 }),
        },
    })
}

/// Best frame first; each frame's score becomes its focus relative to the best one.
fn make_group(analyses: &[ImageAnalysisData], indices: &[usize]) -> CullGroup {
    let tiles: Vec<&[f64]> = indices.iter().map(|&i| analyses[i].focus_tiles.as_slice()).collect();
    let scored: Vec<ImageAnalysisResult> = indices
        .iter()
        .zip(focus_scores(&tiles))
        .map(|(&i, score)| ImageAnalysisResult {
            quality_score: score,
            ..analyses[i].result.clone()
        })
        .collect();
    // Eyes first, then focus: a wide-open frame beats a sharper one taken just after a blink.
    let tiers = eye_tiers(&scored);
    let mut order: Vec<usize> = (0..scored.len()).collect();
    order.sort_by(|&a, &b| tiers[a].cmp(&tiers[b]).then(scored[b].quality_score.total_cmp(&scored[a].quality_score)));
    let mut ranked: Vec<ImageAnalysisResult> = order.into_iter().map(|i| scored[i].clone()).collect();
    let representative = ranked.remove(0);
    CullGroup {
        representative,
        duplicates: ranked,
    }
}

#[tauri::command]
pub async fn cull_images(
    paths: Vec<String>,
    settings: CullingSettings,
    app_handle: AppHandle,
    state: tauri::State<'_, crate::app_state::AppState>,
) -> Result<CullingSuggestions, String> {
    if paths.is_empty() {
        return Ok(CullingSuggestions::default());
    }

    let app_settings = load_settings(app_handle.clone()).unwrap_or_default();
    // Closed eyes need the Faces models: fetched here the first time the check is on, then loaded once.
    let faces = if settings.filter_closed_eyes {
        let dir = crate::ai_processing::ensure_face_models(&app_handle, &state.ai_init_lock)
            .await
            .map_err(|e| format!("Could not get the face models for closed eyes: {e}"))?;
        crate::eyes::installed(&dir)
    } else {
        None
    };

    let total_count = paths.len();
    let completed_count = Arc::new(AtomicUsize::new(0));
    let _ = app_handle.emit("culling-start", total_count);

    let hasher = HasherConfig::new()
        .hash_alg(HashAlg::DoubleGradient)
        .hash_size(16, 16)
        .to_hasher();

    let analysis_results: Vec<Result<ImageAnalysisData, (String, String)>> = paths
        .par_iter()
        .map(|path| {
            let completed = completed_count.fetch_add(1, Ordering::Relaxed) + 1;
            let _ = app_handle.emit(
                "culling-progress",
                CullingProgress {
                    current: completed,
                    total: total_count,
                    stage: "Analyzing images...".to_string(),
                },
            );

            analyze_image(path, &hasher, &app_settings, faces.as_deref()).map_err(|e| (path.to_string(), e))
        })
        .collect();

    let mut successful_analyses = Vec::new();
    let mut failed_paths = Vec::new();
    for res in analysis_results {
        match res {
            Ok(data) => successful_analyses.push(data),
            Err((path, error)) => {
                eprintln!("Failed to analyze image {}: {}", path, error);
                failed_paths.push(path);
            }
        }
    }

    let _ = app_handle.emit(
        "culling-progress",
        CullingProgress {
            current: total_count,
            total: total_count,
            stage: "Grouping similar images...".to_string(),
        },
    );

    let mut suggestions = CullingSuggestions {
        failed_paths,
        ..Default::default()
    };
    let mut processed_indices = vec![false; successful_analyses.len()];

    if settings.group_similar {
        let similar = |a: usize, b: usize| {
            successful_analyses[a].hash.dist(&successful_analyses[b].hash) <= settings.similarity_threshold
        };

        // Photos with a capture time: bursts in shooting order.
        let times: Vec<Option<i64>> = successful_analyses.iter().map(|a| a.captured_ms).collect();
        let mut timed: Vec<usize> = (0..successful_analyses.len()).filter(|&i| times[i].is_some()).collect();
        timed.sort_by(|&a, &b| {
            times[a]
                .cmp(&times[b])
                .then_with(|| successful_analyses[a].result.path.cmp(&successful_analyses[b].result.path))
        });
        let dist = |a: usize, b: usize| successful_analyses[a].hash.dist(&successful_analyses[b].hash);
        for burst in split_bursts(&timed, &times, dist, settings.similarity_threshold) {
            if burst.len() > 1 {
                burst.iter().for_each(|&i| processed_indices[i] = true);
                suggestions.similar_groups.push(make_group(&successful_analyses, &burst));
            }
        }

        // Photos without one: group by look alone.
        for i in 0..successful_analyses.len() {
            if processed_indices[i] || times[i].is_some() {
                continue;
            }

            let mut current_group_indices = vec![];
            let mut queue = VecDeque::new();

            processed_indices[i] = true;
            current_group_indices.push(i);
            queue.push_back(i);

            while let Some(current_idx) = queue.pop_front() {
                for j in (current_idx + 1)..successful_analyses.len() {
                    if processed_indices[j] || times[j].is_some() {
                        continue;
                    }
                    if similar(current_idx, j) {
                        processed_indices[j] = true;
                        current_group_indices.push(j);
                        queue.push_back(j);
                    }
                }
            }

            if current_group_indices.len() > 1 {
                suggestions
                    .similar_groups
                    .push(make_group(&successful_analyses, &current_group_indices));
            }
        }
    }

    if settings.filter_blurry {
        for i in 0..successful_analyses.len() {
            if !processed_indices[i] {
                let item = &successful_analyses[i];
                if item.result.sharpness_metric < settings.blur_threshold {
                    suggestions.blurry_images.push(item.result.clone());
                }
            }
        }
        suggestions.blurry_images.sort_by(|a, b| {
            a.sharpness_metric
                .partial_cmp(&b.sharpness_metric)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // Closed eyes are listed wherever they are, burst or not: a blink inside a burst is the case that matters.
    if settings.filter_closed_eyes {
        suggestions.closed_eyes_images = successful_analyses
            .iter()
            .filter(|a| a.result.eyes_closed > 0)
            .map(|a| a.result.clone())
            .collect();
    }

    let _ = app_handle.emit("culling-complete", &suggestions);
    Ok(suggestions)
}

/// How good a frame's eyes are, for picking within a burst: 0 everyone's eyes fully open, 1 open but heavy-lidded
/// (the frames either side of a blink), 2 someone's eyes closed. Focus only decides within a tier.
/// "Fully open" is relative to the burst itself -- at least EYES_FULLY of the most-open frame -- because eyes
/// differ: a face that is always a little hooded must not have every frame demoted. Frames without a judged
/// face are tier 0, so bursts with nobody in them rank on focus alone, exactly as before.
// ponytail: EYES_FULLY set on one portrait burst, as the app measures it: wide open 94-100% of the burst's best,
// lids still rising after the blink 78-88% -- and 88% still looked heavy-lidded beside the wide-open frames.
const EYES_FULLY: f32 = 0.92;

fn eye_tiers(frames: &[ImageAnalysisResult]) -> Vec<u8> {
    let best = frames.iter().filter(|f| f.faces > 0).map(|f| f.eye_openness).fold(0.0f32, f32::max);
    frames
        .iter()
        .map(|f| {
            if f.faces == 0 || best <= 0.0 {
                0
            } else if f.eyes_closed > 0 {
                2
            } else if f.eye_openness < best * EYES_FULLY {
                1
            } else {
                0
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(focus: f64, faces: u32, closed: u32, open: f32) -> ImageAnalysisResult {
        ImageAnalysisResult {
            path: String::new(), quality_score: focus, sharpness_metric: 0.0, center_focus_metric: 0.0,
            exposure_metric: 0.0, width: 0, height: 0, faces, eyes_closed: closed, eye_openness: open,
        }
    }

    #[test]
    fn heavy_lidded_frames_rank_behind_wide_open_ones() {
        // the portrait burst as the app measured it: wide open 0.280-0.298, closing 0.214, blink 0.085,
        // lids rising 0.234 and 0.262 -- the last at 88% of the best, still heavy-lidded to the eye
        let f = [frame(0.89, 1, 0, 0.292), frame(0.75, 1, 0, 0.298), frame(0.95, 1, 0, 0.214),
                 frame(0.97, 1, 1, 0.085), frame(1.0, 1, 0, 0.234), frame(0.94, 1, 0, 0.262)];
        assert_eq!(eye_tiers(&f), vec![0, 0, 1, 2, 1, 1]);
        // no faces anywhere: every frame tier 0, focus alone decides
        let birds = [frame(0.5, 0, 0, 0.0), frame(1.0, 0, 0, 0.0)];
        assert_eq!(eye_tiers(&birds), vec![0, 0]);
    }

    #[test]
    fn bursts_split_on_time_gaps_and_scene_changes() {
        // 3 -> 4 is a swing to a new scene: half the bits change, so it splits even 200 ms apart
        let times = [Some(0), Some(300), Some(9_000), Some(9_200), Some(9_400), None];
        let groups = split_bursts(&[0, 1, 2, 3, 4], &times, |a, b| if a == 3 && b == 4 { 120 } else { 10 }, 28);
        assert_eq!(groups, vec![vec![0, 1], vec![2, 3], vec![4]]);
    }

    #[test]
    fn a_fast_burst_panning_with_its_subject_stays_one_burst() {
        // 10 fps, the background sliding by as the camera follows: distances like the stork burst's (36-69)
        let times = [Some(0), Some(100), Some(200), Some(300)];
        let d = [52, 67, 36];
        let groups = split_bursts(&[0, 1, 2, 3], &times, |a, _| d[a], 28);
        assert_eq!(groups, vec![vec![0, 1, 2, 3]]);
        // the same change of look three seconds apart is a new picture, not a burst
        let slow = [Some(0), Some(3_000)];
        assert_eq!(split_bursts(&[0, 1], &slow, |_, _| 52, 28), vec![vec![0], vec![1]]);
    }

    #[test]
    fn focus_follows_the_subject_not_the_background() {
        // Tile 0 is the subject. Frame b has a sharper background but a softer subject.
        let a = [100.0, 1.0, 1.0, 1.0];
        let b = [60.0, 30.0, 1.0, 1.0];
        let scores = focus_scores(&[&a, &b]);
        assert_eq!(scores[0], 1.0);
        assert!(scores[1] < 0.95);
    }
}
