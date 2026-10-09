//! Masks the iPhone already made.
//!
//! A ProRAW DNG carries Apple's own semantic mattes beside the raw data -- a standard DNG 1.6 sub-image with
//! PhotometricInterpretation 52527 ("Semantic Mask") and a SemanticName such as
//! "urn:com:apple:photo:2020:aux:semanticskymatte". The sky matte is half the photo's resolution (1512 x 2016 on a
//! 12 MP frame), made on the phone at capture, and it traces railings and branches that our 320 px sky model has
//! to guess at. When a DNG has one, a sky mask uses it directly: instant, no model, Apple's own cut.
//!
//! The matte is stored in sensor orientation as one JPEG strip. Reading it only touches the TIFF headers and that
//! strip, never the raw data.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use image::{DynamicImage, GrayImage, imageops::FilterType};
use rawler::decoders::Orientation;

const PHOTOMETRIC_SEMANTIC_MASK: u32 = 52527;
const TAG_SEMANTIC_NAME: u16 = 52526;

pub struct SemanticMatte {
    /// The DNG SemanticName, e.g. "urn:com:apple:photo:2020:aux:semanticskymatte"
    pub name: String,
    /// In the raw's sensor orientation, as stored
    pub image: GrayImage,
}

struct Reader {
    f: File,
    big: bool,
}

impl Reader {
    fn u16(&self, b: &[u8]) -> u16 {
        if self.big { u16::from_be_bytes([b[0], b[1]]) } else { u16::from_le_bytes([b[0], b[1]]) }
    }
    fn u32(&self, b: &[u8]) -> u32 {
        if self.big { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) } else { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) }
    }
    fn read_at(&mut self, off: u64, n: usize) -> std::io::Result<Vec<u8>> {
        self.f.seek(SeekFrom::Start(off))?;
        let mut v = vec![0u8; n];
        self.f.read_exact(&mut v)?;
        Ok(v)
    }
    /// An IFD's entries: tag -> (type, count, the 4 value-or-offset bytes)
    fn ifd(&mut self, off: u32) -> std::io::Result<Vec<(u16, u16, u32, [u8; 4])>> {
        let count_bytes = self.read_at(off as u64, 2)?;
        let n = self.u16(&count_bytes) as usize;
        if n > 4096 {
            return Err(std::io::Error::other("implausible IFD"));
        }
        let raw = self.read_at(off as u64 + 2, n * 12)?;
        Ok((0..n)
            .map(|i| {
                let e = &raw[i * 12..i * 12 + 12];
                (self.u16(&e[0..2]), self.u16(&e[2..4]), self.u32(&e[4..8]), [e[8], e[9], e[10], e[11]])
            })
            .collect())
    }
    /// SHORT or LONG values of an entry, inline or at its offset
    fn uints(&mut self, typ: u16, count: u32, val: [u8; 4]) -> std::io::Result<Vec<u32>> {
        let size = match typ { 3 => 2, 4 | 13 => 4, _ => return Ok(vec![]) };
        let total = size * count as usize;
        let bytes = if total <= 4 { val[..total].to_vec() } else { let off = self.u32(&val) as u64; self.read_at(off, total)? };
        Ok(bytes.chunks(size).map(|c| if size == 2 { self.u16(c) as u32 } else { self.u32(c) }).collect())
    }
    fn ascii(&mut self, count: u32, val: [u8; 4]) -> std::io::Result<String> {
        let bytes = if count <= 4 { val[..count as usize].to_vec() } else { let off = self.u32(&val) as u64; self.read_at(off, count as usize)? };
        Ok(String::from_utf8_lossy(&bytes).trim_end_matches('\0').to_string())
    }
}

fn is_dng(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("dng"))
}

/// Every semantic matte in a DNG, plus the DNG's orientation. Empty for anything else.
pub fn semantic_mattes(path: &Path) -> (Vec<SemanticMatte>, u16) {
    if !is_dng(path) {
        return (vec![], 1);
    }
    read(path).unwrap_or_else(|e| {
        log::debug!("no semantic mattes read from {}: {e}", path.display());
        (vec![], 1)
    })
}

fn read(path: &Path) -> std::io::Result<(Vec<SemanticMatte>, u16)> {
    let mut f = File::open(path)?;
    let mut head = [0u8; 8];
    f.read_exact(&mut head)?;
    let big = match &head[0..2] {
        b"MM" => true,
        b"II" => false,
        _ => return Err(std::io::Error::other("not a TIFF/DNG")),
    };
    let mut r = Reader { f, big };
    let ifd0 = r.u32(&head[4..8]);
    let entries = r.ifd(ifd0)?;
    let mut orientation = 1u16;
    let mut subs = vec![];
    for (tag, typ, count, val) in &entries {
        match tag {
            274 => orientation = r.uints(*typ, *count, *val)?.first().copied().unwrap_or(1) as u16,
            330 => subs = r.uints(*typ, *count, *val)?,
            _ => {}
        }
    }
    let mut out = vec![];
    for sub in subs {
        let e = r.ifd(sub)?;
        let get = |t: u16| e.iter().find(|x| x.0 == t).copied();
        let photometric = match get(262) { Some((_, ty, c, v)) => r.uints(ty, c, v)?.first().copied(), None => None };
        if photometric != Some(PHOTOMETRIC_SEMANTIC_MASK) {
            continue;
        }
        let name = match get(TAG_SEMANTIC_NAME) { Some((_, _, c, v)) => r.ascii(c, v)?, None => String::new() };
        let (Some(o), Some(n)) = (get(273), get(279)) else { continue };
        let offsets = r.uints(o.1, o.2, o.3)?;
        let counts = r.uints(n.1, n.2, n.3)?;
        // one or more JPEG strips, stacked top to bottom
        let mut strips: Vec<GrayImage> = vec![];
        for (off, len) in offsets.iter().zip(counts.iter()) {
            let bytes = r.read_at(*off as u64, *len as usize)?;
            match image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg) {
                Ok(img) => strips.push(img.to_luma8()),
                Err(e) => {
                    log::debug!("semantic matte {name}: strip not decodable ({e})");
                    strips.clear();
                    break;
                }
            }
        }
        if strips.is_empty() {
            continue;
        }
        let w = strips[0].width();
        let h: u32 = strips.iter().map(|s| s.height()).sum();
        let mut image = GrayImage::new(w, h);
        let mut y = 0;
        for s in strips {
            image::imageops::replace(&mut image, &s, 0, y as i64);
            y += s.height();
        }
        out.push(SemanticMatte { name, image });
    }
    Ok((out, orientation))
}

/// How much to lift an Apple ProRAW so our render starts where Apple's unedited one does, or None for anything else.
///
/// ProRAW keeps scene-linear data with a lot of headroom and says how bright to show it in BaselineExposure
/// (0.6 to 5 EV on the owner's phone). Apple's own render lands near it but not on it, because it tone-maps
/// locally; across 20 ProRAW photos our exposure that matches Apple's unedited preview is 0.555 x BaselineExposure
/// + 1.06 EV (0.5 EV rms off, against 2 EV with no lift and 1.1 EV with the tag applied as is).
/// ponytail: fitted on one iPhone (17 Pro Max); refit with the scratch calibration if another model drifts.
pub fn proraw_base_exposure(path: &Path) -> Option<f64> {
    if !is_dng(path) {
        return None;
    }
    let read = || -> std::io::Result<Option<f64>> {
        let mut f = File::open(path)?;
        let mut head = [0u8; 8];
        f.read_exact(&mut head)?;
        let big = match &head[0..2] {
            b"MM" => true,
            b"II" => false,
            _ => return Ok(None),
        };
        let mut r = Reader { f, big };
        let ifd0 = r.u32(&head[4..8]);
        let (mut apple, mut baseline) = (false, None);
        for (tag, typ, count, val) in r.ifd(ifd0)? {
            match tag {
                271 => apple = r.ascii(count, val)?.starts_with("Apple"),
                50730 if typ == 10 => {
                    let at = r.u32(&val) as u64;
                    let b = r.read_at(at, 8)?;
                    let (n, d) = (r.u32(&b[0..4]) as i32, r.u32(&b[4..8]) as i32);
                    if d != 0 {
                        baseline = Some(n as f64 / d as f64);
                    }
                }
                _ => {}
            }
        }
        Ok(baseline.filter(|_| apple).map(|b| ((0.555 * b + 1.06) * 100.0).round() / 100.0))
    };
    read().ok().flatten()
}

/// Apple's sky matte for this photo, turned the way the decoded raw is turned and sized to it (width x height),
/// or None when the file has no sky matte.
pub fn sky_matte(path: &Path, width: u32, height: u32) -> Option<GrayImage> {
    let (mattes, orientation) = semantic_mattes(path);
    let sky = mattes.into_iter().find(|m| m.name.ends_with("semanticskymatte"))?;
    let turned = crate::image_processing::apply_orientation(
        DynamicImage::ImageLuma8(sky.image),
        Orientation::from_u16(orientation),
    );
    // The matte covers the same frame as the raw at half size; if the aspect disagrees, it is not this photo's.
    let (mw, mh) = (turned.width() as f32, turned.height() as f32);
    if ((mw / mh) - (width as f32 / height as f32)).abs() > 0.02 {
        log::warn!("sky matte in {} is {}x{} for a {width}x{height} photo; not used", path.display(), mw, mh);
        return None;
    }
    Some(turned.resize_exact(width, height, FilterType::Lanczos3).to_luma8())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs a real ProRAW DNG with sky in it (Apple saves no sky matte otherwise): GRAIN_TEST_PRORAW=path cargo test -- --ignored
    #[test]
    #[ignore]
    fn reads_apple_sky_matte_from_a_proraw_dng() {
        let Ok(p) = std::env::var("GRAIN_TEST_PRORAW") else { return };
        let (mattes, orientation) = semantic_mattes(Path::new(&p));
        let sky = mattes.iter().find(|m| m.name.ends_with("semanticskymatte")).expect("no sky matte");
        assert!(sky.image.width() >= 1000, "matte too small: {}x{}", sky.image.width(), sky.image.height());
        assert!(orientation >= 1 && orientation <= 8);
        let m = sky_matte(Path::new(&p), 3024, 4032).or_else(|| sky_matte(Path::new(&p), 4032, 3024)).expect("matte not placed");
        assert!(m.pixels().any(|p| p[0] > 200) && m.pixels().any(|p| p[0] < 50), "matte has no sky/not-sky contrast");
    }

    /// GRAIN_TEST_PRORAW=path cargo test -- --ignored
    #[test]
    #[ignore]
    fn proraw_gets_a_starting_exposure() {
        let Ok(p) = std::env::var("GRAIN_TEST_PRORAW") else { return };
        let e = proraw_base_exposure(Path::new(&p)).expect("no BaselineExposure read");
        assert!((0.0..=5.0).contains(&e), "{e}");
    }

    #[test]
    fn anything_but_a_dng_has_no_mattes() {
        assert!(semantic_mattes(Path::new("photo.jpg")).0.is_empty());
    }
}
