//! Edits made in the iPhone's Photos app.
//!
//! Photos never touches the original: an edit is a recipe in an `.AAE` sidecar (an XML plist whose
//! `adjustmentData` is a compressed plist or JSON), and on export or a USB import the phone adds its own render
//! of the result as `IMG_E1234.HEIC/JPG` next to `IMG_1234.DNG`. Lightroom ignores both. Here they show up as an
//! "iPhone edit" in the photo's versions: Apple's render to look at exactly, and the recipe translated into our
//! sliders as an editable starting point.
//!
//! ponytail: Apple doesn't document the recipe. The identifiers and setting names below are the ones Photos is
//! known to write (SmartTone, SmartColor, ...); the scales in `SCALE` are first guesses, to be calibrated against
//! real AAE + render pairs. Unknown adjustments are logged and skipped, never guessed at.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

pub struct AppleEdit {
    /// Apple's own render of the edit (IMG_E1234.*), when the phone exported one
    pub render: Option<PathBuf>,
    /// The recipe, translated to our adjustments; None without an AAE or when nothing in it maps
    pub adjustments: Option<Value>,
    /// Unix seconds of the newest of the two files
    pub modified: i64,
}

/// `IMG_1234` -> ("IMG_", "1234"): the naming Photos uses for everything it exports
fn split_iphone_stem(stem: &str) -> Option<(&str, &str)> {
    let i = stem.find('_')?;
    let (prefix, digits) = (&stem[..=i], &stem[i + 1..]);
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())).then_some((prefix, digits))
}

const RENDER_EXTS: [&str; 5] = ["heic", "jpg", "jpeg", "png", "tif"];

/// The iPhone's edit of this photo, from files beside it.
pub fn find(source: &Path) -> Option<AppleEdit> {
    let stem = source.file_stem()?.to_str()?;
    let (prefix, digits) = split_iphone_stem(stem)?;
    let dir = source.parent()?;
    let aae_names = [format!("{stem}.aae"), format!("{prefix}O{digits}.aae"), format!("{prefix}E{digits}.aae")]
        .map(|n| n.to_ascii_lowercase());
    let render_stem = format!("{prefix}E{digits}").to_ascii_lowercase();
    let (mut aae, mut render) = (None, None);
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if aae.is_none() && aae_names.contains(&name) {
            aae = Some(entry.path());
        } else if render.is_none()
            && let Some((s, ext)) = name.rsplit_once('.')
            && s == render_stem
            && RENDER_EXTS.contains(&ext)
        {
            render = Some(entry.path());
        }
    }
    if aae.is_none() && render.is_none() {
        return None;
    }
    let modified = [&aae, &render]
        .into_iter()
        .flatten()
        .filter_map(|p| p.metadata().ok()?.modified().ok())
        .filter_map(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .max()
        .unwrap_or(0);
    let adjustments = aae.as_ref().and_then(|p| {
        match std::fs::read(p).map_err(|e| e.to_string()).and_then(|b| decode_aae(&b)) {
            Ok(recipe) => to_adjustments(&recipe, crate::apple_matte::proraw_base_exposure(source).unwrap_or(0.0)),
            Err(e) => {
                log::warn!("iPhone edit {} not readable: {e}", p.display());
                None
            }
        }
    });
    Some(AppleEdit { render, adjustments, modified })
}

fn plist_to_json(v: plist::Value) -> Value {
    match v {
        plist::Value::Dictionary(d) => Value::Object(d.into_iter().map(|(k, v)| (k, plist_to_json(v))).collect()),
        plist::Value::Array(a) => Value::Array(a.into_iter().map(plist_to_json).collect()),
        plist::Value::Boolean(b) => json!(b),
        plist::Value::Integer(i) => i
            .as_signed()
            .map(|i| json!(i))
            .or_else(|| i.as_unsigned().map(|u| json!(u)))
            .unwrap_or(Value::Null),
        plist::Value::Real(f) => json!(f),
        plist::Value::String(s) => json!(s),
        _ => Value::Null, // data blobs and dates carry nothing we map
    }
}

/// A plist (binary or XML) or JSON document, or None
fn parse_doc(b: &[u8]) -> Option<Value> {
    if let Ok(v) = plist::Value::from_reader(std::io::Cursor::new(b)) {
        return Some(plist_to_json(v));
    }
    serde_json::from_slice(b).ok()
}

/// The AAE's recipe: its `adjustmentData`, decompressed (raw deflate or zlib) and parsed. Same cascade as osxphotos.
pub fn decode_aae(aae: &[u8]) -> Result<Value, String> {
    let outer = plist::Value::from_reader(std::io::Cursor::new(aae)).map_err(|e| format!("not a plist: {e}"))?;
    let dict = outer.as_dictionary().ok_or("AAE is not a dictionary")?;
    if let Some(f) = dict.get("adjustmentFormatIdentifier").and_then(|v| v.as_string())
        && f != "com.apple.photo"
    {
        return Err(format!("edit made by {f}, not Photos"));
    }
    let data = dict.get("adjustmentData").and_then(|v| v.as_data()).ok_or("no adjustmentData")?;
    if let Some(v) = parse_doc(data) {
        return Ok(v);
    }
    let mut out = vec![];
    if flate2::read::DeflateDecoder::new(data).read_to_end(&mut out).is_ok()
        && let Some(v) = parse_doc(&out)
    {
        return Ok(v);
    }
    out.clear();
    if flate2::read::ZlibDecoder::new(data).read_to_end(&mut out).is_ok()
        && let Some(v) = parse_doc(&out)
    {
        return Ok(v);
    }
    Err("adjustmentData in an unknown encoding".into())
}

/// (Apple identifiers, setting names, our key, scale). Apple's offsets run -1..1; ours -100..100 (EV for exposure).
/// Names are the ones Photos writes in iOS 26/27 (DG* / RK* operations) plus their older names. Scales and filters
/// were fitted on 19 ProRAW photos edited on the owner's iPhone, against Apple's own render of each edit (lightness,
/// percentiles, colour, local contrast, vignetting): on photos left out of the fit the result is closer to Apple's
/// than leaving the edit off. Sharpening and noise reduction can't be seen at that size and stay 1:1.
const SCALE: &[(&[&str], &[&str], &str, f64)] = &[
    (&["SmartTone"], &["offsetExposure"], "exposure", 1.67),
    (&["SmartTone"], &["inputLight"], "exposure", 1.0),
    (&["SmartTone"], &["offsetBrightness"], "brightness", 3.0),
    (&["SmartTone"], &["offsetContrast"], "contrast", 85.0),
    (&["SmartTone"], &["offsetHighlights"], "highlights", 90.0),
    (&["SmartTone"], &["offsetShadows"], "shadows", 86.0),
    (&["SmartTone"], &["offsetBlackPoint", "offsetBlack"], "blacks", -99.0), // Apple's black point darkens as it rises
    (&["SmartColor"], &["offsetSaturation"], "saturation", 80.0),
    (&["SmartColor"], &["offsetContrast"], "vibrance", 11.0), // the Vibrance slider
    (&["SmartColor"], &["offsetCast"], "temperature", 100.0),
    (&["WhiteBalance"], &["warmTemp"], "temperature", 94.0),
    (&["WhiteBalance"], &["warmTint"], "tint", 98.0),
    (&["DGDefinition2Operation", "Definition"], &["inputIntensity", "inputDefinition"], "clarity", 104.0),
    (&["RKProSharpenOperation", "Sharpen"], &["inputSharpness"], "sharpness", 100.0),
    (&["DGRAWReduceNoiseOperation"], &["inputLNRAmount"], "lumaNoiseReduction", 100.0),
    (&["DGRAWReduceNoiseOperation"], &["inputCNRAmount"], "colorNoiseReduction", 100.0),
    (&["RKNoiseReductionOperation", "NoiseReduction"], &["inputRadius"], "lumaNoiseReduction", 100.0),
    (&["DGVignetteEffectOperation", "Vignette"], &["inputIntensity"], "vignetteAmount", -100.0), // ours darkens below 0
];

/// Brilliance (SmartTone offsetLocalLight) has no single twin: per unit it moves these sliders.
const BRILLIANCE: &[(&str, f64)] = &[("shadows", 40.0), ("highlights", -39.0)];

/// Photos' filters as slider moves at full intensity. Mono and Silverplate are black and white.
const FILTERS: &[(&str, &[(&str, f64)])] = &[
    ("3DVivid", &[("contrast", 13.0), ("saturation", 6.0), ("shadows", -11.0), ("highlights", 5.0), ("clarity", 7.0), ("temperature", -5.0), ("tint", -10.0)]),
    ("3DVividWarm", &[("contrast", 14.0), ("temperature", 29.0), ("tint", 23.0)]),
    ("3DVividCool", &[("contrast", 13.0), ("saturation", 6.0), ("temperature", -25.0)]), // not in the fit: Vivid, cooled
    ("3DDramatic", &[("contrast", 21.0), ("clarity", 14.0), ("saturation", -22.0), ("vibrance", -11.0), ("highlights", 8.0), ("tint", -9.0)]),
    ("3DDramaticWarm", &[("contrast", 14.0), ("clarity", 16.0), ("saturation", -25.0), ("vibrance", -17.0), ("highlights", 11.0), ("temperature", 17.0)]),
    ("3DDramaticCool", &[("contrast", 21.0), ("clarity", 14.0), ("saturation", -22.0), ("temperature", -17.0)]), // not in the fit
    ("3DMono", &[("saturation", -100.0)]),
    ("3DSilverplate", &[("saturation", -98.0), ("contrast", 21.0), ("temperature", 6.0)]),
    ("3DNoir", &[("saturation", -100.0), ("contrast", 35.0)]), // not in the fit
];

/// Recorded by Photos but nothing sliders can redo: logged, never guessed at.
const NOT_SLIDERS: &[&str] = &[
    "RKRawDecodeOperation", "SemanticStyle", "Inpaint", "InpaintMasks", "GenerativeEdits", "GeneratedTiles",
    "DepthEffect", "HighResolutionFusion", "AutoLoop", "SlowMotion", "Trim", "CinematicAudio", "PortraitVideo",
];

/// A setting of one adjustment: Photos keeps newer operations' values one level down, under `inputKeys`.
fn setting(settings: &Value, key: &str) -> Option<f64> {
    settings.get(key).or_else(|| settings.get("inputKeys")?.get(key))?.as_f64().filter(|v| v.abs() <= 1.0 && *v != 0.0)
}

/// Photos' crop, in pixels of the upright photo -- the frame our crop is measured in. Photos stores it in sensor
/// pixels; `metadata` gives the upright size and the orientation. None when the crop also straightens or corrects
/// perspective (ours would need a matching rotation), or falls outside the frame.
fn crop(settings: &Value, metadata: Option<&Value>) -> Option<Value> {
    let f = |k: &str| settings.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0);
    if ["straightenAngle", "pitch", "yaw"].iter().any(|k| f(k).abs() > 1e-4) {
        log::info!("iPhone crop straightens or corrects perspective; not translated");
        return None;
    }
    let m = metadata?;
    let (w_up, h_up) = (m.get("masterWidth")?.as_f64()?, m.get("masterHeight")?.as_f64()?);
    let o = m.get("orientation").and_then(|v| v.as_u64()).unwrap_or(1);
    let (sw, sh) = if matches!(o, 5..=8) { (h_up, w_up) } else { (w_up, h_up) }; // sensor buffer
    let (x, y, w, h) = (f("xOrigin"), f("yOrigin"), f("width"), f("height"));
    let (ux, uy, uw, uh) = match o {
        3 => (sw - x - w, sh - y - h, w, h),
        6 => (sh - y - h, x, h, w),
        8 => (y, sw - x - w, h, w),
        _ => (x, y, w, h),
    };
    if w <= 0.0 || h <= 0.0 || ux < 0.0 || uy < 0.0 || ux + uw > w_up + 0.5 || uy + uh > h_up + 0.5 {
        return None;
    }
    if ux == 0.0 && uy == 0.0 && (uw - w_up).abs() < 1.0 && (uh - h_up).abs() < 1.0 {
        return None; // the whole frame
    }
    Some(json!({"unit": "px", "x": ux, "y": uy, "width": uw, "height": uh}))
}

/// The recipe as our adjustments, an editable starting point that looks like Apple's render, not a copy of it.
/// `base_exposure` is where our render of this photo matches Apple's unedited one (see
/// `apple_matte::proraw_base_exposure`); Apple's exposure moves are relative to that.
pub fn to_adjustments(recipe: &Value, base_exposure: f64) -> Option<Value> {
    let list = recipe.get("adjustments")?.as_array()?;
    let mut out = Map::new();
    let add = |out: &mut Map<String, Value>, key: &str, v: f64| {
        let lim = if key == "exposure" { 5.0 } else { 100.0 };
        let now = out.get(key).and_then(|x| x.as_f64()).unwrap_or(0.0);
        out.insert(key.to_string(), json!(((now + v).clamp(-lim, lim) * 100.0).round() / 100.0));
    };
    for adj in list {
        if adj.get("enabled").and_then(|e| e.as_bool()) == Some(false) {
            continue;
        }
        let Some(id) = adj.get("identifier").and_then(|i| i.as_str()) else { continue };
        let settings = adj.get("settings").cloned().unwrap_or(Value::Null);
        let mut known = NOT_SLIDERS.contains(&id);
        for (ids, names, key, scale) in SCALE {
            if ids.contains(&id) {
                known = true;
                if let Some(v) = names.iter().find_map(|n| setting(&settings, n)) {
                    add(&mut out, key, v * scale);
                }
            }
        }
        match id {
            "SmartTone" => {
                if let Some(v) = setting(&settings, "offsetLocalLight") {
                    for (key, per) in BRILLIANCE {
                        add(&mut out, key, v * per);
                    }
                }
            }
            "Effect" | "Filter" => {
                known = true;
                let name = settings.get("effectName").or_else(|| settings.get("filterName")).and_then(|n| n.as_str()).unwrap_or("");
                let amount = settings.get("effectIntensity").and_then(|v| v.as_f64()).unwrap_or(1.0).clamp(0.0, 1.0);
                match FILTERS.iter().find(|(n, _)| *n == name) {
                    Some((_, moves)) => {
                        for (key, v) in *moves {
                            add(&mut out, key, v * amount);
                        }
                    }
                    None if !name.is_empty() => log::info!("iPhone filter {name} has no Grain Studio twin; skipped"),
                    None => {}
                }
            }
            "Crop" => {
                known = true;
                if let Some(c) = crop(&settings, recipe.get("metadata")) {
                    out.insert("crop".into(), c);
                }
            }
            "Orientation" => known = true,
            _ => {}
        }
        if !known {
            log::info!("iPhone adjustment {id} not translated");
        }
    }
    if out.is_empty() {
        return None;
    }
    if base_exposure != 0.0 {
        add(&mut out, "exposure", base_exposure);
    }
    Some(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// An AAE as Photos writes one: XML plist, adjustmentData = raw-deflated JSON
    fn aae(recipe: &Value) -> Vec<u8> {
        let mut z = flate2::write::DeflateEncoder::new(vec![], flate2::Compression::default());
        z.write_all(recipe.to_string().as_bytes()).unwrap();
        let mut d = plist::Dictionary::new();
        d.insert("adjustmentData".into(), plist::Value::Data(z.finish().unwrap()));
        d.insert("adjustmentFormatIdentifier".into(), "com.apple.photo".into());
        d.insert("adjustmentFormatVersion".into(), "1.4".into());
        let mut out = vec![];
        plist::Value::Dictionary(d).to_writer_xml(&mut out).unwrap();
        out
    }

    /// IMG_8276's recipe as the phone wrote it (iOS 27, Photos 912), statistics and raw-decode parameters left out
    fn recipe() -> Value {
        json!({"metadata": {"masterHeight": 4032, "masterWidth": 3024, "orientation": 6}, "formatVersion": 1, "adjustments": [
            {"identifier": "Effect", "enabled": true, "settings": {"effectVersion": 1, "effectName": "3DVivid", "effectIntensity": 1}},
            {"identifier": "WhiteBalance", "enabled": true, "settings": {"temperature": 5000, "warmTemp": 0.08518518518518514, "tint": 0, "colorType": 3, "warmTint": 0.11851851851851847}},
            {"identifier": "DGDefinition2Operation", "enabled": true, "settings": {"auto": false, "inputKeys": {"inputIntensity": 0.15462962962962962}}},
            {"identifier": "SmartColor", "enabled": true, "settings": {"offsetSaturation": 0.27222222222222214, "inputColor": 0, "offsetCast": 0, "offsetContrast": 0.21111111111111103}},
            {"identifier": "RKRawDecodeOperation", "enabled": true, "settings": {"boostVersion": "FB3", "inputMethodVersion": "8.dng"}},
            {"identifier": "Crop", "enabled": true, "settings": {"height": 2746, "straightenAngle": 0, "pitch": 0, "width": 3661, "xOrigin": 186, "yOrigin": 139, "yaw": 0}},
            {"identifier": "RKProSharpenOperation", "enabled": true, "settings": {"auto": false, "inputKeys": {"inputFalloff": 1, "inputSharpness": 0.29629629629629634, "inputEdgeScale": 1}}},
            {"identifier": "SmartTone", "enabled": true, "settings": {"offsetShadows": -0.19259259259259265, "inputLight": 0, "offsetHighlights": -0.09074074074074079, "offsetLocalLight": -0.35185185185185186, "offsetExposure": -0.15555555555555556, "offsetBrightness": -0.040740740740740744, "offsetBlackPoint": 0.10000000000000009}},
            {"identifier": "DGVignetteEffectOperation", "enabled": false, "settings": {"inputKeys": {"inputIntensity": 0.5}}}
        ]})
    }

    #[test]
    fn decodes_a_deflated_recipe_and_translates_it() {
        let r = decode_aae(&aae(&recipe())).unwrap();
        let a = to_adjustments(&r, 2.79).unwrap();
        let v = |k: &str| a[k].as_f64().unwrap_or(f64::NAN);
        // values one level down (inputKeys) are read
        assert!((v("clarity") - (0.1546 * 104.0 + 7.0)).abs() < 0.1, "{}", v("clarity"));
        assert!((v("sharpness") - 29.63).abs() < 0.01);
        // Apple's exposure is relative to its own base render
        assert!((v("exposure") - (2.79 - 0.1556 * 1.67)).abs() < 0.02, "{}", v("exposure"));
        assert!(v("blacks") < 0.0, "Apple's black point rising darkens");
        assert!(v("temperature") > 0.0 && v("tint") > 0.0);
        assert!(a.get("vignetteAmount").is_none(), "disabled adjustments are skipped");
        // the crop, turned upright: the frame Apple's render shows (checked against IMG_E8276)
        assert_eq!(a["crop"], json!({"unit": "px", "x": 139.0, "y": 186.0, "width": 2746.0, "height": 3661.0}));
    }

    #[test]
    fn a_straightened_crop_is_not_guessed_at() {
        let mut r = recipe();
        r["adjustments"][5]["settings"]["straightenAngle"] = json!(-0.07);
        assert!(to_adjustments(&r, 0.0).unwrap().get("crop").is_none());
    }

    #[test]
    fn finds_the_aae_and_apples_render_beside_the_raw() {
        let dir = std::env::temp_dir().join(format!("grain-aae-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let raw = dir.join("IMG_0042.DNG");
        std::fs::write(&raw, b"raw").unwrap();
        std::fs::write(dir.join("IMG_0042.AAE"), aae(&recipe())).unwrap();
        std::fs::write(dir.join("IMG_E0042.HEIC"), b"render").unwrap();
        std::fs::write(dir.join("IMG_E0043.HEIC"), b"another photo").unwrap();
        let e = find(&raw).unwrap();
        assert_eq!(e.render.unwrap().file_name().unwrap(), "IMG_E0042.HEIC");
        assert!(e.adjustments.is_some());
        assert!(find(&dir.join("DSC_0042.ARW")).is_none(), "no sidecars, no edit");
        assert!(find(&dir.join("holiday.jpg")).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn edits_from_other_apps_are_not_read_as_photos_recipes() {
        let mut d = plist::Dictionary::new();
        d.insert("adjustmentData".into(), plist::Value::Data(b"{}".to_vec()));
        d.insert("adjustmentFormatIdentifier".into(), "com.example.app".into());
        let mut b = vec![];
        plist::Value::Dictionary(d).to_writer_xml(&mut b).unwrap();
        assert!(decode_aae(&b).is_err());
    }
}
