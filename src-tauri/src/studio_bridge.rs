//! Studio bridge: a loopback MCP server that lets Claude Code drive the live editor.
//! Tool calls are forwarded to the frontend (`useStudioBridge.ts`), which applies them through the editor store.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        session::local::LocalSessionManager,
        tower::{StreamableHttpServerConfig, StreamableHttpService},
    },
};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;

/// Fixed local port Claude Code connects to: `claude mcp add --transport http studio http://127.0.0.1:47800/mcp`
/// Preferred address. Kept fixed so the port stays familiar, but not required: Windows hands whole port
/// bands to Hyper-V and WSL, and a machine where this one is reserved must still get its editor tools.
pub const BRIDGE_ADDR: &str = "127.0.0.1:47800";

/// Where the bridge actually listens, once it is up. Callers build their MCP url from this.
static BRIDGE_URL: OnceLock<String> = OnceLock::new();

/// The MCP endpoint to hand a client, e.g. http://127.0.0.1:47800/mcp.
pub fn bridge_url() -> String {
    BRIDGE_URL
        .get()
        .cloned()
        .unwrap_or_else(|| format!("http://{BRIDGE_ADDR}/mcp"))
}
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);

struct Bridge {
    app: AppHandle,
    pending: Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>,
}

static BRIDGE: OnceLock<Bridge> = OnceLock::new();

/// Ask the frontend to run `method` and wait for its answer.
async fn call_frontend(method: &str, params: Value) -> Result<Value, String> {
    let bridge = BRIDGE.get().ok_or("Studio bridge not started")?;
    let id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel();
    bridge.pending.lock().unwrap().insert(id.clone(), tx);

    if let Err(e) = bridge.app.emit(
        "studio-bridge-request",
        json!({ "id": id, "method": method, "params": params }),
    ) {
        bridge.pending.lock().unwrap().remove(&id);
        return Err(e.to_string());
    }

    match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err("Bridge request dropped".into()),
        Err(_) => {
            bridge.pending.lock().unwrap().remove(&id);
            Err(format!(
                "Editor did not answer '{method}' in time. Is the RapidRAW window open?"
            ))
        }
    }
}

#[tauri::command]
pub fn studio_bridge_respond(id: String, ok: bool, result: Value) {
    if let Some(bridge) = BRIDGE.get()
        && let Some(tx) = bridge.pending.lock().unwrap().remove(&id)
    {
        let _ = tx.send(if ok {
            Ok(result)
        } else {
            Err(result.as_str().unwrap_or("error").to_string())
        });
    }
}

fn to_tool_result(result: Result<Value, String>) -> Result<CallToolResult, McpError> {
    Ok(match result {
        Ok(v) => CallToolResult::success(vec![ContentBlock::text(
            serde_json::to_string_pretty(&v).unwrap_or_default(),
        )]),
        Err(e) => CallToolResult::error(vec![ContentBlock::text(e)]),
    })
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct OpenImageArgs {
    /// Absolute path of the photo to open in the editor.
    pub path: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetAdjustmentsArgs {
    /// Partial adjustments object, deep-merged into the current edit
    /// (e.g. {"exposure": 0.3, "colorGrading": {"highlights": {"hue": 40, "saturation": 12}}}).
    /// Arrays such as `masks` replace the existing value.
    pub patch: serde_json::Map<String, Value>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct SmartMaskRegion {
    /// Include points on this area, each [x, y] in 0..1 of the capture_mask frame (0,0 = top-left).
    /// Spread them over the area; up to 12 are used.
    #[serde(default)]
    pub points: Vec<[f64; 2]>,
    /// Exclude points on nearby things that must stay out (a boat on the water, a roof against the sky). Up to 8.
    #[serde(default)]
    pub exclude_points: Vec<[f64; 2]>,
    /// Optional box [x1, y1, x2, y2] in 0..1 around this area; keeps the selection inside it.
    #[serde(rename = "box")]
    pub bbox: Option<[f64; 4]>,
    /// Remove this area from the mask instead of adding it (fixes spill).
    #[serde(default)]
    pub subtract: bool,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SmartMaskArgs {
    /// Mask name shown in the Masks panel, e.g. "Water", "Mountains", "Face skin".
    pub name: Option<String>,
    /// 1-8 regions, one per separate area (a lake and its reflection, each person). They combine into one mask.
    pub regions: Vec<SmartMaskRegion>,
    /// Add these regions to this existing mask instead of creating a new one.
    pub mask_id: Option<String>,
    /// New masks only: select everything except the target (e.g. background).
    pub invert: Option<bool>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RefineMaskArgs {
    /// Mask id to limit.
    pub mask_id: String,
    /// "color" keeps only pixels close to the sampled color; "luminance" keeps pixels of similar brightness.
    pub by: String,
    /// Sample point [x, y] in 0..1 of the capture_mask frame, on a typical part of the target.
    pub point: [f64; 2],
    /// 1-100, how far from the sample still counts (default 30). Mask too tight: undo and raise it.
    pub tolerance: Option<f64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MaskViewArgs {
    /// Mask to show tinted red. Omit to get the plain photo for picking points.
    pub mask_id: Option<String>,
    /// Longest edge of the returned image in pixels (default 768, enough to judge a mask's edge).
    pub max_edge: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct UpdateMaskArgs {
    /// Mask id from get_state (adjustments.masks[].id) or create_smart_mask.
    pub mask_id: String,
    /// Partial mask: {"adjustments": {...}} is deep-merged; name, invert, opacity, visible replace.
    /// {"edges": {"refine", "contrast", "grow", "feather"}} sets the edge of its AI parts (smart, sky, subject):
    /// refine 0-100 snaps it to the photo's real edges (hair, branches, rooflines), contrast 0-100 clears grey haze,
    /// grow -100..100, feather 0-100.
    pub patch: serde_json::Map<String, Value>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MeasureArgs {
    /// A mask to measure as well, by id. Omit to measure only the photo.
    pub mask_id: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PreviewArgs {
    /// Longest edge of the returned preview in pixels (default 1024).
    pub max_edge: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CullArgs {
    /// Absolute paths of the photos to cull together (a burst, a shoot, a folder's worth).
    pub paths: Vec<String>,
    /// Flag frames where anyone's eyes are closed (fetches the small face models the first time). Default true.
    pub find_closed_eyes: Option<bool>,
    /// Sharpness below this is blurry (default 100; higher is stricter).
    pub blur_threshold: Option<f64>,
    /// How alike two frames must look to be one burst (default 28; lower is stricter).
    pub similarity_threshold: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct UpscaleArgs {
    /// Absolute path of a finished (exported) photo. Photos with Grain Studio edits and RAW files are refused.
    pub path: String,
    /// "classic" (Lanczos + light sharpening) or "ai" (Real-ESRGAN 4x; only when Clean Mode is off).
    pub method: String,
    /// Long edge of the result in pixels (default 3840).
    pub long_edge: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ViewImageArgs {
    /// Absolute path of any image file; it is shown exactly as it is on disk.
    pub path: String,
    /// Optional crop [x, y, width, height] as fractions 0..1, to inspect detail at full resolution.
    pub crop: Option<[f64; 4]>,
    /// Longest edge of the returned image in pixels (default 1024).
    pub max_edge: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReferenceArgs {
    /// Longest edge of the returned image in pixels (default 1024).
    pub max_edge: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SaveVersionArgs {
    /// Name shown in the Versions menu, e.g. "Matched look".
    pub name: String,
}

#[derive(Clone)]
pub struct StudioTools {
    tool_router: ToolRouter<StudioTools>,
}

/// ponytail: hand-kept list of the Effects panel sliders; regenerate from src/utils/adjustments.ts if they drift.
const EFFECTS_REFERENCE: &str = "Creative effects. Set them with set_adjustments like any slider; ranges are the UI slider limits.

Limit an effect to one area: set its mask field to a mask id from get_state (adjustments.masks[].id) or from create_smart_mask; null means the whole photo. Any mask works, including a brush mask the user painted, so an effect can follow a freehand stroke. Mask fields: glassMaskId, motionMaskId, popBarsMaskId, reflectMaskId, stretchMaskId, doubleExposureMaskId.

Fractal glass: glassEnabled true; glassWidth 1..15 (rib width); glassStrength 0..100 (refraction); glassAngle -90..90; glassCurve 0..100 with glassCurveWaves 1..10 (bent ribs); glassSoftness 0..50; glassHighlight 0..100; placement in percent of the photo: glassStart 0..95, glassBottom 5..100, glassLeft 0..95, glassRight 5..100.
Motion (movement blur, silky water, light trails): motionEnabled true; motionAngle -90..90; motionLength 1..20; motionThreshold 10..100 (only areas brighter than this streak); motionIntensity 0..100.
3D pop-out bars: popBarsEnabled true; popBarsWidth 1..25; popBarsInset 0..45.
Pixel stretch: stretchEnabled true; stretchPosition 0..100; stretchBandStart 0..100; stretchBandEnd 0..100.
Reflection: reflectEnabled true; reflectLine 5..95; reflectRipple 0..100; reflectFade 0..100.
Double exposure: doubleExposurePath (a second photo's file path); doubleExposureOpacity 0..100; doubleExposureScale 25..300; doubleExposureOffsetX and doubleExposureOffsetY -100..100; doubleExposureMode (blend mode, values as in get_state).
Lens blur: lensBlurEnabled true; lensBlurAmount 0..100; lensBlurDiffusion 0..100.
Light: glowAmount 0..100; halationAmount 0..100; flareAmount 0..100.
Frame: frameWidth 0..20 (0 is off); frameRadius 0..25; frameColor (format as in get_state).
Vignette: vignetteAmount -100..100; vignetteMidpoint 0..100; vignetteRoundness -100..100; vignetteFeather 0..100.
Grain: grainAmount 0..100; grainSize 0..100; grainRoughness 0..100.

Keep it subtle unless asked: capture_preview after each change and pull back if it looks heavy.";

#[tool_router]
impl StudioTools {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Current editor state: open image (path, size) and its full adjustments. Large bitmaps are omitted."
    )]
    async fn get_state(&self) -> Result<CallToolResult, McpError> {
        to_tool_result(call_frontend("get_state", json!({})).await)
    }

    #[tool(description = "List photos in the current library folder with rating and edited flag.")]
    async fn list_images(&self) -> Result<CallToolResult, McpError> {
        to_tool_result(call_frontend("list_images", json!({})).await)
    }

    #[tool(description = "Open a photo in the editor and wait until it is ready.")]
    async fn open_image(
        &self,
        Parameters(args): Parameters<OpenImageArgs>,
    ) -> Result<CallToolResult, McpError> {
        to_tool_result(call_frontend("open_image", json!({ "path": args.path })).await)
    }

    #[tool(
        description = "Change adjustments on the open photo, exactly like moving sliders: live in the UI, undoable, auto-saved."
    )]
    async fn set_adjustments(
        &self,
        Parameters(args): Parameters<SetAdjustmentsArgs>,
    ) -> Result<CallToolResult, McpError> {
        to_tool_result(call_frontend("set_adjustments", json!({ "patch": args.patch })).await)
    }

    #[tool(
        description = "List the creative effects (fractal glass, motion, pop-out bars, pixel stretch, reflection, double exposure, lens blur, glow, frame, grain, vignette) with their setting names and ranges, and how to limit one to a mask, including a freehand brush mask. Read this before adding an effect."
    )]
    async fn get_effects(&self) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![ContentBlock::text(EFFECTS_REFERENCE)]))
    }

    #[tool(
        description = "Create or fix a precise AI mask (like Lightroom's People and Landscape masks: sky, water, vegetation, mountains, buildings, person, face, hair, skin, clothes). 1) capture_mask without mask_id to see the photo. 2) create_smart_mask with one region per separate area: include points spread over it, exclude points on neighbours that must stay out, optional box. 3) capture_mask with the returned mask_id: red = selected. 4) Fix until it matches: call again with mask_id to add a missed area or subtract spill, refine_mask to limit by color/brightness, or undo and retry. Returns the mask id."
    )]
    async fn create_smart_mask(
        &self,
        Parameters(args): Parameters<SmartMaskArgs>,
    ) -> Result<CallToolResult, McpError> {
        to_tool_result(
            call_frontend(
                "create_smart_mask",
                json!({
                    "name": args.name,
                    "regions": args.regions,
                    "maskId": args.mask_id,
                    "invert": args.invert.unwrap_or(false),
                }),
            )
            .await,
        )
    }

    #[tool(
        description = "Limit a mask to pixels whose color (or brightness) is close to a sampled point, e.g. only the blue of the sky or only skin tones inside a person mask. Check with capture_mask afterwards."
    )]
    async fn refine_mask(
        &self,
        Parameters(args): Parameters<RefineMaskArgs>,
    ) -> Result<CallToolResult, McpError> {
        to_tool_result(
            call_frontend(
                "refine_mask",
                json!({
                    "maskId": args.mask_id,
                    "by": args.by,
                    "point": args.point,
                    "tolerance": args.tolerance,
                }),
            )
            .await,
        )
    }

    #[tool(
        description = "Edit one mask: deep-merge its adjustments (exposure, contrast, saturation, temperature...), set name, invert, opacity, visible, and set the edge of its AI parts with {\"edges\": {\"refine\": 0-100, \"contrast\": 0-100, \"grow\", \"feather\"}}. Raise refine when capture_mask shows a soft or haloed edge around hair, branches or rooftops."
    )]
    async fn update_mask(
        &self,
        Parameters(args): Parameters<UpdateMaskArgs>,
    ) -> Result<CallToolResult, McpError> {
        to_tool_result(
            call_frontend(
                "update_mask",
                json!({ "maskId": args.mask_id, "patch": args.patch }),
            )
            .await,
        )
    }

    #[tool(description = "Undo the last edit step.")]
    async fn undo(&self) -> Result<CallToolResult, McpError> {
        to_tool_result(call_frontend("undo", json!({})).await)
    }

    #[tool(description = "Redo the last undone edit step.")]
    async fn redo(&self) -> Result<CallToolResult, McpError> {
        to_tool_result(call_frontend("redo", json!({})).await)
    }

    #[tool(
        description = "Measure the open photo instead of looking at it: brightness, what is crushed to black or blown out, and with mask_id how much of the frame that mask covers and where it sits. Costs a fraction of a picture, so check an edit with this first; capture_preview only when the look itself is the question, and capture_mask when a mask's shape needs your eyes."
    )]
    async fn measure(&self, Parameters(args): Parameters<MeasureArgs>) -> Result<CallToolResult, McpError> {
        let input = match call_frontend("get_render_input", json!({})).await {
            Ok(v) => v,
            Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        };
        let app = match BRIDGE.get().map(|b| b.app.clone()) {
            Some(app) => app,
            None => return Err(McpError::internal_error("bridge not started", None)),
        };
        let measured = tokio::task::spawn_blocking(move || crate::measure_photo(&input, &app, args.mask_id.as_deref()))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);
        Ok(match measured {
            Ok(value) => CallToolResult::success(vec![ContentBlock::text(
                serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()),
            )]),
            Err(e) => CallToolResult::error(vec![ContentBlock::text(format!("Could not measure: {e}"))]),
        })
    }

    #[tool(
        description = "Render the open photo with its current edit (including any frame) and return it as a JPEG, so you can see the result."
    )]
    async fn capture_preview(
        &self,
        Parameters(args): Parameters<PreviewArgs>,
    ) -> Result<CallToolResult, McpError> {
        render_view(args.max_edge, true, None).await
    }

    #[tool(
        description = "See a mask: the photo (without frame; this is the frame create_smart_mask and refine_mask coordinates refer to) with the mask tinted red. Omit mask_id for the plain photo to pick points on. Check every new or changed mask with this before editing inside it."
    )]
    async fn capture_mask(
        &self,
        Parameters(args): Parameters<MaskViewArgs>,
    ) -> Result<CallToolResult, McpError> {
        render_view(args.max_edge, false, args.mask_id).await
    }

    #[tool(
        description = "See the reference photo the user picked for you to match: its colours, light and any creative effect. Compare it with capture_preview of the photo being edited."
    )]
    async fn capture_reference(
        &self,
        Parameters(args): Parameters<ReferenceArgs>,
    ) -> Result<CallToolResult, McpError> {
        let input = match call_frontend("get_reference", json!({})).await {
            Ok(v) => v,
            Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
        };
        let path = input["path"].as_str().unwrap_or_default().to_string();
        // The reference is shown exactly as it is: no edits, no frame.
        render_path(path, json!({}), args.max_edge, false, None).await
    }

    #[tool(
        description = "Cull photos the way the Culling window does: group bursts (shot within seconds and alike), pick each burst's best frame by focus, flag blurry frames and frames where someone's eyes are closed. Suggests only -- nothing is rated or deleted."
    )]
    async fn cull_photos(&self, Parameters(args): Parameters<CullArgs>) -> Result<CallToolResult, McpError> {
        let app = BRIDGE
            .get()
            .map(|b| b.app.clone())
            .ok_or_else(|| McpError::internal_error("bridge not started", None))?;
        let settings = crate::culling::CullingSettings {
            similarity_threshold: args.similarity_threshold.unwrap_or(28),
            blur_threshold: args.blur_threshold.unwrap_or(100.0),
            group_similar: true,
            filter_blurry: true,
            filter_closed_eyes: args.find_closed_eyes.unwrap_or(true),
        };
        let result = crate::culling::cull_images(args.paths, settings, app.clone(), app.state())
            .await
            .map(|r| serde_json::to_value(r).unwrap_or_default());
        to_tool_result(result)
    }

    #[tool(
        description = "Upscale a finished (exported) photo into a new file next to it, named <name>_upscaled_<method>.jpg. Returns the new path, size and time taken."
    )]
    async fn upscale_photo(
        &self,
        Parameters(args): Parameters<UpscaleArgs>,
    ) -> Result<CallToolResult, McpError> {
        let app = BRIDGE
            .get()
            .map(|b| b.app.clone())
            .ok_or_else(|| McpError::internal_error("bridge not started", None))?;
        let result = crate::upscale::upscale_photo_inner(&app, args.path, args.method, args.long_edge.unwrap_or(3840))
            .await
            .map(|r| serde_json::to_value(r).unwrap_or_default());
        to_tool_result(result)
    }

    #[tool(
        description = "See any image file as it is on disk, optionally a crop, e.g. to compare the detail of two upscaled versions at full resolution."
    )]
    async fn view_image(
        &self,
        Parameters(args): Parameters<ViewImageArgs>,
    ) -> Result<CallToolResult, McpError> {
        let max_edge = args.max_edge.unwrap_or(1024);
        let rendered = tokio::task::spawn_blocking(move || crate::upscale::view_image_jpeg(&args.path, args.crop, max_edge))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);
        Ok(match rendered {
            Ok(bytes) => {
                use base64::Engine;
                let data = base64::engine::general_purpose::STANDARD.encode(bytes);
                CallToolResult::success(vec![ContentBlock::image(data, "image/jpeg")])
            }
            Err(e) => CallToolResult::error(vec![ContentBlock::text(e)]),
        })
    }

    #[tool(
        description = "Save the open photo's current edit as a named version, so the user can compare it with other versions or roll back."
    )]
    async fn save_version(
        &self,
        Parameters(args): Parameters<SaveVersionArgs>,
    ) -> Result<CallToolResult, McpError> {
        to_tool_result(call_frontend("save_version", json!({ "name": args.name })).await)
    }
}

/// Render the open photo in the backend (optionally framed, optionally with a mask tinted red).
async fn render_view(
    max_edge: Option<u32>,
    framed: bool,
    mask_id: Option<String>,
) -> Result<CallToolResult, McpError> {
    let input = match call_frontend("get_render_input", json!({})).await {
        Ok(v) => v,
        Err(e) => return Ok(CallToolResult::error(vec![ContentBlock::text(e)])),
    };
    let path = input["path"].as_str().unwrap_or_default().to_string();
    render_path(path, input["adjustments"].clone(), max_edge, framed, mask_id).await
}

/// Render any photo with the given adjustments and hand it back as an image block.
async fn render_path(
    path: String,
    adjustments: Value,
    max_edge: Option<u32>,
    framed: bool,
    mask_id: Option<String>,
) -> Result<CallToolResult, McpError> {
    let app = BRIDGE
        .get()
        .map(|b| b.app.clone())
        .ok_or_else(|| McpError::internal_error("bridge not started", None))?;
    // A mask is judged by its edge, which reads fine smaller; the photo itself is judged on its tones, and
    // every one of these images stays in the conversation and is re-read on each later step.
    let max_edge = max_edge.unwrap_or(if mask_id.is_some() { 768 } else { 1024 });

    let rendered = tokio::task::spawn_blocking(move || {
        crate::render_preview_jpeg(
            &path,
            &adjustments,
            &app,
            Some(max_edge),
            framed,
            mask_id.as_deref(),
        )
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);

    Ok(match rendered {
        Ok(bytes) => {
            use base64::Engine;
            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
            CallToolResult::success(vec![ContentBlock::image(data, "image/jpeg")])
        }
        Err(e) => CallToolResult::error(vec![ContentBlock::text(format!("Render failed: {e}"))]),
    })
}

#[tool_handler]
impl ServerHandler for StudioTools {
    fn get_info(&self) -> ServerInfo {
        // InitializeResult is #[non_exhaustive]: use the builder, not a struct literal.
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Controls the live RapidRAW Studio editor. Real photos only: use parametric adjustments and masks, never generative tools. Call get_state before editing and capture_preview to check results. Call get_effects before adding a creative effect.",
        )
    }
}

/// What the photo looks like as numbers: the questions an assistant would otherwise squint at a preview to
/// answer -- is anything crushed to black, has anything blown out, is it generally dark or bright.
/// ponytail: luma from the rendered preview, which is small; exact enough to judge an edit by.
pub(crate) fn photo_stats(image: &image::RgbImage) -> Value {
    let mut histogram = [0u32; 256];
    for p in image.pixels() {
        let luma = (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32).round();
        histogram[(luma as usize).min(255)] += 1;
    }
    let total: u32 = histogram.iter().sum();
    if total == 0 {
        return json!({});
    }
    let percentile = |wanted: f32| -> u8 {
        let target = (total as f32 * wanted).round() as u32;
        let mut seen = 0u32;
        for (value, count) in histogram.iter().enumerate() {
            seen += count;
            if seen >= target.max(1) {
                return value as u8;
            }
        }
        255
    };
    // Rounded as f64: an f32 tenth prints as 3.299999952316284, which is noise in the answer and tokens spent.
    let share = |from: usize, to: usize| {
        let count: u32 = histogram[from..=to].iter().sum();
        (count as f64 * 1000.0 / total as f64).round() / 10.0
    };
    let mean = histogram
        .iter()
        .enumerate()
        .map(|(value, count)| value as f64 * *count as f64)
        .sum::<f64>()
        / total as f64;

    json!({
        "meanBrightness": (mean).round() as u8,
        "darkest2Percent": percentile(0.02),
        "median": percentile(0.5),
        "brightest2Percent": percentile(0.98),
        "crushedToBlackPercent": share(0, 2),
        "blownOutPercent": share(253, 255),
        "inShadowPercent": share(0, 63),
        "inHighlightPercent": share(192, 255)
    })
}

/// Where a mask actually falls: how much of the frame it takes and the box it sits in, so "does this cover the
/// thing I circled" is answered by numbers rather than by looking at a red tint.
pub(crate) fn mask_stats(mask: &image::GrayImage) -> Value {
    let (w, h) = mask.dimensions();
    let (mut covered, mut soft) = (0u32, 0u32);
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (w, h, 0u32, 0u32);
    for (x, y, p) in mask.enumerate_pixels() {
        if p[0] > 127 {
            covered += 1;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        } else if p[0] > 8 {
            soft += 1;
        }
    }
    let total = (w * h).max(1);
    if covered == 0 {
        return json!({ "coversPercent": 0.0, "empty": true });
    }
    let pct = |n: u32| (n as f64 * 1000.0 / total as f64).round() / 10.0;
    let unit = |n: u32, of: u32| (n as f64 * 1000.0 / of as f64).round() / 1000.0;
    json!({
        "coversPercent": pct(covered),
        "softEdgePercent": pct(soft),
        "box": [unit(min_x, w), unit(min_y, h), unit(max_x, w), unit(max_y, h)],
    })
}

/// Start the local MCP server. Loopback only; the port is fixed so Claude Code config stays stable.
pub fn start(app: AppHandle) {
    if BRIDGE
        .set(Bridge {
            app,
            pending: Mutex::new(HashMap::new()),
        })
        .is_err()
    {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let service = StreamableHttpService::new(
            || Ok(StudioTools::new()),
            Arc::new(LocalSessionManager::default()),
            StreamableHttpServerConfig::default(),
        );
        let router = axum::Router::new().nest_service("/mcp", service);
        // A restart can leave the previous process holding the port while it shuts down (wgpu and the AI
        // runtime take their time), so wait a few seconds for it. If the port is unavailable for good --
        // Hyper-V and WSL reserve whole bands of ports on Windows -- take any free one instead: a bridge
        // that never binds leaves the assistant with no editor tools at all.
        let mut bound = None;
        let mut last_error = None;
        for _ in 0..12 {
            match tokio::net::TcpListener::bind(BRIDGE_ADDR).await {
                Ok(listener) => {
                    bound = Some(listener);
                    break;
                }
                Err(e) => {
                    last_error = Some(e);
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
            }
        }
        if bound.is_none() {
            log::warn!(
                "Studio bridge could not use {BRIDGE_ADDR} ({}); taking a free port instead.",
                last_error.map_or_else(|| "unknown error".to_string(), |e| e.to_string())
            );
            match tokio::net::TcpListener::bind("127.0.0.1:0").await {
                Ok(listener) => bound = Some(listener),
                Err(e) => {
                    log::error!("Studio bridge could not listen at all, so the assistant has no editor tools: {e}");
                    return;
                }
            }
        }
        let listener = bound.expect("bridge listener");
        let addr = listener
            .local_addr()
            .map_or_else(|_| BRIDGE_ADDR.to_string(), |a| a.to_string());
        let _ = BRIDGE_URL.set(format!("http://{addr}/mcp"));
        log::info!("Studio bridge listening on {addr}");
        if let Err(e) = axum::serve(listener, router).await {
            log::error!("Studio bridge stopped: {e}");
        }
    });
}

#[cfg(test)]
mod measure_tests {
    use super::{mask_stats, photo_stats};

    #[test]
    fn photo_stats_name_what_an_eye_would_look_for() {
        // A photo with a quarter crushed to black, a quarter blown out, and half mid-grey.
        let image = image::RgbImage::from_fn(100, 100, |_, y| match y {
            0..=24 => image::Rgb([0, 0, 0]),
            25..=49 => image::Rgb([255, 255, 255]),
            _ => image::Rgb([128, 128, 128]),
        });
        let stats = photo_stats(&image);
        assert_eq!(stats["crushedToBlackPercent"], 25.0);
        assert_eq!(stats["blownOutPercent"], 25.0);
        assert_eq!(stats["median"], 128);
        assert_eq!(stats["inShadowPercent"], 25.0);

        // Nothing is crushed or blown in an evenly mid-grey photo.
        let flat = image::RgbImage::from_pixel(10, 10, image::Rgb([128, 128, 128]));
        let stats = photo_stats(&flat);
        assert_eq!(stats["crushedToBlackPercent"], 0.0);
        assert_eq!(stats["blownOutPercent"], 0.0);
        assert_eq!(stats["meanBrightness"], 128);
    }

    #[test]
    fn mask_stats_say_how_much_is_covered_and_where() {
        // A mask over the top-left quarter, with a band of half-covered pixels along its edge.
        let mask = image::GrayImage::from_fn(100, 100, |x, y| {
            if x < 50 && y < 50 {
                image::Luma([255])
            } else if x < 55 && y < 55 {
                image::Luma([90])
            } else {
                image::Luma([0])
            }
        });
        let stats = mask_stats(&mask);
        assert_eq!(stats["coversPercent"], 25.0);
        assert!(stats["softEdgePercent"].as_f64().unwrap() > 0.0, "the feathered band is reported");
        let boxed = stats["box"].as_array().unwrap();
        assert_eq!(boxed[0], 0.0);
        assert_eq!(boxed[1], 0.0);
        assert!((boxed[2].as_f64().unwrap() - 0.49).abs() < 0.02, "right edge near half way");

        // An empty mask says so rather than reporting a box around nothing.
        let empty = image::GrayImage::from_pixel(10, 10, image::Luma([0]));
        assert_eq!(mask_stats(&empty)["empty"], true);
        assert_eq!(mask_stats(&empty)["coversPercent"], 0.0);
    }
}
