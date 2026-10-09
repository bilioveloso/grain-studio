//! Per-photo versions: named snapshots of an edit, plus automatic ones taken while you work,
//! so any state can be brought back later. They live in the photo's own sidecar.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::exif_processing::load_sidecar_with_exif;
use crate::file_management::parse_virtual_path;

/// Automatic snapshots are capped; named ones are kept for as long as the owner wants them.
const MAX_AUTO_VERSIONS: usize = 20;
/// A burst of edits produces one automatic snapshot, not one per slider move.
const AUTO_INTERVAL_SECONDS: i64 = 600;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PhotoVersion {
    pub id: String,
    pub name: String,
    /// Unix seconds; the frontend formats it.
    pub created_at: i64,
    /// True for snapshots the app took by itself.
    #[serde(default)]
    pub auto: bool,
    /// The photo as shot, captured before its first edit. Adjustments are null: restoring it
    /// means "back to defaults", which only the editor knows.
    #[serde(default)]
    pub original: bool,
    pub adjustments: Value,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VersionSummary {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub auto: bool,
    pub original: bool,
    /// The edit made in the iPhone's Photos app, read from the files beside the photo (never stored here)
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub iphone: bool,
    /// Apple's own render of that edit, to open as it is
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render: Option<String>,
    /// False when there is only Apple's render and no recipe to turn into sliders
    pub editable: bool,
}

/// The iPhone edit's id; it is read from disk each time, so it is never in the sidecar.
const IPHONE_ID: &str = "iphone-edit";

impl From<&PhotoVersion> for VersionSummary {
    fn from(v: &PhotoVersion) -> Self {
        VersionSummary {
            id: v.id.clone(),
            name: v.name.clone(),
            created_at: v.created_at,
            auto: v.auto,
            original: v.original,
            iphone: false,
            render: None,
            editable: true,
        }
    }
}

pub fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Newest first, so the list reads like a history.
fn sorted(versions: &[PhotoVersion]) -> Vec<VersionSummary> {
    let mut list: Vec<VersionSummary> = versions.iter().map(VersionSummary::from).collect();
    list.sort_by(|a, b| a.original.cmp(&b.original).then(b.created_at.cmp(&a.created_at)));
    list
}

fn write_sidecar(sidecar_path: &std::path::Path, metadata: &crate::image_processing::ImageMetadata) -> Result<(), String> {
    let json = serde_json::to_string_pretty(metadata).map_err(|e| e.to_string())?;
    crate::file_management::write_atomic(sidecar_path, json.as_bytes()).map_err(|e| e.to_string())
}

/// Add a version, dropping the oldest automatic ones once the cap is reached.
pub fn push_version(versions: &mut Vec<PhotoVersion>, version: PhotoVersion) {
    versions.push(version);
    let auto_count = versions.iter().filter(|v| v.auto).count();
    if auto_count > MAX_AUTO_VERSIONS {
        let mut to_drop = auto_count - MAX_AUTO_VERSIONS;
        let mut oldest: Vec<(i64, String)> = versions
            .iter()
            .filter(|v| v.auto)
            .map(|v| (v.created_at, v.id.clone()))
            .collect();
        oldest.sort();
        let doomed: Vec<String> = oldest
            .into_iter()
            .take(to_drop.max(0))
            .map(|(_, id)| id)
            .collect();
        versions.retain(|v| !doomed.contains(&v.id));
        to_drop = 0;
        let _ = to_drop;
    }
}

/// Snapshot the state a photo was in before this editing burst, at most once per interval.
/// Called while saving an edit, so working normally builds a history without any button.
pub fn maybe_auto_version(
    metadata: &mut crate::image_processing::ImageMetadata,
    incoming_adjustments: &Value,
) {
    let previous = metadata.adjustments.clone();
    if previous.is_null() || previous == *incoming_adjustments {
        return;
    }
    let now = now_seconds();
    let last_auto = metadata
        .versions
        .iter()
        .filter(|v| v.auto)
        .map(|v| v.created_at)
        .max()
        .unwrap_or(0);
    if now - last_auto < AUTO_INTERVAL_SECONDS {
        return;
    }
    push_version(
        &mut metadata.versions,
        PhotoVersion {
            id: uuid::Uuid::new_v4().to_string(),
            name: String::new(), // the frontend shows the time for automatic snapshots
            created_at: now,
            auto: true,
            original: false,
            adjustments: previous,
        },
    );
}

/// Before a photo's first saved edit, keep the untouched photo as a permanent "Original".
pub fn ensure_original_version(metadata: &mut crate::image_processing::ImageMetadata) {
    if metadata.versions.iter().any(|v| v.original) {
        return;
    }
    metadata.versions.push(PhotoVersion {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Original".to_string(),
        created_at: now_seconds(),
        auto: false,
        original: true,
        adjustments: Value::Null,
    });
}

#[tauri::command]
pub fn list_versions(path: String) -> Result<Vec<VersionSummary>, String> {
    let (source_path, sidecar_path) = parse_virtual_path(&path);
    let metadata = load_sidecar_with_exif(&sidecar_path, &source_path);
    let mut list = sorted(&metadata.versions);
    // Just above the Original: it is the first edit the photo ever had.
    if let Some(edit) = crate::apple_edits::find(&source_path) {
        let at = list.iter().position(|v| v.original).unwrap_or(list.len());
        list.insert(
            at,
            VersionSummary {
                id: IPHONE_ID.into(),
                name: "iPhone edit".into(),
                created_at: edit.modified,
                auto: false,
                original: false,
                iphone: true,
                render: edit.render.map(|p| p.to_string_lossy().into_owned()),
                editable: edit.adjustments.is_some(),
            },
        );
    }
    Ok(list)
}

/// Save the current edit as a named version.
#[tauri::command]
pub fn save_version(
    path: String,
    name: String,
    adjustments: Value,
) -> Result<Vec<VersionSummary>, String> {
    let (source_path, sidecar_path) = parse_virtual_path(&path);
    let mut metadata = load_sidecar_with_exif(&sidecar_path, &source_path);
    push_version(
        &mut metadata.versions,
        PhotoVersion {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.trim().to_string(),
            created_at: now_seconds(),
            auto: false,
            original: false,
            adjustments,
        },
    );
    write_sidecar(&sidecar_path, &metadata)?;
    Ok(sorted(&metadata.versions))
}

/// The adjustments stored in a version, for the editor to apply (undoable like any edit).
#[tauri::command]
pub fn restore_version(path: String, version_id: String) -> Result<Value, String> {
    let (source_path, sidecar_path) = parse_virtual_path(&path);
    if version_id == IPHONE_ID {
        return crate::apple_edits::find(&source_path)
            .and_then(|e| e.adjustments)
            .ok_or_else(|| "This iPhone edit has no recipe to turn into sliders; open Apple's render instead".into());
    }
    let metadata = load_sidecar_with_exif(&sidecar_path, &source_path);
    metadata
        .versions
        .iter()
        .find(|v| v.id == version_id)
        .map(|v| v.adjustments.clone())
        .ok_or_else(|| format!("No version with id {version_id}"))
}

#[tauri::command]
pub fn delete_version(path: String, version_id: String) -> Result<Vec<VersionSummary>, String> {
    let (source_path, sidecar_path) = parse_virtual_path(&path);
    let mut metadata = load_sidecar_with_exif(&sidecar_path, &source_path);
    metadata.versions.retain(|v| v.id != version_id || v.original);
    write_sidecar(&sidecar_path, &metadata)?;
    Ok(sorted(&metadata.versions))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(auto: bool, at: i64) -> PhotoVersion {
        PhotoVersion {
            id: format!("{}-{}", if auto { "auto" } else { "named" }, at),
            name: String::new(),
            created_at: at,
            auto,
            original: false,
            adjustments: Value::Null,
        }
    }

    #[test]
    fn auto_versions_are_capped_but_named_ones_survive() {
        let mut versions = vec![version(false, 1)];
        for i in 0..(MAX_AUTO_VERSIONS as i64 + 5) {
            push_version(&mut versions, version(true, 100 + i));
        }
        assert_eq!(versions.iter().filter(|v| v.auto).count(), MAX_AUTO_VERSIONS);
        assert_eq!(versions.iter().filter(|v| !v.auto).count(), 1, "named version must be kept");
        // The oldest automatic ones are the ones dropped.
        assert!(versions.iter().all(|v| !v.auto || v.created_at >= 105));
    }

    #[test]
    fn original_is_added_once_and_listed_last() {
        let mut metadata = crate::image_processing::ImageMetadata::default();
        push_version(&mut metadata.versions, version(false, 50));
        ensure_original_version(&mut metadata);
        ensure_original_version(&mut metadata);
        assert_eq!(metadata.versions.iter().filter(|v| v.original).count(), 1);
        let list = sorted(&metadata.versions);
        assert!(list.last().unwrap().original, "original must be the bottom entry");
    }
}
