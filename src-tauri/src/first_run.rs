//! What the setup on a fresh install needs to know and do: what this computer can run, whether a local model
//! server is there, and pulling a model with progress. The choosing is done in the app; this only reports
//! facts and carries out what was chosen.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

/// Where Ollama listens unless it was moved.
const OLLAMA: &str = "http://127.0.0.1:11434";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hardware {
    /// Total system memory, in whole gigabytes.
    pub ram_gb: u32,
    pub cpu_cores: u32,
    /// The graphics card's own memory, in whole gigabytes; 0 when there is none to speak of.
    pub vram_gb: u32,
    pub gpu_name: String,
    pub os: String,
}

/// The graphics card and how much memory it has of its own. Shared memory does not count: a model has to fit in
/// the card's own memory to run at a sensible speed.
#[cfg(windows)]
fn graphics() -> (String, u32) {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
            return (String::new(), 0);
        };
        let mut best = (String::new(), 0u64);
        for index in 0..8 {
            let Ok(adapter) = factory.EnumAdapters1(index) else { break };
            let Ok(desc) = adapter.GetDesc1() else { continue };
            let name = String::from_utf16_lossy(&desc.Description)
                .trim_end_matches('\0')
                .trim()
                .to_string();
            // Skip Microsoft's software adapter, which reports memory it cannot really use.
            if name.contains("Basic Render") {
                continue;
            }
            let memory = desc.DedicatedVideoMemory as u64;
            if memory > best.1 {
                best = (name, memory);
            }
        }
        (best.0, (best.1 / 1_073_741_824) as u32)
    }
}

#[cfg(not(windows))]
fn graphics() -> (String, u32) {
    // Elsewhere the card is named by the renderer already in use; its memory is not worth guessing at.
    (String::new(), 0)
}

/// What this computer is, so the setup can say what it can and cannot run.
#[tauri::command]
pub fn detect_hardware() -> Hardware {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    let (gpu_name, vram_gb) = graphics();
    Hardware {
        ram_gb: (system.total_memory() / 1_073_741_824) as u32,
        cpu_cores: std::thread::available_parallelism().map_or(4, |n| n.get() as u32),
        vram_gb,
        gpu_name,
        os: std::env::consts::OS.to_string(),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalServer {
    pub running: bool,
    /// Models it already has, newest first as the server lists them.
    pub models: Vec<String>,
}

/// Whether a local model server is up, and what it holds. Checked rather than assumed, because the whole point
/// of the local path is that it works without an account, and a wrong claim about it wastes someone's evening.
#[tauri::command]
pub async fn local_server_status(url: Option<String>) -> LocalServer {
    let base = url.unwrap_or_else(|| OLLAMA.to_string());
    let base = base.trim_end_matches('/').trim_end_matches("/v1").to_string();
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{base}/api/tags"))
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await;
    match response {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().await.unwrap_or_default();
            let models = body["models"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .filter_map(|m| m["name"].as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            LocalServer { running: true, models }
        }
        _ => LocalServer { running: false, models: Vec::new() },
    }
}

/// Downloads a model, reporting progress as `local-model-pull` events ({percent, status, done, error}) so the
/// setup can show a bar instead of a spinner over several gigabytes.
#[tauri::command]
pub async fn pull_local_model(app: AppHandle, model: String, url: Option<String>) -> Result<(), String> {
    use futures::StreamExt;

    let base = url.unwrap_or_else(|| OLLAMA.to_string());
    let base = base.trim_end_matches('/').trim_end_matches("/v1").to_string();
    let response = reqwest::Client::new()
        .post(format!("{base}/api/pull"))
        .json(&serde_json::json!({ "model": model, "stream": true }))
        .send()
        .await
        .map_err(|e| format!("Could not reach the model server: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("The model server refused to download {model}"));
    }

    let mut stream = response.bytes_stream();
    let mut pending = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("The download stopped: {e}"))?;
        pending.push_str(&String::from_utf8_lossy(&chunk));
        // Ollama sends one JSON object per line, and a chunk can split a line in half.
        while let Some(line_end) = pending.find('\n') {
            let line = pending[..line_end].trim().to_string();
            pending.drain(..=line_end);
            if line.is_empty() {
                continue;
            }
            let Ok(update): Result<serde_json::Value, _> = serde_json::from_str(&line) else {
                continue;
            };
            if let Some(error) = update["error"].as_str() {
                let _ = app.emit("local-model-pull", serde_json::json!({ "error": error }));
                return Err(error.to_string());
            }
            let total = update["total"].as_f64().unwrap_or(0.0);
            let completed = update["completed"].as_f64().unwrap_or(0.0);
            let percent = if total > 0.0 { (completed / total * 100.0).round() } else { 0.0 };
            let _ = app.emit(
                "local-model-pull",
                serde_json::json!({
                    "status": update["status"].as_str().unwrap_or(""),
                    "percent": percent,
                    "done": update["status"].as_str() == Some("success"),
                }),
            );
        }
    }
    Ok(())
}
