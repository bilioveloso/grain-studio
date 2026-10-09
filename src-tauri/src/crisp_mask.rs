//! Subject masks that sit on the photograph's own edge.
//!
//! SAM draws its mask on a 256x256 grid stretched over whatever it was shown. Shown a whole 4000 px photo, one
//! cell is ~16 px: a crane's 4 px struts and the sky between them fall inside single cells, so it answers with
//! an envelope round the subject, and the old pipeline then blurred that envelope by ~40 px to hide the grid.
//! That blur is the halo.
//!
//! Two steps instead, each fixing a different failure:
//!
//! 1. **Crop pass.** Run SAM a second time on a crop around the subject, so the same 256 cells cover the
//!    subject instead of the frame. On the harbour crane that is 15.7 -> 5.1 px a cell, and the edge lands on
//!    the steel. A subject that already fills the frame gains little, and skips it.
//! 2. **Colour matte.** Near and inside the mask, every pixel is some mix of the subject's colour F and the
//!    background's colour B behind it. Estimate both locally (a sky is blue at the top of a crane and orange at
//!    its feet), and project each pixel onto the line between them. Sky seen through a lattice projects to ~0,
//!    steel to ~1, a 2 px strut blurred over 4 px to the fractions between -- a matte, not a blur.
//!
//! Measured on the crane against its own silhouette: spill outside the subject 72% -> 10%, IoU 0.58 -> 0.91.
//!
//! What colour cannot do: tell pale from pale. White feathers on an overcast sky are too close for a projection
//! to mean anything -- it leaves a light rim inside the head, or punches holes in cream back feathers. So the
//! colour matte keeps SAM's answer wherever |F - B| is small, and deep inside the subject only ever opens a hole
//! where the colours are strongly different.
//!
//! 3. **Learned matte (optional).** When the user has installed one of the ViTMatte models (Settings -> AI
//!    Models), it decides the uncertain band instead: a network trained on hair, fur and feathers, given a
//!    trimap built from steps 1-2. On the stork it removes both colour failures; on the crane it keeps the
//!    lattice open (spill 9.7%) and tightens the worst reach from 19 to 15 px. Without it, step 2 stands alone.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use ndarray::Array4;
use ort::value::Tensor;
use image::{DynamicImage, GenericImageView, GrayImage, Luma};
use ort::session::Session;

use crate::ai_processing::{
    ImageEmbeddings, MATTE_FINE_FILENAME, MATTE_FINEST_FILENAME, edt_2d, generate_image_embeddings, sam_binary_mask,
};

// ponytail: pixel constants tuned on 3000-4000 px photos (phone and APS-C). Scale them by long_edge / 4000 if
// 8000+ px originals start showing a too-narrow edge band.
const CROP_PAD: f64 = 0.12; // of the subject's long side, around the crop
const CROP_PAD_MIN: f64 = 48.0;
const CROP_WORTH_IT: f64 = 0.8; // re-run only if the crop is at most this fraction of the photo's long side
const NEAR: f32 = 8.0; // the matte may only act this close to SAM's mask
const SURE_BG: f32 = 14.0; // background samples come from at least this far outside it
const FG_ERODE: f32 = 3.0;
const FAR_FROM_BG: f32 = 0.12; // |I - B| a sure-foreground sample must clear (0..1 RGB)
const BG_SIGMA: f32 = 50.0;
const FG_SIGMA: f32 = 70.0;
const MIN_CONF: f32 = 0.10; // |F - B| below this: the photograph cannot answer, keep SAM's
const EDGE_BAND: f32 = 12.0; // ~ one crop-pass grid cell either side of SAM's edge
const HOLE_CONF: f32 = 0.25; // inside the subject, only colours this different may open a hole
const HOLE_ALPHA: f32 = 0.35;
const UNSURE_BAND: f32 = 14.0; // the learned matte's unknown region, either side of the crop pass's edge
const UNSURE_INSIDE: u8 = 230; // ...plus anywhere inside the colour matte already doubted (lattice holes)
const TILE: usize = 512; // ViTMatte's global attention over a whole crop would need gigabytes: tile the band
const STEP: usize = 384;

static MATTE: Mutex<Option<(PathBuf, Arc<Mutex<Session>>)>> = Mutex::new(None);

/// The larger matting model the user has installed, loaded once. Never downloads: that is the user's call, in
/// Settings -> AI Models. A model that fails to load is logged and treated as absent.
pub fn installed_matte(models_dir: &Path) -> Option<Arc<Mutex<Session>>> {
    let path = [MATTE_FINEST_FILENAME, MATTE_FINE_FILENAME]
        .iter()
        .map(|f| models_dir.join(f))
        .find(|p| p.exists())?;
    let mut slot = MATTE.lock().unwrap();
    if let Some((p, s)) = slot.as_ref()
        && *p == path
    {
        return Some(s.clone());
    }
    match Session::builder().and_then(|b| b.commit_from_file(&path)) {
        Ok(session) => {
            let s = Arc::new(Mutex::new(session));
            *slot = Some((path, s.clone()));
            Some(s)
        }
        Err(e) => {
            log::warn!("Edge matte model at {} would not load: {e}", path.display());
            None
        }
    }
}

pub fn unload_matte() {
    *MATTE.lock().unwrap() = None;
}

/// SAM on the whole photo, again on a crop around what it found, then matted to the photograph's edge.
/// `start`/`end`/`stroke` are in the embedded image's coordinates, as for `run_sam_decoder`.
pub fn crisp_subject_mask(
    encoder: &Mutex<Session>,
    decoder: &Mutex<Session>,
    embeddings: &ImageEmbeddings,
    image: &DynamicImage,
    start: (f64, f64),
    end: (f64, f64),
    stroke: Option<&[(f64, f64, f32)]>,
    matte: Option<&Mutex<Session>>,
) -> Result<GrayImage> {
    let coarse = sam_binary_mask(decoder, embeddings, start, end, stroke)?;
    let (w, h) = image.dimensions();
    let coarse = if coarse.dimensions() == (w, h) {
        coarse
    } else {
        image::imageops::resize(&coarse, w, h, image::imageops::FilterType::Nearest)
    };

    let is_box = (start.0 - end.0).abs() > 1e-6 || (start.1 - end.1).abs() > 1e-6;
    let mut region = bbox(&coarse);
    if is_box {
        let b = (start.0.min(end.0), start.1.min(end.1), start.0.max(end.0), start.1.max(end.1));
        region = Some(match region {
            Some(r) => (r.0.min(b.0), r.1.min(b.1), r.2.max(b.2), r.3.max(b.3)),
            None => b,
        });
    }
    let Some(region) = region else { return Ok(coarse) };

    let side = (region.2 - region.0).max(region.3 - region.1);
    let pad = (side * CROP_PAD).max(CROP_PAD_MIN);
    let cx0 = (region.0 - pad).max(0.0).floor() as u32;
    let cy0 = (region.1 - pad).max(0.0).floor() as u32;
    let cx1 = ((region.2 + pad).ceil() as u32).min(w);
    let cy1 = ((region.3 + pad).ceil() as u32).min(h);

    let mask = if (cx1 - cx0).max(cy1 - cy0) as f64 <= CROP_WORTH_IT * w.max(h) as f64 {
        let crop = image.crop_imm(cx0, cy0, cx1 - cx0, cy1 - cy0);
        let crop_emb = generate_image_embeddings(&crop, encoder)?;
        let shift = |p: (f64, f64)| (p.0 - cx0 as f64, p.1 - cy0 as f64);
        let stroke: Option<Vec<_>> =
            stroke.map(|s| s.iter().map(|&(x, y, l)| (x - cx0 as f64, y - cy0 as f64, l)).collect());
        let fine = sam_binary_mask(decoder, &crop_emb, shift(start), shift(end), stroke.as_deref())?;
        if bbox(&fine).is_none() {
            coarse // the crop lost the subject: trust the first pass rather than return nothing
        } else {
            let mut full = GrayImage::new(w, h);
            image::imageops::replace(&mut full, &fine, cx0 as i64, cy0 as i64);
            full
        }
    } else {
        coarse
    };

    let rgb = image.to_rgb8();
    let colour = colour_matte(&rgb, &mask);
    Ok(match matte {
        Some(session) => learned_matte(session, &rgb, &mask, &colour).unwrap_or_else(|e| {
            log::warn!("Edge matte failed, keeping the colour matte: {e}");
            colour
        }),
        None => colour,
    })
}

/// ViTMatte on the band where the crop pass and the colour matte are unsure; their answer everywhere else.
fn learned_matte(
    session: &Mutex<Session>,
    rgb: &image::RgbImage,
    binary: &GrayImage,
    colour: &GrayImage,
) -> Result<GrayImage> {
    let (w, h) = binary.dimensions();
    let Some(b) = bbox(binary) else { return Ok(colour.clone()) };
    let p = UNSURE_BAND as f64 + 64.0;
    let x0 = (b.0 - p).max(0.0) as usize;
    let y0 = (b.1 - p).max(0.0) as usize;
    let x1 = ((b.2 + p) as usize).min(w as usize);
    let y1 = ((b.3 + p) as usize).min(h as usize);
    let (rw, rh) = (x1 - x0, y1 - y0);
    let at = |i: usize| ((x0 + i % rw) as u32, (y0 + i / rw) as u32);

    let m: Vec<bool> = (0..rw * rh).map(|i| binary.get_pixel(at(i).0, at(i).1)[0] > 127).collect();
    let not_m: Vec<bool> = m.iter().map(|v| !v).collect();
    let (d_in, d_out) = (edt_2d(&m, rw, rh), edt_2d(&not_m, rw, rh));
    let unknown: Vec<bool> = (0..rw * rh)
        .map(|i| {
            let c = colour.get_pixel(at(i).0, at(i).1)[0];
            (m[i] && (d_in[i] <= UNSURE_BAND || c < UNSURE_INSIDE)) || (!m[i] && d_out[i] <= UNSURE_BAND)
        })
        .collect();

    let mut acc = vec![0f32; rw * rh];
    let mut wsum = vec![0f32; rw * rh];
    let hann: Vec<f32> = (0..TILE)
        .map(|i| (std::f32::consts::PI * (i as f32 + 1.0) / (TILE as f32 + 1.0)).sin().powi(2))
        .collect();
    let starts = |n: usize| -> Vec<usize> {
        let last = n.saturating_sub(TILE);
        let mut v: Vec<usize> = (0..=last).step_by(STEP).collect();
        if *v.last().unwrap() != last {
            v.push(last);
        }
        v
    };
    for &ty in &starts(rh) {
        for &tx in &starts(rw) {
            let (th, tw) = (TILE.min(rh - ty), TILE.min(rw - tx));
            if !(0..th).any(|y| (0..tw).any(|x| unknown[(ty + y) * rw + tx + x])) {
                continue;
            }
            // ViTMatte's processor: RGB to [-1, 1], the trimap in [0, 1] as a fourth channel, padded to /32.
            let (ph, pw) = (th.div_ceil(32) * 32, tw.div_ceil(32) * 32);
            let mut input = Array4::<f32>::zeros((1, 4, ph, pw));
            for y in 0..th {
                for x in 0..tw {
                    let i = (ty + y) * rw + tx + x;
                    let px = rgb.get_pixel((x0 + tx + x) as u32, (y0 + ty + y) as u32);
                    for c in 0..3 {
                        input[[0, c, y, x]] = px[c] as f32 / 127.5 - 1.0;
                    }
                    input[[0, 3, y, x]] = if unknown[i] { 0.5 } else { m[i] as u8 as f32 };
                }
            }
            let alpha = {
                let mut s = session.lock().unwrap();
                let out = s.run(ort::inputs![Tensor::from_array(input)?])?;
                out[0].try_extract_array::<f32>()?.to_owned()
            };
            for y in 0..th {
                for x in 0..tw {
                    let i = (ty + y) * rw + tx + x;
                    let wv = hann[y] * hann[x] + 1e-3;
                    acc[i] += alpha[[0, 0, y, x]] * wv;
                    wsum[i] += wv;
                }
            }
        }
    }

    let mut out = colour.clone();
    for i in 0..rw * rh {
        let a = if unknown[i] { acc[i] / wsum[i].max(1e-6) } else { m[i] as u8 as f32 };
        let (x, y) = at(i);
        out.put_pixel(x, y, Luma([(a.clamp(0.0, 1.0) * 255.0).round() as u8]));
    }
    Ok(out)
}

fn bbox(m: &GrayImage) -> Option<(f64, f64, f64, f64)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in m.enumerate_pixels() {
        if p[0] > 127 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (x0 <= x1).then(|| (x0 as f64, y0 as f64, x1 as f64 + 1.0, y1 as f64 + 1.0))
}

/// The matte, worked only on the window round the mask: everything else is background by definition.
pub fn colour_matte(rgb: &image::RgbImage, mask: &GrayImage) -> GrayImage {
    let (w, h) = mask.dimensions();
    let mut out = GrayImage::new(w, h);
    let Some(b) = bbox(mask) else { return out };
    let p = (FG_SIGMA * 2.3) as f64;
    let x0 = (b.0 - p).max(0.0) as u32;
    let y0 = (b.1 - p).max(0.0) as u32;
    let x1 = ((b.2 + p) as u32).min(w);
    let y1 = ((b.3 + p) as u32).min(h);
    let (rw, rh) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let n = rw * rh;

    let mut img = vec![[0f32; 3]; n];
    let mut m = vec![false; n];
    for y in 0..rh {
        for x in 0..rw {
            let px = rgb.get_pixel(x0 + x as u32, y0 + y as u32);
            img[y * rw + x] = [px[0] as f32 / 255.0, px[1] as f32 / 255.0, px[2] as f32 / 255.0];
            m[y * rw + x] = mask.get_pixel(x0 + x as u32, y0 + y as u32)[0] > 127;
        }
    }

    let alpha = matte_region(&img, &m, rw, rh);
    for y in 0..rh {
        for x in 0..rw {
            let a = (alpha[y * rw + x].clamp(0.0, 1.0) * 255.0).round() as u8;
            out.put_pixel(x0 + x as u32, y0 + y as u32, Luma([a]));
        }
    }
    out
}

fn matte_region(img: &[[f32; 3]], m: &[bool], w: usize, h: usize) -> Vec<f32> {
    let not_m: Vec<bool> = m.iter().map(|v| !v).collect();
    let d_in = edt_2d(m, w, h); // inside the mask: distance to its edge
    let d_out = edt_2d(&not_m, w, h); // outside it: distance to the mask

    let bg_w: Vec<f32> = d_out.iter().map(|&d| if d > SURE_BG { 1.0 } else { 0.0 }).collect();
    let (bk, _) = local_mean(img, &bg_w, w, h, BG_SIGMA);
    // Sure foreground is inside the mask AND clearly unlike the background beside it. Without the second test,
    // the sky trapped in a lattice panel counts as "subject" and drags F toward sky exactly where it matters.
    let fg_w: Vec<f32> = (0..w * h)
        .map(|i| (d_in[i] > FG_ERODE && dist(img[i], bk[i]) > FAR_FROM_BG) as u8 as f32)
        .collect();
    let (fk, fden) = local_mean(img, &fg_w, w, h, FG_SIGMA);

    (0..w * h)
        .map(|i| {
            if d_out[i] > NEAR {
                return 0.0;
            }
            let sam = m[i] as u8 as f32;
            let d = sub(fk[i], bk[i]);
            let conf = norm(d);
            if conf <= MIN_CONF || fden[i] <= 0.02 {
                return sam;
            }
            let alpha = (dot(sub(img[i], bk[i]), d) / (conf * conf)).clamp(0.0, 1.0);
            // The edge band is where SAM's answer is uncertain by construction: the matte decides there. Deep
            // inside, SAM is right and the photograph is the unreliable one -- pale feathers on a pale sky
            // project halfway to "sky" and an edit goes on in streaks. So inside, the matte may only open a
            // HOLE, only where it is nearly sure, and only where subject and background are strongly unlike.
            let deep = m[i] && d_in[i] > EDGE_BAND;
            if deep && !(conf > HOLE_CONF && alpha < HOLE_ALPHA) {
                1.0
            } else {
                alpha
            }
        })
        .collect()
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}
fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    norm(sub(a, b))
}

/// Normalised convolution: the mean colour of the pixels where `wt` = 1, smoothly filled in everywhere else,
/// plus the weight density (how much real data each estimate rests on). Worked at quarter size: these are
/// broad, slow estimates and do not need full resolution.
fn local_mean(img: &[[f32; 3]], wt: &[f32], w: usize, h: usize, sigma: f32) -> (Vec<[f32; 3]>, Vec<f32>) {
    const S: usize = 4;
    let (sw, sh) = (w.div_ceil(S), h.div_ceil(S));
    let mut num = vec![[0f32; 3]; sw * sh];
    let mut den = vec![0f32; sw * sh];
    let mut cnt = vec![0f32; sw * sh];
    for y in 0..h {
        for x in 0..w {
            let (i, j) = (y * w + x, (y / S) * sw + x / S);
            for c in 0..3 {
                num[j][c] += img[i][c] * wt[i];
            }
            den[j] += wt[i];
            cnt[j] += 1.0;
        }
    }
    for j in 0..sw * sh {
        for c in 0..3 {
            num[j][c] /= cnt[j];
        }
        den[j] /= cnt[j];
    }
    let s = sigma / S as f32;
    let mut planes: Vec<Vec<f32>> = (0..3).map(|c| num.iter().map(|v| v[c]).collect()).collect();
    for p in planes.iter_mut() {
        gauss(p, sw, sh, s);
    }
    gauss(&mut den, sw, sh, s);

    let mut mean = vec![[0f32; 3]; w * h];
    let mut dens = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            // bilinear, sampling the small grid at pixel centres
            let fx = ((x as f32 + 0.5) / S as f32 - 0.5).clamp(0.0, (sw - 1) as f32);
            let fy = ((y as f32 + 0.5) / S as f32 - 0.5).clamp(0.0, (sh - 1) as f32);
            let (ax, ay) = (fx.floor() as usize, fy.floor() as usize);
            let (bx, by) = ((ax + 1).min(sw - 1), (ay + 1).min(sh - 1));
            let (tx, ty) = (fx - ax as f32, fy - ay as f32);
            let at = |v: &[f32]| {
                let top = v[ay * sw + ax] * (1.0 - tx) + v[ay * sw + bx] * tx;
                let bot = v[by * sw + ax] * (1.0 - tx) + v[by * sw + bx] * tx;
                top * (1.0 - ty) + bot * ty
            };
            let d = at(&den);
            let i = y * w + x;
            for c in 0..3 {
                mean[i][c] = at(&planes[c]) / d.max(1e-4);
            }
            dens[i] = d;
        }
    }
    (mean, dens)
}

/// Separable Gaussian, edges clamped.
fn gauss(p: &mut [f32], w: usize, h: usize, sigma: f32) {
    let r = (sigma * 3.0).ceil() as i64;
    let k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let ks: f32 = k.iter().sum();
    let mut tmp = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut a = 0.0;
            for (t, kv) in k.iter().enumerate() {
                let xx = (x as i64 + t as i64 - r).clamp(0, w as i64 - 1) as usize;
                a += p[y * w + xx] * kv;
            }
            tmp[y * w + x] = a / ks;
        }
    }
    for y in 0..h {
        for x in 0..w {
            let mut a = 0.0;
            for (t, kv) in k.iter().enumerate() {
                let yy = (y as i64 + t as i64 - r).clamp(0, h as i64 - 1) as usize;
                a += tmp[yy * w + x] * kv;
            }
            p[y * w + x] = a / ks;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn canvas(w: u32, h: u32, sky: [u8; 3]) -> RgbImage {
        RgbImage::from_pixel(w, h, Rgb(sky))
    }

    /// A latticed panel: dark bars round a square of sky, and the SAM-style mask that fills the whole square.
    /// The matte has to open the sky inside the frame and keep the steel -- the crane's jib triangle.
    #[test]
    fn opens_sky_seen_through_a_lattice() {
        let mut img = canvas(400, 400, [200, 190, 215]);
        let mut mask = GrayImage::new(400, 400);
        for y in 100..300 {
            for x in 100..300 {
                mask.put_pixel(x, y, Luma([255]));
                let bar = x < 106 || x >= 294 || y < 106 || y >= 294 || (x / 30) % 2 == 0 && x % 30 < 4;
                if bar {
                    img.put_pixel(x, y, Rgb([30, 28, 35]));
                }
            }
        }
        let a = colour_matte(&img, &mask);
        assert!(a.get_pixel(200, 200)[0] < 60, "sky inside the lattice stayed masked: {}", a.get_pixel(200, 200)[0]);
        assert!(a.get_pixel(102, 200)[0] > 200, "the steel bar lost its mask: {}", a.get_pixel(102, 200)[0]);
    }

    /// Cream feathers on a pale sky, with texture: too close to separate. The interior must stay wholly
    /// masked -- a half-applied edit inside a bird is worse than any halo.
    #[test]
    fn never_streaks_a_pale_subject_on_a_pale_sky() {
        let mut img = canvas(400, 400, [222, 220, 216]);
        let mut mask = GrayImage::new(400, 400);
        for y in 0..400u32 {
            for x in 0..400u32 {
                let (dx, dy) = (x as f32 - 200.0, y as f32 - 200.0);
                if dx * dx + dy * dy < 120.0 * 120.0 {
                    mask.put_pixel(x, y, Luma([255]));
                    let t = ((x * 7 + y * 13) % 17) as u8; // feather texture
                    img.put_pixel(x, y, Rgb([228 - t, 214 - t, 190 - t]));
                }
            }
        }
        let a = colour_matte(&img, &mask);
        for (x, y) in [(200, 200), (150, 210), (260, 180), (200, 110)] {
            assert_eq!(a.get_pixel(x, y)[0], 255, "pale interior at ({x},{y}) was weakened");
        }
    }

    /// SAM's edge one grid cell too far out, round a dark subject on a bright sky: the matte pulls it in
    /// to the photograph's edge -- the halo the old blur produced, gone.
    #[test]
    fn pulls_an_overreaching_edge_onto_the_subject() {
        let mut img = canvas(400, 400, [190, 200, 225]);
        let mut mask = GrayImage::new(400, 400);
        for y in 0..400u32 {
            for x in 0..400u32 {
                let (dx, dy) = (x as f32 - 200.0, y as f32 - 200.0);
                let r = (dx * dx + dy * dy).sqrt();
                if r < 100.0 {
                    img.put_pixel(x, y, Rgb([40, 35, 30]));
                }
                if r < 106.0 {
                    mask.put_pixel(x, y, Luma([255]));
                }
            }
        }
        let a = colour_matte(&img, &mask);
        assert!(a.get_pixel(303, 200)[0] < 30, "sky just past the subject stayed masked: {}", a.get_pixel(303, 200)[0]);
        assert!(a.get_pixel(296, 200)[0] > 225, "the subject's own edge was lost: {}", a.get_pixel(296, 200)[0]);
    }
}
