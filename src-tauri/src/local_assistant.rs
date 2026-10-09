//! The assistant on a local model (Ollama, LM Studio, llama.cpp or any OpenAI-compatible server),
//! so Grain Studio works without a Claude subscription. It gets the same studio bridge tools and
//! instructions as Claude and emits the same chat events, so the panel doesn't care which runs.

use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

use crate::studio_bridge::bridge_url;

pub const DEFAULT_URL: &str = "http://127.0.0.1:11434/v1";
/// Tool rounds per message before giving up, so a confused small model can't loop forever.
const MAX_STEPS: usize = 24;

static HISTORY: Mutex<Vec<Value>> = Mutex::const_new(Vec::new());
/// Bumped by reset: a reply still running for an older conversation stops and stays silent.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Minimal MCP client for the in-app studio bridge (JSON-RPC over streamable HTTP).
struct Bridge {
    client: reqwest::Client,
    session: Option<String>,
}

impl Bridge {
    async fn connect(client: reqwest::Client) -> Result<Self, String> {
        let mut bridge = Bridge { client, session: None };
        bridge
            .rpc(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                "protocolVersion": "2025-03-26", "capabilities": {},
                "clientInfo": {"name": "grain-studio-local", "version": "1"}}}))
            .await?;
        bridge.rpc(json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).await?;
        Ok(bridge)
    }

    async fn rpc(&mut self, body: Value) -> Result<Value, String> {
        let mut req = self
            .client
            .post(bridge_url())
            .header("Accept", "application/json, text/event-stream")
            .json(&body);
        if let Some(session) = &self.session {
            req = req.header("Mcp-Session-Id", session);
        }
        let resp = req.send().await.map_err(|e| format!("The studio bridge is offline: {e}"))?;
        if let Some(session) = resp.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
            self.session = Some(session.to_string());
        }
        let text = resp.text().await.map_err(|e| e.to_string())?;
        // Replies come as plain JSON or as server-sent events; take the one answering a request.
        Ok(text
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .chain(std::iter::once(text.as_str()))
            .filter_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
            .find(|m| m.get("id").is_some())
            .map(|m| m["result"].clone())
            .unwrap_or(Value::Null))
    }

    /// Studio tools in the OpenAI function-calling format.
    async fn tools(&mut self) -> Result<Vec<Value>, String> {
        let list = self.rpc(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).await?;
        Ok(list["tools"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|t| {
                json!({"type": "function", "function": {
                    "name": t["name"], "description": t["description"], "parameters": t["inputSchema"]}})
            })
            .collect())
    }

    /// Runs one tool; returns its text and any images as data URLs.
    async fn call(&mut self, name: &str, args: Value) -> (String, Vec<String>) {
        let result = match self
            .rpc(json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                "params": {"name": name, "arguments": args}}))
            .await
        {
            Ok(r) => r,
            Err(e) => return (format!("Error: {e}"), vec![]),
        };
        let (mut text, mut images) = (String::new(), vec![]);
        for block in result["content"].as_array().into_iter().flatten() {
            match block["type"].as_str() {
                Some("text") => text.push_str(block["text"].as_str().unwrap_or_default()),
                Some("image") => images.push(format!(
                    "data:{};base64,{}",
                    block["mimeType"].as_str().unwrap_or("image/jpeg"),
                    block["data"].as_str().unwrap_or_default()
                )),
                _ => {}
            }
        }
        if result["isError"].as_bool() == Some(true) {
            text = format!("Error: {text}");
        } else if text.is_empty() {
            text = if images.is_empty() { "Done.".into() } else { "Image attached below.".into() };
        }
        (text, images)
    }
}

/// Send one message; the reply streams as `claude-chat-event`s and ends with a `result` event.
pub async fn send(app: AppHandle, text: String, url: Option<String>, model: Option<String>) -> Result<(), String> {
    let url = url.filter(|u| !u.trim().is_empty()).unwrap_or_else(|| DEFAULT_URL.into());
    let url = url.trim_end_matches('/').to_string();
    let generation = GENERATION.load(Ordering::SeqCst);
    {
        let mut history = HISTORY.lock().await;
        if history.is_empty() {
            history.push(json!({"role": "system", "content": crate::claude_chat::SYSTEM_PROMPT}));
        }
        history.push(json!({"role": "user", "content": text}));
    }
    tauri::async_runtime::spawn(async move {
        let emit = |event: Value| {
            let _ = app.emit("claude-chat-event", event);
        };
        let outcome = run(&emit, generation, &url, model).await;
        if GENERATION.load(Ordering::SeqCst) == generation {
            emit(match outcome {
                Ok(()) => json!({"type": "result", "is_error": false}),
                Err(e) => json!({"type": "result", "is_error": true, "result": e}),
            });
        }
    });
    Ok(())
}

async fn run(emit: &(dyn Fn(Value) + Sync), generation: u64, url: &str, model: Option<String>) -> Result<(), String> {
    let client = reqwest::Client::new();
    let unreachable = |e: reqwest::Error| {
        format!("Could not reach the local model at {url} ({e}). Start Ollama or your model server, or check the address in Settings.")
    };
    let model = match model.filter(|m| !m.trim().is_empty()) {
        Some(m) => m,
        // No model chosen: use the first one the server has.
        None => client
            .get(format!("{url}/models"))
            .send()
            .await
            .map_err(unreachable)?
            .json::<Value>()
            .await
            .ok()
            .and_then(|v| v["data"][0]["id"].as_str().map(String::from))
            .ok_or("The local model server has no models. Download one first (for example: ollama pull gemma4).")?,
    };
    let mut bridge = Bridge::connect(client.clone()).await?;
    let tools = bridge.tools().await?;

    for _ in 0..MAX_STEPS {
        let messages = HISTORY.lock().await.clone();
        let resp = client
            .post(format!("{url}/chat/completions"))
            .json(&json!({"model": model, "messages": messages, "tools": tools}))
            .send()
            .await
            .map_err(unreachable)?;
        let status = resp.status();
        let body: Value = resp.json().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            let detail = body["error"]["message"].as_str().map(String::from).unwrap_or_else(|| body.to_string());
            return Err(format!("The local model ({model}) failed: {detail}"));
        }
        if GENERATION.load(Ordering::SeqCst) != generation {
            return Ok(());
        }

        let message = body["choices"][0]["message"].clone();
        let calls = message["tool_calls"].as_array().cloned().unwrap_or_default();
        let reply = message["content"].as_str().unwrap_or_default().to_string();
        HISTORY.lock().await.push(json!({"role": "assistant", "content": reply, "tool_calls": message["tool_calls"]}));

        let mut blocks = vec![];
        if !reply.trim().is_empty() {
            blocks.push(json!({"type": "text", "text": reply}));
        }
        blocks.extend(calls.iter().map(|c| json!({"type": "tool_use", "name": c["function"]["name"]})));
        if !blocks.is_empty() {
            emit(json!({"type": "assistant", "message": {"content": blocks}}));
        }
        if calls.is_empty() {
            return Ok(());
        }

        let mut images = vec![];
        for call in &calls {
            let name = call["function"]["name"].as_str().unwrap_or_default();
            // Arguments are a JSON string in the OpenAI format; some servers send an object.
            let args = match &call["function"]["arguments"] {
                Value::String(s) => serde_json::from_str(s).unwrap_or(json!({})),
                other if other.is_object() => other.clone(),
                _ => json!({}),
            };
            let (text, mut found) = bridge.call(name, args).await;
            images.append(&mut found);
            HISTORY.lock().await.push(json!({"role": "tool", "tool_call_id": call["id"], "content": text}));
        }
        if !images.is_empty() {
            let mut history = HISTORY.lock().await;
            // ponytail: only the newest images stay in the conversation; small local models run out of context fast.
            for old in history.iter_mut() {
                if let Some(parts) = old["content"].as_array_mut() {
                    parts.retain(|p| p["type"] != "image_url");
                }
            }
            let mut parts = vec![json!({"type": "text", "text": "Images returned by the tools:"})];
            parts.extend(images.into_iter().map(|u| json!({"type": "image_url", "image_url": {"url": u}})));
            history.push(json!({"role": "user", "content": parts}));
        }
    }
    Err(format!("Stopped after {MAX_STEPS} tool steps without an answer. Try a clearer request or a larger model."))
}

pub async fn reset() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    HISTORY.lock().await.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs the app running (studio bridge) and Ollama with a tool-capable model:
    /// cargo test --no-default-features local_assistant -- --ignored --nocapture
    /// (LOCAL_MODEL picks the model, LOCAL_PROMPT the request.)
    #[test]
    #[ignore]
    fn local_model_uses_studio_tools() {
        tauri::async_runtime::block_on(async {
            let events = std::sync::Mutex::new(Vec::new());
            HISTORY.lock().await.extend([
                json!({"role": "system", "content": crate::claude_chat::SYSTEM_PROMPT}),
                json!({"role": "user", "content": std::env::var("LOCAL_PROMPT")
                    .unwrap_or_else(|_| "Call get_state, then tell me in one line which photo is open.".into())}),
            ]);
            let started = std::time::Instant::now();
            let record = |e: Value| {
                println!("{e}");
                events.lock().unwrap().push(e);
            };
            run(&record, 0, DEFAULT_URL, std::env::var("LOCAL_MODEL").ok()).await.expect("reply");
            println!("answered in {:.1}s", started.elapsed().as_secs_f32());
            let events = events.lock().unwrap();
            assert!(events.iter().any(|e| e.to_string().contains("tool_use")), "never used a studio tool");
        });
    }
}
