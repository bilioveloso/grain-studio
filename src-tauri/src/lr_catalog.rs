//! Bring a Lightroom Classic catalog's work along: star ratings, picks and rejects, colour labels, keywords,
//! collections, and each photo's develop settings.
//!
//! The approach follows RAWmakase (MIT, github.com/pch/rawmakase): never touch the original. A catalog Lightroom
//! has open (its `.lock` beside it) is refused; otherwise the catalog is copied to a private snapshot, the snapshot
//! must pass SQLite's integrity check, and everything is read from it inside one read transaction. The catalog
//! itself is only ever read by the copy.
//!
//! Nothing a photo already has in Grain Studio is overwritten. Ratings and labels fill only empty ones, and tags
//! are added. Lightroom's develop settings arrive as a version named "Lightroom". They become the current edit only
//! on a photo that has no edit yet.
//!
//! ponytail: virtual copies are counted, not imported. Develop settings go through the .xmp preset converter, so
//! they map exactly as an exported preset would: crop and local masks don't come over. XMP sync is not run here;
//! the next edit of each photo writes it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use regex::Regex;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::Value;

use crate::file_management::{parse_virtual_path, write_atomic};
use crate::tagging::{COLOR_TAG_PREFIX, USER_TAG_PREFIX};
use crate::versions::{PhotoVersion, ensure_original_version, now_seconds, push_version};

const LIGHTROOM_VERSION: &str = "Lightroom";
const COLORS: [&str; 5] = ["red", "yellow", "green", "blue", "purple"];

#[derive(Serialize, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LrImportReport {
    /// Photos found on disk and updated
    pub imported: usize,
    /// Photos the catalog knows whose file is not where it says
    pub missing: usize,
    pub virtual_copies_skipped: usize,
    pub with_develop: usize,
}

struct Row {
    id: i64,
    path: PathBuf,
    rating: u8,
    pick: i8,
    label: Option<String>,
    develop: Option<String>,
}

/// A Lightroom develop table (`s = { Exposure2012 = 0.35, ToneCurvePV2012 = { 0, 0, 255, 255, }, ... }`) written
/// as the XMP attributes the preset converter reads. Only top-level scalars and the tone curves matter there.
fn develop_to_xmp(lua: &str) -> String {
    let scalar = Regex::new(r#"^\s*([A-Za-z0-9]+)\s*=\s*("([^"]*)"|-?[0-9.]+|true|false)\s*,?\s*$"#).unwrap();
    let open = Regex::new(r"^\s*(ToneCurvePV2012(?:Red|Green|Blue)?)\s*=\s*\{").unwrap();
    let number = Regex::new(r"-?[0-9]+(?:\.[0-9]+)?").unwrap();
    let strings = Regex::new(r#""[^"]*""#).unwrap();
    let mut out = String::from("<x:xmpmeta><rdf:Description ");
    let mut curves = String::new();
    let (mut depth, mut curve): (i32, Option<(String, Vec<String>)>) = (0, None);
    for line in lua.lines() {
        let bare = strings.replace_all(line, "");
        if depth == 1 {
            if let Some(c) = scalar.captures(line) {
                let v = c.get(3).map_or(&c[2], |m| m.as_str());
                out.push_str(&format!("crs:{}=\"{}\" ", &c[1], v.replace('"', "")));
            } else if let Some(c) = open.captures(line) {
                curve = Some((c[1].to_string(), vec![]));
            }
        } else if let Some((_, nums)) = curve.as_mut() {
            nums.extend(number.find_iter(&bare).map(|m| m.as_str().to_string()));
        }
        depth += bare.matches('{').count() as i32 - bare.matches('}').count() as i32;
        if depth <= 1
            && let Some((name, nums)) = curve.take()
        {
            let points: String =
                nums.chunks(2).filter(|p| p.len() == 2).map(|p| format!("<rdf:li>{}, {}</rdf:li>", p[0], p[1])).collect();
            curves.push_str(&format!("<crs:{name}><rdf:Seq>{points}</rdf:Seq></crs:{name}>"));
        }
    }
    out.push('>');
    out.push_str(&curves);
    out.push_str("</rdf:Description></x:xmpmeta>");
    out
}

fn develop_adjustments(lua: &str) -> Option<Value> {
    let preset = crate::preset_converter::convert_xmp_to_preset(&develop_to_xmp(lua)).ok()?;
    preset.adjustments.as_object().filter(|m| !m.is_empty())?;
    Some(preset.adjustments)
}

/// A private, checked copy of the catalog, removed when dropped
struct Snapshot(PathBuf, Option<PathBuf>);

impl Drop for Snapshot {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        if let Some(w) = &self.1 {
            let _ = std::fs::remove_file(w);
        }
    }
}

fn with_suffix(p: &Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn snapshot(catalog: &Path) -> Result<Snapshot, String> {
    if with_suffix(catalog, ".lock").exists() {
        return Err("Lightroom has this catalog open. Quit Lightroom and try again.".into());
    }
    let dest = std::env::temp_dir().join(format!("grain-lrcat-{}-{}.lrcat", std::process::id(), now_seconds()));
    std::fs::copy(catalog, &dest).map_err(|e| format!("Could not copy the catalog: {e}"))?;
    // Changes still in the write-ahead log belong to the catalog too.
    let wal = with_suffix(catalog, "-wal");
    let wal_copy = wal.exists().then(|| with_suffix(&dest, "-wal"));
    let snap = Snapshot(dest, wal_copy);
    if let Some(w) = &snap.1 {
        std::fs::copy(&wal, w).map_err(|e| format!("Could not copy the catalog's log: {e}"))?;
    }
    Ok(snap)
}

fn read_catalog(conn: &Connection) -> rusqlite::Result<(Vec<Row>, HashMap<i64, Vec<String>>, usize)> {
    let mut stmt = conn.prepare(
        "SELECT i.id_local, i.rating, i.pick, i.colorLabels, rf.absolutePath, fo.pathFromRoot, f.baseName, f.extension, d.text
         FROM Adobe_images i
         JOIN AgLibraryFile f ON f.id_local = i.rootFile
         JOIN AgLibraryFolder fo ON fo.id_local = f.folder
         JOIN AgLibraryRootFolder rf ON rf.id_local = fo.rootFolder
         LEFT JOIN Adobe_imageDevelopSettings d ON d.image = i.id_local
         WHERE i.masterImage IS NULL",
    )?;
    let rows = stmt
        .query_map([], |r| {
            let (root, folder, base, ext): (String, String, String, String) = (r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?);
            Ok(Row {
                id: r.get(0)?,
                path: PathBuf::from(format!("{root}{folder}{base}.{ext}")),
                rating: r.get::<_, Option<f64>>(1)?.unwrap_or(0.0).clamp(0.0, 5.0) as u8,
                // not signum: it calls 0.0 positive, which made every unflagged photo a pick
                pick: match r.get::<_, Option<f64>>(2)?.unwrap_or(0.0) {
                    p if p > 0.0 => 1,
                    p if p < 0.0 => -1,
                    _ => 0,
                },
                label: r.get::<_, Option<String>>(3)?.map(|s| s.to_ascii_lowercase()).filter(|s| COLORS.contains(&s.as_str())),
                develop: r.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut tags: HashMap<i64, Vec<String>> = HashMap::new();
    for sql in [
        "SELECT ki.image, k.name FROM AgLibraryKeywordImage ki JOIN AgLibraryKeyword k ON k.id_local = ki.tag WHERE k.name IS NOT NULL",
        // plain collections only: smart collections are rules, sets are folders of collections
        "SELECT ci.image, c.name FROM AgLibraryCollectionImage ci JOIN AgLibraryCollection c ON c.id_local = ci.collection
         WHERE c.creationId = 'com.adobe.ag.library.collection' AND c.name IS NOT NULL",
    ] {
        let mut s = conn.prepare(sql)?;
        for t in s.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
            let (image, name) = t?;
            tags.entry(image).or_default().push(name);
        }
    }
    let copies: i64 = conn.query_row("SELECT COUNT(*) FROM Adobe_images WHERE masterImage IS NOT NULL", [], |r| r.get(0))?;
    Ok((rows, tags, copies as usize))
}

fn merge(row: &Row, names: &[String], report: &mut LrImportReport) -> Result<(), String> {
    let (_, sidecar) = parse_virtual_path(&row.path.to_string_lossy());
    let mut meta = crate::exif_processing::load_sidecar(&sidecar);
    if meta.rating == 0 {
        meta.rating = row.rating;
    }
    let mut tags = meta.tags.take().unwrap_or_default();
    let has_color = tags.iter().any(|t| t.starts_with(COLOR_TAG_PREFIX));
    let mut add = |t: String| {
        if !tags.contains(&t) {
            tags.push(t);
        }
    };
    if let Some(c) = &row.label
        && !has_color
    {
        add(format!("{COLOR_TAG_PREFIX}{c}"));
    }
    match row.pick {
        1 => add(format!("{USER_TAG_PREFIX}Pick")),
        -1 => add(format!("{USER_TAG_PREFIX}Rejected")),
        _ => {}
    }
    for n in names {
        add(format!("{USER_TAG_PREFIX}{n}"));
    }
    meta.tags = (!tags.is_empty()).then_some(tags);
    if let Some(adj) = row.develop.as_deref().and_then(develop_adjustments) {
        report.with_develop += 1;
        ensure_original_version(&mut meta);
        meta.versions.retain(|v| v.name != LIGHTROOM_VERSION || v.auto || v.original);
        push_version(
            &mut meta.versions,
            PhotoVersion {
                id: uuid::Uuid::new_v4().to_string(),
                name: LIGHTROOM_VERSION.into(),
                created_at: now_seconds(),
                auto: false,
                original: false,
                adjustments: adj.clone(),
            },
        );
        if meta.adjustments.is_null() {
            meta.adjustments = adj;
        }
    }
    let json = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    write_atomic(&sidecar, json).map_err(|e| format!("{}: {e}", sidecar.display()))
}

pub fn import(catalog: &Path) -> Result<LrImportReport, String> {
    let snap = snapshot(catalog)?;
    let mut conn = Connection::open_with_flags(&snap.0, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|e| format!("Not a Lightroom catalog: {e}"))?;
    // read-write only so SQLite can fold the copied log in; the snapshot is thrown away after
    let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0)).map_err(|e| format!("Not a Lightroom catalog: {e}"))?;
    if check != "ok" {
        return Err(format!("The catalog failed SQLite's integrity check ({check}); open it in Lightroom to repair it first."));
    }
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let (rows, tags, copies) = read_catalog(&tx).map_err(|e| format!("Could not read the catalog: {e}"))?;
    drop(tx);
    let mut report = LrImportReport { virtual_copies_skipped: copies, ..Default::default() };
    for row in &rows {
        if !row.path.exists() {
            report.missing += 1;
            continue;
        }
        merge(row, tags.get(&row.id).map_or(&[][..], |v| v), &mut report)?;
        report.imported += 1;
    }
    log::info!("Lightroom catalog {}: {report:?}", catalog.display());
    Ok(report)
}

#[tauri::command]
pub async fn import_lightroom_catalog(path: String) -> Result<LrImportReport, String> {
    tauri::async_runtime::spawn_blocking(move || import(Path::new(&path))).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVELOP: &str = "s = {\n\tBlacks2012 = -12,\n\tCameraProfile = \"Adobe Standard\",\n\tExposure2012 = 0.35,\n\tLook = {\n\t\tName = \"Vivid\",\n\t\tParameters = {\n\t\t\tExposure2012 = 3,\n\t\t},\n\t},\n\tToneCurvePV2012 = {\n\t\t0,\n\t\t0,\n\t\t128,\n\t\t140,\n\t\t255,\n\t\t255,\n\t},\n\tVibrance = 20,\n}\n";

    #[test]
    fn develop_text_becomes_our_adjustments() {
        let a = develop_adjustments(DEVELOP).unwrap();
        assert_eq!(a["exposure"].as_f64(), Some(0.35), "the Look's nested exposure must not leak out");
        assert_eq!(a["blacks"].as_f64(), Some(-12.0));
        assert_eq!(a["vibrance"].as_f64(), Some(20.0));
        assert_eq!(a["curves"]["luma"].as_array().map(|p| p.len()), Some(3));
    }

    /// A catalog with Lightroom Classic's table and column names (only the ones read), built here: no real
    /// catalog was available to test against.
    fn catalog(dir: &Path, photo_root: &Path) -> PathBuf {
        let p = dir.join("Test.lrcat");
        let c = Connection::open(&p).unwrap();
        c.execute_batch(&format!(
            "CREATE TABLE AgLibraryRootFolder (id_local INTEGER PRIMARY KEY, absolutePath TEXT, name TEXT);
             CREATE TABLE AgLibraryFolder (id_local INTEGER PRIMARY KEY, rootFolder INTEGER, pathFromRoot TEXT);
             CREATE TABLE AgLibraryFile (id_local INTEGER PRIMARY KEY, folder INTEGER, baseName TEXT, extension TEXT);
             CREATE TABLE Adobe_images (id_local INTEGER PRIMARY KEY, rootFile INTEGER, rating REAL, pick REAL, colorLabels TEXT, masterImage INTEGER);
             CREATE TABLE Adobe_imageDevelopSettings (id_local INTEGER PRIMARY KEY, image INTEGER, text TEXT);
             CREATE TABLE AgLibraryKeyword (id_local INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE AgLibraryKeywordImage (id_local INTEGER PRIMARY KEY, image INTEGER, tag INTEGER);
             CREATE TABLE AgLibraryCollection (id_local INTEGER PRIMARY KEY, name TEXT, creationId TEXT);
             CREATE TABLE AgLibraryCollectionImage (id_local INTEGER PRIMARY KEY, image INTEGER, collection INTEGER);
             INSERT INTO AgLibraryRootFolder VALUES (1, '{root}/', 'Photos');
             INSERT INTO AgLibraryFolder VALUES (1, 1, 'trip/');
             INSERT INTO AgLibraryFile VALUES (1, 1, 'stork', 'jpg'), (2, 1, 'gone', 'jpg'), (3, 1, 'edited', 'jpg'), (4, 1, 'plain', 'jpg');
             INSERT INTO Adobe_images VALUES (10, 1, 4, 1, 'Red', NULL), (11, 2, 2, 0, '', NULL), (12, 1, 5, 0, '', 10), (13, 3, 3, -1, 'Blue', NULL), (14, 4, 0, 0, '', NULL);
             INSERT INTO AgLibraryKeyword VALUES (1, 'bird'), (2, 'spring');
             INSERT INTO AgLibraryKeywordImage VALUES (1, 10, 1), (2, 10, 2);
             INSERT INTO AgLibraryCollection VALUES (1, 'Portfolio', 'com.adobe.ag.library.collection'), (2, 'Recent', 'com.adobe.ag.library.smart_collection');
             INSERT INTO AgLibraryCollectionImage VALUES (1, 10, 1), (2, 10, 2);",
            root = photo_root.to_string_lossy().replace('\\', "/")
        ))
        .unwrap();
        c.execute("INSERT INTO Adobe_imageDevelopSettings (image, text) VALUES (10, ?1), (13, ?1)", [DEVELOP]).unwrap();
        p
    }

    #[test]
    fn imports_ratings_labels_tags_and_develop_without_touching_existing_work() {
        let dir = std::env::temp_dir().join(format!("grain-lrcat-test-{}", std::process::id()));
        let trip = dir.join("trip");
        std::fs::create_dir_all(&trip).unwrap();
        std::fs::write(trip.join("stork.jpg"), b"jpg").unwrap();
        std::fs::write(trip.join("edited.jpg"), b"jpg").unwrap();
        std::fs::write(trip.join("plain.jpg"), b"jpg").unwrap();
        // a photo already edited and rated in Grain Studio
        let mut mine = crate::image_processing::ImageMetadata::default();
        mine.rating = 1;
        mine.adjustments = serde_json::json!({"exposure": -1.0});
        std::fs::write(trip.join("edited.jpg.rrdata"), serde_json::to_string(&mine).unwrap()).unwrap();
        let cat = catalog(&dir, &dir);
        let before = std::fs::read(&cat).unwrap();

        let r = import(&cat).unwrap();
        assert_eq!(r, LrImportReport { imported: 3, missing: 1, virtual_copies_skipped: 1, with_develop: 2 });
        assert_eq!(std::fs::read(&cat).unwrap(), before, "the catalog itself is never written");

        let (_, side) = parse_virtual_path(&trip.join("stork.jpg").to_string_lossy());
        let m = crate::exif_processing::load_sidecar(&side);
        assert_eq!(m.rating, 4);
        let tags = m.tags.unwrap();
        for t in ["color:red", "user:Pick", "user:bird", "user:spring", "user:Portfolio"] {
            assert!(tags.contains(&t.to_string()), "missing {t} in {tags:?}");
        }
        assert!(!tags.contains(&"user:Recent".to_string()), "smart collections are not tags");
        assert_eq!(m.adjustments["exposure"].as_f64(), Some(0.35));
        assert!(m.versions.iter().any(|v| v.name == "Lightroom") && m.versions.iter().any(|v| v.original));

        let (_, side) = parse_virtual_path(&trip.join("edited.jpg").to_string_lossy());
        let m = crate::exif_processing::load_sidecar(&side);
        assert_eq!(m.rating, 1, "an existing rating stays");
        assert_eq!(m.adjustments["exposure"].as_f64(), Some(-1.0), "an existing edit stays current");
        assert!(m.versions.iter().any(|v| v.name == "Lightroom"), "Lightroom's edit is one click away");
        assert!(m.tags.unwrap().contains(&"user:Rejected".to_string()));

        let (_, side_plain) = parse_virtual_path(&trip.join("plain.jpg").to_string_lossy());
        assert!(crate::exif_processing::load_sidecar(&side_plain).tags.is_none(), "an unflagged photo is not a pick");

        // importing again changes nothing but the Lightroom version's timestamp
        import(&cat).unwrap();
        let m = crate::exif_processing::load_sidecar(&side);
        assert_eq!(m.versions.iter().filter(|v| v.name == "Lightroom").count(), 1);

        std::fs::write(with_suffix(&cat, ".lock"), b"").unwrap();
        assert!(import(&cat).unwrap_err().contains("open"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
