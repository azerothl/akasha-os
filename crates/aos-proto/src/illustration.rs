//! Illustration surface — semantic scene + look/palette (skill-inspired), separate from whiteboard ops.

use serde::{Deserialize, Serialize};

use crate::CanvasPoint;

/// Hand-drawn look (one finish per still / shot).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum IllustrationLook {
    #[default]
    Ink,
    Riso,
    Screen,
    Pencil,
    Blueprint,
}

impl IllustrationLook {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ink => "ink",
            Self::Riso => "riso",
            Self::Screen => "screen",
            Self::Pencil => "pencil",
            Self::Blueprint => "blueprint",
        }
    }

    pub fn default_palette(self) -> IllustrationPaletteId {
        match self {
            Self::Ink => IllustrationPaletteId::PaperInk,
            Self::Riso => IllustrationPaletteId::RisoPop,
            Self::Screen => IllustrationPaletteId::ScreenSea,
            Self::Pencil => IllustrationPaletteId::PencilMinimal,
            Self::Blueprint => IllustrationPaletteId::BlueprintNight,
        }
    }

    pub fn finish(self) -> IllustrationFinish {
        match self {
            Self::Ink | Self::Blueprint => IllustrationFinish::Ink,
            Self::Riso => IllustrationFinish::Riso,
            Self::Screen => IllustrationFinish::Screen,
            Self::Pencil => IllustrationFinish::Pencil,
        }
    }
}

/// Named palette presets (no free hex in scenes).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum IllustrationPaletteId {
    #[default]
    PaperInk,
    RisoPop,
    ScreenSea,
    PencilMinimal,
    BlueprintNight,
}

impl IllustrationPaletteId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PaperInk => "paperInk",
            Self::RisoPop => "risoPop",
            Self::ScreenSea => "screenSea",
            Self::PencilMinimal => "pencilMinimal",
            Self::BlueprintNight => "blueprintNight",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "paperInk" | "paper_ink" => Some(Self::PaperInk),
            "risoPop" | "riso_pop" => Some(Self::RisoPop),
            "screenSea" | "screen_sea" => Some(Self::ScreenSea),
            "pencilMinimal" | "pencil_minimal" => Some(Self::PencilMinimal),
            "blueprintNight" | "blueprint_night" => Some(Self::BlueprintNight),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum IllustrationFinish {
    #[default]
    Ink,
    Riso,
    Screen,
    Pencil,
    Flat,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum IllustrationRenderMode {
    #[default]
    Normal,
    Blueprint,
}

/// Resolved palette colours (engine-side; agents pick by id).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IllustrationPaletteColors {
    pub paper: String,
    pub ink: String,
    pub night: String,
    pub chalk: String,
    pub chalk_dim: String,
    pub guide: String,
    pub fills: Vec<String>,
    pub shade: String,
    pub light: String,
    pub blush: String,
    pub accents: Vec<String>,
    pub finish: IllustrationFinish,
}

impl IllustrationPaletteColors {
    pub fn for_preset(id: IllustrationPaletteId) -> Self {
        match id {
            IllustrationPaletteId::PaperInk => Self {
                paper: "#f3e6cf".into(),
                ink: "#1e1630".into(),
                night: "#0b0d1f".into(),
                chalk: "#e8ecff".into(),
                chalk_dim: "#8d97c9".into(),
                guide: "#4664ff88".into(),
                fills: vec![
                    "#e79256".into(),
                    "#c99a5a".into(),
                    "#b8864e".into(),
                    "#d9b078".into(),
                ],
                shade: "#3a2214".into(),
                light: "#fff1d6".into(),
                blush: "#c8473f".into(),
                accents: vec![
                    "#ff2bd6".into(),
                    "#28f0e0".into(),
                    "#ffe22b".into(),
                    "#5cff5c".into(),
                ],
                finish: IllustrationFinish::Ink,
            },
            IllustrationPaletteId::RisoPop => Self {
                paper: "#f0ece2".into(),
                ink: "#22366b".into(),
                night: "#2a2050".into(),
                chalk: "#f3ebb1".into(),
                chalk_dim: "#8f86c8".into(),
                guide: "#22366b80".into(),
                fills: vec![
                    "#ff48b0".into(),
                    "#0078bf".into(),
                    "#ffe800".into(),
                    "#00a95c".into(),
                    "#ff6c2f".into(),
                    "#765ba7".into(),
                ],
                shade: "#22366b".into(),
                light: "#fff9c8".into(),
                blush: "#ff48b0".into(),
                accents: vec![
                    "#ff48b0".into(),
                    "#0078bf".into(),
                    "#ffe800".into(),
                    "#00a95c".into(),
                ],
                finish: IllustrationFinish::Riso,
            },
            IllustrationPaletteId::ScreenSea => Self {
                paper: "#e8e6db".into(),
                ink: "#1f1e2d".into(),
                night: "#1a1c2e".into(),
                chalk: "#e8e6db".into(),
                chalk_dim: "#8a93a6".into(),
                guide: "#0a508380".into(),
                fills: vec![
                    "#0a5083".into(),
                    "#518e9d".into(),
                    "#becacc".into(),
                    "#e4a05c".into(),
                    "#91906a".into(),
                    "#e8c84a".into(),
                    "#c8473f".into(),
                    "#051630".into(),
                ],
                shade: "#051630".into(),
                light: "#f4f2e8".into(),
                blush: "#e4a05c".into(),
                accents: vec![
                    "#c8473f".into(),
                    "#e8c84a".into(),
                    "#518e9d".into(),
                    "#f0a0b0".into(),
                ],
                finish: IllustrationFinish::Screen,
            },
            IllustrationPaletteId::PencilMinimal => Self {
                paper: "#f4efe4".into(),
                ink: "#201f1b".into(),
                night: "#27251f".into(),
                chalk: "#d9d2c2".into(),
                chalk_dim: "#7d786c".into(),
                guide: "#201f1b59".into(),
                fills: vec![
                    "#e8d6cc".into(),
                    "#e0e2d0".into(),
                    "#dad2c5".into(),
                    "#f4efe4".into(),
                ],
                shade: "#5e5a50".into(),
                light: "#ffffff".into(),
                blush: "#c9a9a0".into(),
                accents: vec![
                    "#8a8a55".into(),
                    "#b0483a".into(),
                    "#7e8aa0".into(),
                    "#c9a15a".into(),
                ],
                finish: IllustrationFinish::Pencil,
            },
            IllustrationPaletteId::BlueprintNight => Self {
                paper: "#0b0d1f".into(),
                ink: "#e8ecff".into(),
                night: "#0b0d1f".into(),
                chalk: "#e8ecff".into(),
                chalk_dim: "#8d97c9".into(),
                guide: "#96aaffb3".into(),
                fills: vec![
                    "#1a2040".into(),
                    "#22306a".into(),
                    "#2c3a80".into(),
                    "#141a33".into(),
                ],
                shade: "#8d97c9".into(),
                light: "#ffffff".into(),
                blush: "#7fe7ff".into(),
                accents: vec![
                    "#7fe7ff".into(),
                    "#ff6fd8".into(),
                    "#ffe22b".into(),
                    "#5fe08a".into(),
                ],
                finish: IllustrationFinish::Ink,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IllustrationBrief {
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub look: IllustrationLook,
    #[serde(default)]
    pub palette: IllustrationPaletteId,
    /// Element that survives cuts / remains the visual anchor.
    #[serde(default)]
    pub anchor: String,
    #[serde(default)]
    pub beats: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IllustrationPartGeometry {
    Ellipse {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        #[serde(default)]
        rotation: f32,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        #[serde(default)]
        rotation: f32,
    },
    Path {
        points: Vec<CanvasPoint>,
        #[serde(default = "default_true")]
        closed: bool,
    },
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IllustrationPart {
    pub id: String,
    #[serde(default)]
    pub role: String,
    /// Index into palette fills (clamped).
    #[serde(default)]
    pub fill_index: u8,
    #[serde(default = "default_true")]
    pub fill: bool,
    #[serde(default)]
    pub outline: bool,
    #[serde(default)]
    pub seed: u32,
    pub geometry: IllustrationPartGeometry,
}

impl Default for IllustrationPart {
    fn default() -> Self {
        Self {
            id: String::new(),
            role: String::new(),
            fill_index: 0,
            fill: true,
            outline: true,
            seed: 1,
            geometry: IllustrationPartGeometry::Ellipse {
                x: 0.3,
                y: 0.3,
                w: 0.4,
                h: 0.4,
                rotation: 0.0,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IllustrationCamera {
    /// World point placed at frame centre (normalized 0..1).
    #[serde(default = "default_cam_center")]
    pub x: f32,
    #[serde(default = "default_cam_center")]
    pub y: f32,
    #[serde(default = "default_cam_zoom")]
    pub zoom: f32,
    #[serde(default)]
    pub rot: f32,
}

fn default_cam_center() -> f32 {
    0.5
}

fn default_cam_zoom() -> f32 {
    1.0
}

impl Default for IllustrationCamera {
    fn default() -> Self {
        Self {
            x: 0.5,
            y: 0.5,
            zoom: 1.0,
            rot: 0.0,
        }
    }
}

/// Pose knobs (3..6 numbers); unused keys stay 0.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IllustrationPose {
    #[serde(default)]
    pub walk: f32,
    #[serde(default)]
    pub twitch: f32,
    #[serde(default)]
    pub wing: f32,
    #[serde(default)]
    pub flap: f32,
    #[serde(default)]
    pub tuck: f32,
    #[serde(default)]
    pub tilt: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IllustrationSpec {
    #[serde(default = "default_illust_version")]
    pub version: u32,
    #[serde(default)]
    pub brief: IllustrationBrief,
    #[serde(default)]
    pub parts: Vec<IllustrationPart>,
    #[serde(default)]
    pub pose: IllustrationPose,
    #[serde(default)]
    pub camera: IllustrationCamera,
    #[serde(default)]
    pub mode: IllustrationRenderMode,
    /// Draw construction guides around the subject.
    #[serde(default)]
    pub show_construction: bool,
    /// Scribble accent on at most one part id.
    #[serde(default)]
    pub scribble_part: Option<String>,
}

fn default_illust_version() -> u32 {
    1
}

impl Default for IllustrationSpec {
    fn default() -> Self {
        Self {
            version: default_illust_version(),
            brief: IllustrationBrief::default(),
            parts: Vec::new(),
            pose: IllustrationPose::default(),
            camera: IllustrationCamera::default(),
            mode: IllustrationRenderMode::Normal,
            show_construction: false,
            scribble_part: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IllustrationTimelineBeat {
    pub name: String,
    #[serde(default = "default_beat_dur")]
    pub dur_s: f32,
    #[serde(default)]
    pub pose: IllustrationPose,
    #[serde(default)]
    pub camera: Option<IllustrationCamera>,
    #[serde(default)]
    pub mode: Option<IllustrationRenderMode>,
}

fn default_beat_dur() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IllustrationTimeline {
    #[serde(default)]
    pub beats: Vec<IllustrationTimelineBeat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IllustrationLock {
    pub holder: String,
    pub expires_ms: u64,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IllustrationDoc {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub brief: IllustrationBrief,
    #[serde(default)]
    pub spec: Option<IllustrationSpec>,
    #[serde(default)]
    pub timeline: IllustrationTimeline,
    #[serde(default)]
    pub lock: Option<IllustrationLock>,
    /// Last rendered still path under /downloads.
    #[serde(default)]
    pub last_png: Option<String>,
    #[serde(default)]
    pub last_sheet_png: Option<String>,
    #[serde(default)]
    pub last_mp4: Option<String>,
    #[serde(default)]
    pub revision: u64,
}

// --- IPC requests / responses ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustSetOpenRequest {
    pub session_id: String,
    pub open: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustGetRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustGetResponse {
    pub session_id: String,
    pub illustration_open: bool,
    pub doc: IllustrationDoc,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustLockAcquireRequest {
    pub session_id: String,
    pub holder: String,
    #[serde(default)]
    pub reason: String,
    /// Lock TTL milliseconds (default 120_000).
    #[serde(default)]
    pub ttl_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustLockReleaseRequest {
    pub session_id: String,
    pub holder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustLockStatusRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustLockStatusResponse {
    pub session_id: String,
    pub lock: Option<IllustrationLock>,
    pub locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustSetBriefRequest {
    pub session_id: String,
    pub holder: String,
    pub brief: IllustrationBrief,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustComposeRequest {
    pub session_id: String,
    pub holder: String,
    pub spec: IllustrationSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustComposeResponse {
    pub doc: IllustrationDoc,
    pub illustration_open: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustRenderSheetRequest {
    pub session_id: String,
    pub holder: String,
    #[serde(default)]
    pub width: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustExportRequest {
    pub session_id: String,
    pub holder: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    /// `png` (default) or `json`.
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustReviewRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustReviewIssue {
    pub kind: String,
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustReviewResponse {
    pub score: f32,
    pub issues: Vec<IllustReviewIssue>,
    pub checklist: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustAnimateRequest {
    pub session_id: String,
    pub holder: String,
    #[serde(default)]
    pub timeline: Option<IllustrationTimeline>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustAnimateResponse {
    pub doc: IllustrationDoc,
    pub frames_dir: String,
    pub mp4_path: Option<String>,
    pub contact_sheet: Option<String>,
    pub drawn_frames: u32,
    pub message: String,
}

/// Deterministic local review (no vision required).
pub fn review_illustration(doc: &IllustrationDoc) -> IllustReviewResponse {
    let mut issues = Vec::new();
    if doc.brief.subject.trim().is_empty() {
        issues.push(IllustReviewIssue {
            kind: "missing_brief".into(),
            severity: "error".into(),
            message: "Brief subject is empty — set_brief before compose.".into(),
        });
    }
    let Some(spec) = doc.spec.as_ref() else {
        issues.push(IllustReviewIssue {
            kind: "empty_scene".into(),
            severity: "error".into(),
            message: "No illustration spec composed yet.".into(),
        });
        return IllustReviewResponse {
            score: 0.0,
            checklist: format_checklist(&issues),
            issues,
        };
    };
    if spec.parts.is_empty() {
        issues.push(IllustReviewIssue {
            kind: "empty_parts".into(),
            severity: "error".into(),
            message: "Spec has no parts — silhouette cannot read.".into(),
        });
    }
    if spec.parts.len() > 12 {
        issues.push(IllustReviewIssue {
            kind: "too_many_parts".into(),
            severity: "warning".into(),
            message: "More than 12 parts; prefer 3–8 for a readable silhouette.".into(),
        });
    }
    if doc.brief.anchor.trim().is_empty() {
        issues.push(IllustReviewIssue {
            kind: "missing_anchor".into(),
            severity: "warning".into(),
            message: "No anchor named in the brief.".into(),
        });
    }
    let has_body = spec
        .parts
        .iter()
        .any(|p| matches!(p.role.as_str(), "body" | "masse" | "main" | "subject"));
    if !spec.parts.is_empty() && !has_body {
        issues.push(IllustReviewIssue {
            kind: "no_main_mass".into(),
            severity: "warning".into(),
            message: "No part with role body/main/subject — mark the primary mass.".into(),
        });
    }
    let errors = issues.iter().filter(|i| i.severity == "error").count();
    let warnings = issues.iter().filter(|i| i.severity == "warning").count();
    let score = (1.0 - errors as f32 * 0.35 - warnings as f32 * 0.1).clamp(0.0, 1.0);
    IllustReviewResponse {
        score,
        checklist: format_checklist(&issues),
        issues,
    }
}

fn format_checklist(issues: &[IllustReviewIssue]) -> String {
    if issues.is_empty() {
        return "ok: paper+finish applied by raster; silhouette parts present; brief set.".into();
    }
    issues
        .iter()
        .map(|i| format!("{} [{}] {}", i.kind, i.severity, i.message))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn illustration_digest(doc: &IllustrationDoc) -> String {
    let look = doc.brief.look.as_str();
    let pal = doc.brief.palette.as_str();
    let parts = doc
        .spec
        .as_ref()
        .map(|s| s.parts.len())
        .unwrap_or(0);
    let lock = doc
        .lock
        .as_ref()
        .map(|l| l.holder.as_str())
        .unwrap_or("-");
    format!(
        "illust rev={} look={} palette={} subject={:?} parts={} lock={} png={} sheet={}",
        doc.revision,
        look,
        pal,
        doc.brief.subject,
        parts,
        lock,
        doc.last_png.as_deref().unwrap_or("-"),
        doc.last_sheet_png.as_deref().unwrap_or("-"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_presets_resolve() {
        let p = IllustrationPaletteColors::for_preset(IllustrationPaletteId::PaperInk);
        assert_eq!(p.paper, "#f3e6cf");
        assert_eq!(p.finish, IllustrationFinish::Ink);
    }

    #[test]
    fn review_flags_empty() {
        let r = review_illustration(&IllustrationDoc::default());
        assert!(r.score < 0.5);
        assert!(r.issues.iter().any(|i| i.kind == "missing_brief"));
    }
}
