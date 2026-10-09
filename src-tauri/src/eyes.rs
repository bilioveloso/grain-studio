//! Closed eyes, for culling: who in the frame blinked.
//!
//! Two small models, both optional downloads (Settings -> AI Models -> Faces):
//! * YuNet (OpenCV Zoo, MIT) finds the faces. Its raw outputs are decoded here -- per stride, a score from
//!   class x objectness, a box from the cell offset and log size -- then non-maximum suppression.
//! * MediaPipe's face landmarks (Google, Apache-2.0) trace each eye on a square crop with margin round the face.
//!
//! Open or closed is the eye aspect ratio: the eyelid gap over the eye's width, averaged over two pairs of eyelid
//! points and over both eyes, so a turned head (one eye foreshortened) still reads right. Measured on a portrait
//! clip: open eyes ~0.30, a full blink ~0.03-0.06, a turned head mid-blink 0.115. Under CLOSED is closed.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use image::{DynamicImage, GenericImageView, imageops::FilterType};
use ndarray::Array4;
use ort::session::Session;
use ort::value::Tensor;

use crate::ai_processing::{FACE_MESH_FILENAME, YUNET_FILENAME};

// ponytail: one threshold for everyone, set on one clip. Expose it in the culling settings if squinting
// portraits or very hooded eyes start getting flagged.
const CLOSED: f32 = 0.15;
const YUNET_IN: u32 = 640;
const MIN_SCORE: f32 = 0.6;
const MIN_FACE_PX: f32 = 40.0; // smaller faces at 640 px are too few pixels to judge an eyelid
const MESH_IN: u32 = 256;
const MARGIN: f32 = 1.6; // the face box, grown to the square the landmark model was trained on

// MediaPipe 478-point mesh: eye corners, then two upper/lower eyelid pairs
const EYES: [(usize, usize, [(usize, usize); 2]); 2] =
    [(33, 133, [(160, 144), (158, 153)]), (362, 263, [(385, 380), (387, 373)])];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EyeState {
    pub faces: u32,
    pub closed: u32,
    /// the least-open face's eye aspect ratio
    pub min_openness: f32,
}

pub struct FaceModels {
    detector: Mutex<Session>,
    mesh: Mutex<Session>,
}

static MODELS: Mutex<Option<(PathBuf, Arc<FaceModels>)>> = Mutex::new(None);

/// Both models if the user installed them, loaded once. Never downloads.
pub fn installed(models_dir: &Path) -> Option<Arc<FaceModels>> {
    let (d, m) = (models_dir.join(YUNET_FILENAME), models_dir.join(FACE_MESH_FILENAME));
    if !d.exists() || !m.exists() {
        return None;
    }
    let mut slot = MODELS.lock().unwrap();
    if let Some((p, f)) = slot.as_ref()
        && *p == models_dir
    {
        return Some(f.clone());
    }
    let load = |p: &Path| Session::builder().and_then(|b| b.commit_from_file(p));
    match (load(&d), load(&m)) {
        (Ok(detector), Ok(mesh)) => {
            let f = Arc::new(FaceModels { detector: Mutex::new(detector), mesh: Mutex::new(mesh) });
            *slot = Some((models_dir.to_path_buf(), f.clone()));
            Some(f)
        }
        (a, b) => {
            log::warn!("Face models would not load: {:?} / {:?}", a.err(), b.err());
            None
        }
    }
}

pub fn unload() {
    *MODELS.lock().unwrap() = None;
}

/// Faces in the frame and how many have their eyes closed. None when nothing could be judged.
pub fn eye_state(models: &FaceModels, img: &DynamicImage) -> Result<Option<EyeState>> {
    let boxes = detect(models, img)?;
    if boxes.is_empty() {
        return Ok(Some(EyeState { faces: 0, closed: 0, min_openness: f32::NAN }));
    }
    let mut closed = 0;
    let mut min_open = f32::INFINITY;
    for b in &boxes {
        let open = openness(models, img, *b)?;
        min_open = min_open.min(open);
        if open < CLOSED {
            closed += 1;
        }
    }
    Ok(Some(EyeState { faces: boxes.len() as u32, closed, min_openness: min_open }))
}

/// YuNet: [x, y, w, h] boxes in image pixels.
fn detect(models: &FaceModels, img: &DynamicImage) -> Result<Vec<[f32; 4]>> {
    let (w, h) = img.dimensions();
    let s = YUNET_IN as f32 / w.max(h) as f32;
    let (sw, sh) = (((w as f32) * s).round() as u32, ((h as f32) * s).round() as u32);
    let small = img.resize_exact(sw, sh, FilterType::Triangle).to_rgb8();
    let n = YUNET_IN as usize;
    let mut input = Array4::<f32>::zeros((1, 3, n, n));
    for (x, y, p) in small.enumerate_pixels() {
        // YuNet reads BGR, 0..255, unnormalised
        for (c, v) in [p[2], p[1], p[0]].into_iter().enumerate() {
            input[[0, c, y as usize, x as usize]] = v as f32;
        }
    }
    let mut found: Vec<(f32, [f32; 4])> = Vec::new();
    {
        let mut sess = models.detector.lock().unwrap();
        let out = sess.run(ort::inputs!["input" => Tensor::from_array(input)?])?;
        for stride in [8usize, 16, 32] {
            let cls = out[format!("cls_{stride}").as_str()].try_extract_array::<f32>()?.to_owned();
            let obj = out[format!("obj_{stride}").as_str()].try_extract_array::<f32>()?.to_owned();
            let bbox = out[format!("bbox_{stride}").as_str()].try_extract_array::<f32>()?.to_owned();
            let cols = n / stride;
            let cells = cls.len();
            let (cls, obj, bbox) = (cls.as_slice().unwrap(), obj.as_slice().unwrap(), bbox.as_slice().unwrap());
            for i in 0..cells {
                let score = (cls[i].clamp(0.0, 1.0) * obj[i].clamp(0.0, 1.0)).sqrt();
                if score <= MIN_SCORE {
                    continue;
                }
                let (r, c) = ((i / cols) as f32, (i % cols) as f32);
                let st = stride as f32;
                let (cx, cy) = ((c + bbox[i * 4]) * st, (r + bbox[i * 4 + 1]) * st);
                let (bw, bh) = (bbox[i * 4 + 2].exp() * st, bbox[i * 4 + 3].exp() * st);
                if bw.min(bh) < MIN_FACE_PX {
                    continue;
                }
                found.push((score, [(cx - bw / 2.0) / s, (cy - bh / 2.0) / s, bw / s, bh / s]));
            }
        }
    }
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut keep: Vec<[f32; 4]> = Vec::new();
    for (_, b) in found {
        if keep.iter().all(|k| iou(k, &b) < 0.3) {
            keep.push(b);
        }
    }
    Ok(keep)
}

fn iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let ix = ((a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0])).max(0.0);
    let iy = ((a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1])).max(0.0);
    ix * iy / (a[2] * a[3] + b[2] * b[3] - ix * iy + 1e-9)
}

/// The face's eye aspect ratio, both eyes averaged.
fn openness(models: &FaceModels, img: &DynamicImage, b: [f32; 4]) -> Result<f32> {
    let (w, h) = img.dimensions();
    let side = b[2].max(b[3]) * MARGIN;
    let (cx, cy) = (b[0] + b[2] / 2.0, b[1] + b[3] / 2.0);
    let (x0, y0) = (cx - side / 2.0, cy - side / 2.0);
    let rgb = img.to_rgb8();
    let n = MESH_IN as usize;
    let mut input = Array4::<f32>::zeros((1, n, n, 3));
    // the square crop, sampled straight into the model's 256 x 256; outside the photo is black
    for y in 0..n {
        for x in 0..n {
            let sx = (x0 + (x as f32 + 0.5) * side / n as f32) as i64;
            let sy = (y0 + (y as f32 + 0.5) * side / n as f32) as i64;
            if sx >= 0 && sy >= 0 && (sx as u32) < w && (sy as u32) < h {
                let p = rgb.get_pixel(sx as u32, sy as u32);
                for c in 0..3 {
                    input[[0, y, x, c]] = p[c] as f32 / 255.0;
                }
            }
        }
    }
    let pts = {
        let mut sess = models.mesh.lock().unwrap();
        let out = sess.run(ort::inputs![Tensor::from_array(input)?])?;
        out[0].try_extract_array::<f32>()?.to_owned()
    };
    let pts = pts.as_slice().unwrap();
    Ok(ear(|i| (pts[i * 3], pts[i * 3 + 1])))
}

/// Eye aspect ratio from a landmark lookup: (two eyelid gaps) / (2 x eye width), averaged over both eyes.
fn ear(p: impl Fn(usize) -> (f32, f32)) -> f32 {
    let d = |a: usize, b: usize| {
        let (pa, pb) = (p(a), p(b));
        ((pa.0 - pb.0).powi(2) + (pa.1 - pb.1).powi(2)).sqrt()
    };
    EYES.iter()
        .map(|&(c0, c1, lids)| (d(lids[0].0, lids[0].1) + d(lids[1].0, lids[1].1)) / (2.0 * d(c0, c1) + 1e-6))
        .sum::<f32>()
        / EYES.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An eye drawn as points: corners 100 apart, lids `gap` apart.
    fn eye_points(gap: f32) -> impl Fn(usize) -> (f32, f32) {
        move |i| match i {
            33 | 362 => (0.0, 0.0),
            133 | 263 => (100.0, 0.0),
            160 | 158 | 385 | 387 => (50.0, -gap / 2.0),
            144 | 153 | 380 | 373 => (50.0, gap / 2.0),
            _ => (0.0, 0.0),
        }
    }

    #[test]
    fn open_and_closed_eyes_land_either_side_of_the_line() {
        assert!(ear(eye_points(30.0)) > CLOSED, "an open eye (gap 30 of 100) read as closed");
        assert!(ear(eye_points(4.0)) < CLOSED, "a blink (gap 4 of 100) read as open");
    }

    #[test]
    fn overlapping_detections_are_one_face() {
        assert!(iou(&[0.0, 0.0, 100.0, 100.0], &[10.0, 10.0, 100.0, 100.0]) > 0.3);
        assert!(iou(&[0.0, 0.0, 100.0, 100.0], &[300.0, 0.0, 100.0, 100.0]) < 0.01);
    }
}
