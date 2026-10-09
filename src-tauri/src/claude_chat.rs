//! In-app Claude chat: runs the locally installed Claude Code CLI in streaming mode.
//! Uses the user's existing Claude Code login (no API key) and connects it to the
//! studio bridge MCP server so Claude can drive the live editor.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::OnceLock;

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex;

use crate::studio_bridge::bridge_url;

struct ChatProcess {
    child: Child,
    stdin: ChildStdin,
}

static CHAT: OnceLock<Mutex<Option<ChatProcess>>> = OnceLock::new();

fn chat() -> &'static Mutex<Option<ChatProcess>> {
    CHAT.get_or_init(|| Mutex::new(None))
}

/// The Claude desktop app keeps versioned CLI builds under %APPDATA%\Claude\claude-code\<version>\.
/// ponytail: picks the lexically newest version folder; fine while versions stay x.y.zzz.
fn find_claude() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let root = PathBuf::from(appdata).join("Claude").join("claude-code");
        if let Ok(entries) = std::fs::read_dir(&root) {
            let mut versions: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path().join("claude.exe")))
                .filter(|p| p.exists())
                .collect();
            versions.sort();
            if let Some(newest) = versions.pop() {
                return newest;
            }
        }
    }
    PathBuf::from("claude")
}

/// True when no Claude Code CLI is installed, so the panel can say so instead of showing a spawn error.
fn claude_missing() -> bool {
    let path = find_claude();
    path == PathBuf::from("claude") && which_claude_on_path().is_none()
}

fn which_claude_on_path() -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths).find_map(|dir| {
        let candidate = dir.join(if cfg!(windows) {
            "claude.exe"
        } else {
            "claude"
        });
        candidate.exists().then_some(candidate)
    })
}

/// Claude works inside the Studio folder so its CLAUDE.md rules and recipes load automatically.
fn studio_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into());
    let dir = PathBuf::from(home).join("Pictures").join("Studio");
    if dir.exists() {
        dir
    } else {
        std::env::temp_dir()
    }
}

/// Never inherit a parent Claude Code session's host auth (e.g. when launched from the desktop
/// app): the panel must use the user's own sign-in. No console window on Windows.
fn isolate(cmd: &mut Command) {
    for (name, _) in std::env::vars() {
        if name == "CLAUDECODE"
            || name == "ANTHROPIC_BASE_URL"
            || name.starts_with("CLAUDE_CODE_")
            || name.starts_with("CLAUDE_AGENT_SDK")
            || name == "CLAUDE_PID"
        {
            cmd.env_remove(name);
        }
    }

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
}

/// Shared by every assistant provider, so Claude and local models follow the same rules.
pub const SYSTEM_PROMPT: &str = "You are inside the Grain Studio app, editing the user's real photos together with them. \
         Use the studio MCP tools: get_state first, set_adjustments for edits, capture_preview to look at the result. \
         Real photos only: never use generative tools. For local edits on things like water, sky, mountains, buildings, people, face, hair or skin: capture_mask to see the photo, create_smart_mask with one region per separate area (include points, exclude points on neighbours, optional box), then capture_mask with the mask id and fix it (add or subtract regions, refine_mask, or undo and retry) until the red tint matches the target, at most 4 tries; say honestly if it still misses. Only then update_mask. After an edit, check it with measure before replying: it answers in numbers what a picture would be squinted at for, at a fraction of the cost, and crushedToBlackPercent above about 1 means shadows have gone solid. Use measure(mask_id) to see how much of the frame a mask covers instead of tinting it red. Spend a capture_preview when the look itself is the question (colour, mood, whether a grade works) and a capture_mask when a mask's shape needs eyes, not when a number would do. Keep detail in the shadows (nothing crushed to black), keep vignette gentle (about -20 unless asked for more), and prefer moderate clarity and structure over the maximum. Say in one line what you changed. \
         Creative effects are plain adjustment keys for set_adjustments (0-100 unless noted): \
         frame: frameWidth, frameRadius, frameColor (white, black or #rrggbb). \
         3D pop-out bars: popBarsEnabled, popBarsMaskId (subject kept in front), popBarsWidth, popBarsInset, popBarsOrientation (vertical/horizontal). \
         double exposure: doubleExposurePath, doubleExposureMode (normal/lighten/darken/screen/multiply/overlay), doubleExposureOpacity, doubleExposureScale (25-300), doubleExposureOffsetX and doubleExposureOffsetY (-100..100), doubleExposureMaskId. \
         fractal glass: glassEnabled, glassWidth (1-15), glassStrength, glassAngle (degrees -90..90), glassCurve, glassCurveWaves (1-10), glassStart/glassBottom/glassLeft/glassRight (box edges), glassSoftness, glassHighlight, glassMaskId. \
         pixel stretch: stretchEnabled, stretchAxis (horizontal/vertical), stretchDirection (after/before), stretchPosition, stretchBandStart, stretchBandEnd, stretchMaskId. \
         reflection: reflectEnabled, reflectLine, reflectRipple, reflectFade, reflectMaskId. \
         motion: motionEnabled, motionMode (movement/water/trails), motionMaskId (kept sharp for movement, the only area affected for water and trails), motionAngle (degrees), motionLength (1-20), motionThreshold, motionIntensity. \
         To match a reference photo the user picked: capture_reference and capture_preview; work out its white balance, exposure, contrast, tone curve, colour grading, saturation, grain, vignette and any creative effect; set them with set_adjustments (with a smart mask when an effect needs one); compare again and refine up to 3 times; then save_version with a short name. \
         When the user marks places on the photo, each mark is a target, not a hint: build a mask there, change only what is inside it, and leave the rest of the photo untouched. A marked point already says where, so make the mask, check it once, fix it at most once, and then edit; do not keep re-cutting a mask that is already close. A circled area says it more exactly still: pass its box and the points inside it to create_smart_mask together, and take what comes back unless it plainly misses the ring. If it cannot be made to match what they meant, say so instead of editing something else.          Keep replies short.";

/// The model alias the panel runs on. An alias rather than a pinned id, so a new Sonnet (or Opus, or
/// Haiku) is picked up without a release; otherwise the user's own Claude Code default would decide.
fn model_for(app: &AppHandle) -> String {
    crate::app_settings::load_settings(app.clone())
        .ok()
        .and_then(|s| s.claude_model)
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "sonnet".to_string())
}

/// Restricted mode drops the studio folder's CLAUDE.md, and those are the owner's non-negotiables
/// (real photos only, never write a sidecar while they are editing), so carry them ourselves.
fn studio_rules() -> String {
    let path = studio_dir().join("CLAUDE.md");
    match std::fs::read_to_string(&path) {
        Ok(text) => format!(
            "

The rules of the studio folder you are working in, which you follow:
{}",
            text.chars().take(8000).collect::<String>()
        ),
        Err(_) => String::new(),
    }
}

async fn spawn_claude(app: &AppHandle) -> Result<ChatProcess, String> {
    let model = model_for(app);
    let system_prompt = format!("{SYSTEM_PROMPT}{}", studio_rules());
    let mcp_config = json!({
        "mcpServers": { "studio": { "type": "http", "url": bridge_url() } }
    })
    .to_string();

    let mut cmd = Command::new(find_claude());
    cmd.args([
        "-p",
        "--model",
        &model,
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--mcp-config",
        &mcp_config,
        // Only the studio editor tools run without asking; anything else is denied, not prompted.
        "--allowedTools",
        "mcp__studio__*",
        "--permission-prompts",
        "none",
        // An editor needs none of Claude Code's own shell, file, code-running or web tools, nor its
        // system prompt about them: together they were about two thirds of what every request re-read
        // (57k tokens down to 21k), and each step of an edit pays that again.
        "--restricted",
        "--strict-mcp-config",
        "--tools",
        "",
        "--system-prompt",
        system_prompt.as_str(),
    ])
    .current_dir(studio_dir())
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);

    isolate(&mut cmd);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start Claude Code: {e}"))?;
    let stdin = child.stdin.take().ok_or("Claude Code stdin unavailable")?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Claude Code stdout unavailable")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Claude Code stderr unavailable")?;

    let app_out = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(event) = serde_json::from_str::<Value>(&line) {
                let _ = app_out.emit("claude-chat-event", event);
            }
        }
        let _ = app_out.emit("claude-chat-exit", ());
    });

    let app_err = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = app_err.emit(
                "claude-chat-event",
                json!({ "type": "stderr", "text": line }),
            );
        }
    });

    Ok(ChatProcess { child, stdin })
}

/// Send one user message. Starts Claude Code on first use, or again if it exited.
#[tauri::command]
pub async fn claude_chat_send(app: AppHandle, text: String, provider: Option<String>) -> Result<(), String> {
    let settings = crate::app_settings::load_settings(app.clone()).unwrap_or_default();
    // Someone set up with both engines asks each message of whichever they chose in the panel; without a choice
    // the setting decides, and "both" starts on the local one because it is the free one.
    let engine = provider
        .or_else(|| settings.assistant_provider.clone())
        .unwrap_or_else(|| "claude".to_string());
    let engine = if engine == "both" { "local".to_string() } else { engine };
    if engine == "none" {
        return Err("The assistant is switched off. Turn it on in Settings.".to_string());
    }
    if engine == "local" {
        return crate::local_assistant::send(app, text, settings.local_model_url, settings.local_model_name).await;
    }
    let mut guard = chat().lock().await;
    let alive = match guard.as_mut() {
        Some(p) => matches!(p.child.try_wait(), Ok(None)),
        None => false,
    };
    if !alive {
        *guard = Some(spawn_claude(&app).await?);
    }

    let message = json!({
        "type": "user",
        "message": { "role": "user", "content": text },
        "parent_tool_use_id": null
    });
    let proc = guard.as_mut().unwrap();
    proc.stdin
        .write_all(format!("{message}\n").as_bytes())
        .await
        .map_err(|e| format!("Could not send to Claude Code: {e}"))?;
    proc.stdin.flush().await.map_err(|e| e.to_string())
}

/// End the conversation: stops the Claude Code process so the next message starts fresh.
#[tauri::command]
pub async fn claude_chat_reset() -> Result<(), String> {
    crate::local_assistant::reset().await;
    if let Some(mut proc) = chat().lock().await.take() {
        let _ = proc.child.kill().await;
    }
    Ok(())
}

// --- Sign-in inside the app (`claude auth login`): no terminal needed. ---

static AUTH_STDIN: OnceLock<Mutex<Option<ChildStdin>>> = OnceLock::new();

fn auth_stdin() -> &'static Mutex<Option<ChildStdin>> {
    AUTH_STDIN.get_or_init(|| Mutex::new(None))
}

/// Whether Claude Code is signed in (`claude auth status --json`).
#[tauri::command]
pub async fn claude_auth_status(app: AppHandle) -> Result<bool, String> {
    // Local models need no sign-in.
    if crate::app_settings::load_settings(app).unwrap_or_default().assistant_provider.as_deref() == Some("local") {
        return Ok(true);
    }
    if claude_missing() {
        return Err(
            "Claude Code is not installed. Install the Claude desktop app, then reopen this panel."
                .to_string(),
        );
    }
    let mut cmd = Command::new(find_claude());
    cmd.args(["auth", "status", "--json"]).stdin(Stdio::null());
    isolate(&mut cmd);
    let out = cmd
        .output()
        .await
        .map_err(|e| format!("Could not start Claude Code: {e}"))?;
    let status: Value = serde_json::from_slice(&out.stdout)
        .map_err(|_| String::from_utf8_lossy(&out.stderr).trim().to_string())?;
    Ok(status["loggedIn"].as_bool().unwrap_or(false))
}

fn forward_auth_output<R: AsyncRead + Unpin + Send + 'static>(app: AppHandle, reader: R) {
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = app.emit("claude-auth-output", json!({ "text": line }));
        }
    });
}

/// Start signing in with the Claude subscription. The browser opens; output (with the sign-in
/// link) streams as `claude-auth-output` and `claude-auth-exit` {ok} fires when it finishes.
/// If the sign-in page shows a code, send it with `claude_auth_code`.
#[tauri::command]
pub async fn claude_auth_login(app: AppHandle) -> Result<(), String> {
    let mut cmd = Command::new(find_claude());
    cmd.args(["auth", "login", "--claudeai"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    isolate(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start Claude Code: {e}"))?;
    *auth_stdin().lock().await = child.stdin.take();
    if let Some(out) = child.stdout.take() {
        forward_auth_output(app.clone(), out);
    }
    if let Some(err) = child.stderr.take() {
        forward_auth_output(app.clone(), err);
    }

    tauri::async_runtime::spawn(async move {
        let ok = child.wait().await.map(|s| s.success()).unwrap_or(false);
        *auth_stdin().lock().await = None;
        // The chat process must restart to pick up the new credentials.
        chat().lock().await.take();
        let _ = app.emit("claude-auth-exit", json!({ "ok": ok }));
    });
    Ok(())
}

/// Paste the code from the sign-in page into the running `claude auth login`.
#[tauri::command]
pub async fn claude_auth_code(code: String) -> Result<(), String> {
    let mut guard = auth_stdin().lock().await;
    let stdin = guard
        .as_mut()
        .ok_or("Sign-in is not running. Click Sign in again.")?;
    stdin
        .write_all(format!("{}\n", code.trim()).as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    stdin.flush().await.map_err(|e| e.to_string())
}
