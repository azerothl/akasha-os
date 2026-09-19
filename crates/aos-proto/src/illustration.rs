//! Illustration surface — semantic scene + look/palette (skill-inspired), separate from whiteboard ops.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::CanvasPoint;

/// Hand-drawn look (one finish per still / shot).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ink" | "encre" => Some(Self::Ink),
            "riso" => Some(Self::Riso),
            "screen" | "serigraphie" | "sérigraphie" => Some(Self::Screen),
            "pencil" | "crayon" => Some(Self::Pencil),
            "blueprint" | "plan" => Some(Self::Blueprint),
            _ => None,
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

impl Serialize for IllustrationLook {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for IllustrationLook {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Self::parse(&s).unwrap_or_default())
    }
}

/// Named palette presets (no free hex in scenes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
            "paperInk" | "paper_ink" | "ink" => Some(Self::PaperInk),
            "risoPop" | "riso_pop" | "riso" => Some(Self::RisoPop),
            "screenSea" | "screen_sea" | "screen" => Some(Self::ScreenSea),
            "pencilMinimal" | "pencil_minimal" | "pencil" => Some(Self::PencilMinimal),
            "blueprintNight" | "blueprint_night" | "blueprint" | "night" => {
                Some(Self::BlueprintNight)
            }
            _ => None,
        }
    }

    /// Unknown agent names fall back to the look's default preset.
    pub fn parse_or(s: &str, look: IllustrationLook) -> Self {
        Self::parse(s).unwrap_or_else(|| look.default_palette())
    }
}

impl Serialize for IllustrationPaletteId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for IllustrationPaletteId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Self::parse_or(&s, IllustrationLook::Ink))
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
    /// Print inks for riso multi-plate separations (skill `inks[]`).
    pub inks: Vec<String>,
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
                inks: vec!["#1e1630".into(), "#c8473f".into(), "#2b5fb8".into()],
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
                inks: vec![
                    "#0078bf".into(),
                    "#ff48b0".into(),
                    "#ffe800".into(),
                    "#22366b".into(),
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
                inks: vec!["#0a5083".into(), "#051630".into(), "#e8c84a".into()],
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
                inks: vec!["#201f1b".into(), "#8a8a55".into()],
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
                inks: vec!["#e8ecff".into(), "#7fe7ff".into()],
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
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub role: String,
    /// Index into palette fills (clamped).
    #[serde(default)]
    pub fill_index: u8,
    #[serde(default = "default_true")]
    pub fill: bool,
    #[serde(default = "default_true")]
    pub outline: bool,
    #[serde(default)]
    pub seed: u32,
    #[serde(default = "default_part_geometry")]
    pub geometry: IllustrationPartGeometry,
}

fn default_part_geometry() -> IllustrationPartGeometry {
    IllustrationPartGeometry::Ellipse {
        x: 0.3,
        y: 0.3,
        w: 0.4,
        h: 0.4,
        rotation: 0.0,
    }
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
    let subject_parts = spec
        .parts
        .iter()
        .filter(|p| !is_background_part(p))
        .count();
    if !spec.parts.is_empty() && subject_parts < 5 {
        issues.push(IllustReviewIssue {
            kind: "sparse_puppet".into(),
            severity: "warning".into(),
            message: "Fewer than 5 subject parts — enrich to head/body/ears/eyes (skill puppet recipe)."
                .into(),
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
    let has_body = spec.parts.iter().any(|p| {
        let id = p.id.to_ascii_lowercase();
        matches!(p.role.as_str(), "body" | "masse" | "main" | "subject")
            || id.contains("body")
            || id.contains("corps")
    });
    let has_head = spec.parts.iter().any(|p| {
        let id = p.id.to_ascii_lowercase();
        id.contains("head") || id.contains("tete") || id.contains("tête") || p.role == "head"
    });
    if !spec.parts.is_empty() && !has_body {
        issues.push(IllustReviewIssue {
            kind: "no_main_mass".into(),
            severity: "warning".into(),
            message: "No part with role body/main/subject — mark the primary mass.".into(),
        });
    }
    if subject_parts >= 3 && !has_head {
        issues.push(IllustReviewIssue {
            kind: "no_head".into(),
            severity: "warning".into(),
            message: "No head part — add overlapping head/ears/eyes for a readable silhouette."
                .into(),
        });
    }
    // Skill rule 12: silhouette must read on a contact-sheet cell (~240px).
    if doc.last_sheet_png.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true) {
        issues.push(IllustReviewIssue {
            kind: "missing_contact_sheet".into(),
            severity: "error".into(),
            message: "Call illust.render_sheet before export — skill requires a contact-sheet readability check."
                .into(),
        });
    }
    let subject_l = format!(
        "{} {}",
        doc.brief.subject.to_ascii_lowercase(),
        doc.brief.beats.join(" ").to_ascii_lowercase()
    );
    let wants_stretch = subject_l.contains("étire")
        || subject_l.contains("etire")
        || subject_l.contains("stretch")
        || subject_l.contains("allong");
    if wants_stretch {
        if let Some(body) = spec.parts.iter().find(|p| {
            let id = p.id.to_ascii_lowercase();
            id.contains("body") || id.contains("corps") || p.role == "body"
        }) {
            let (_, _, w, h) = part_bbox(body);
            if w < h * 1.2 {
                issues.push(IllustReviewIssue {
                    kind: "not_stretched".into(),
                    severity: "error".into(),
                    message: "Subject asks for a stretch but body is not horizontal (w should exceed h)."
                        .into(),
                });
            }
        }
    }
    if subject_has_furniture(&subject_l) {
        let has_furniture = spec.parts.iter().any(|p| {
            let id = p.id.to_ascii_lowercase();
            id.contains("cushion")
                || id.contains("sofa")
                || id.contains("couch")
                || id.contains("canap")
                || id.contains("pillow")
                || p.role == "furniture"
        });
        if !has_furniture {
            issues.push(IllustReviewIssue {
                kind: "missing_furniture".into(),
                severity: "error".into(),
                message: "Brief mentions a cushion/sofa but no furniture part is present — recompose."
                    .into(),
            });
        }
    }
    if looks_center_anchored(
        &spec
            .parts
            .iter()
            .filter(|p| !is_background_part(p))
            .collect::<Vec<_>>(),
    ) {
        issues.push(IllustReviewIssue {
            kind: "center_coords".into(),
            severity: "error".into(),
            message: "Parts look center-anchored — recompose with empty parts so puppet uses top-left unit boxes."
                .into(),
        });
    }
    if ellipse_only_blob(&spec.parts) {
        issues.push(IllustReviewIssue {
            kind: "ellipse_only".into(),
            severity: "error".into(),
            message: "Silhouette is a stack of near-circles (skill rule 12 fails at 240px). Recompose with empty parts so the puppet replaces the snowman — overlapping body/head/ears, plus sofa or cushion."
                .into(),
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

/// Expand a sparse / bad LLM scene into a skill-style puppet (3–8 overlapping parts).
pub fn enrich_illustration_puppet(spec: &mut IllustrationSpec) {
    let subject = format!(
        "{} {} {}",
        spec.brief.subject,
        spec.brief.anchor,
        spec.brief.beats.join(" ")
    )
    .to_ascii_lowercase();

    if !needs_puppet_enrichment(&spec.parts, &subject) {
        ensure_scribble_and_construction(spec);
        return;
    }

    let backgrounds = spec
        .parts
        .iter()
        .filter(|p| is_background_part(p))
        .cloned()
        .collect::<Vec<_>>();
    let anchor = subject_anchor(&spec.parts);
    let mut parts = if backgrounds.is_empty() {
        default_background_for_subject(&subject)
    } else {
        backgrounds
    };
    parts.extend(puppet_parts_for_subject(&subject, anchor));
    // Stable unique ids
    let mut seen = std::collections::HashSet::new();
    parts.retain(|p| seen.insert(p.id.clone()));
    spec.parts = parts;
    ensure_scribble_and_construction(spec);
}

fn needs_puppet_enrichment(parts: &[IllustrationPart], subject: &str) -> bool {
    if parts.is_empty() {
        return true;
    }
    // Known skill recipes: trust the procedural puppet over LLM snowmen.
    if is_known_puppet_subject(subject) {
        return true;
    }
    let subject_parts: Vec<_> = parts.iter().filter(|p| !is_background_part(p)).collect();
    if subject_parts.len() < 5 {
        return true;
    }
    let has_head = subject_parts.iter().any(|p| {
        let id = p.id.to_ascii_lowercase();
        id.contains("head") || id.contains("tete") || id.contains("tête") || p.role == "head"
    });
    let has_body = subject_parts.iter().any(|p| {
        let id = p.id.to_ascii_lowercase();
        id.contains("body")
            || id.contains("corps")
            || matches!(p.role.as_str(), "body" | "main" | "subject" | "masse")
    });
    if !(has_head && has_body) {
        return true;
    }
    if looks_center_anchored(&subject_parts) {
        return true;
    }
    let wants_stretch = subject_wants_stretch(subject);
    if wants_stretch {
        if let Some(body) = subject_parts.iter().find(|p| {
            let id = p.id.to_ascii_lowercase();
            id.contains("body") || id.contains("corps") || p.role == "body"
        }) {
            let (_, _, w, h) = part_bbox(body);
            if w < h * 1.25 {
                return true;
            }
        }
    }
    let wants_furniture = subject_has_furniture(subject);
    if wants_furniture {
        let has_furniture = subject_parts.iter().any(|p| {
            let id = p.id.to_ascii_lowercase();
            id.contains("cushion")
                || id.contains("sofa")
                || id.contains("couch")
                || id.contains("canap")
                || id.contains("pillow")
                || p.role == "furniture"
        });
        if !has_furniture {
            return true;
        }
    }
    false
}

fn is_known_puppet_subject(subject: &str) -> bool {
    subject.contains("chat")
        || subject.contains("cat")
        || subject.contains("kitten")
        || subject.contains("chien")
        || subject.contains("dog")
        || subject.contains("puppy")
        || subject.contains("pingouin")
        || subject.contains("penguin")
}

fn subject_has_furniture(subject: &str) -> bool {
    subject.contains("coussin")
        || subject.contains("cushion")
        || subject.contains("pillow")
        || subject.contains("canap")
        || subject.contains("sofa")
        || subject.contains("couch")
        || subject.contains("fauteuil")
}

fn subject_wants_stretch(subject: &str) -> bool {
    subject.contains("étire")
        || subject.contains("etire")
        || subject.contains("stretch")
        || subject.contains("allong")
}

/// LLM often emits ellipse x,y as center (skill / canvas habit). Top-left recipe
/// then reads as a right-shifted snowman.
fn looks_center_anchored(parts: &[&IllustrationPart]) -> bool {
    let mut hits = 0;
    for p in parts {
        match &p.geometry {
            IllustrationPartGeometry::Ellipse { x, y, w, h, .. }
            | IllustrationPartGeometry::Rect { x, y, w, h, .. } => {
                if *w < 0.15 || *h < 0.15 {
                    continue;
                }
                let centerish_x = (*x - 0.5).abs() < 0.08;
                let overflows = *x + *w > 1.02 || *y + *h > 1.02;
                if centerish_x || overflows {
                    hits += 1;
                }
            }
            _ => {}
        }
    }
    hits >= 2
}

fn is_background_part(p: &IllustrationPart) -> bool {
    let id = p.id.to_ascii_lowercase();
    let role = p.role.to_ascii_lowercase();
    role == "background"
        || role == "bg"
        || id.starts_with("bg_")
        || id.contains("sky")
        || id.contains("ciel")
        || id.contains("sand")
        || id.contains("sable")
        || id.contains("sea")
        || id.contains("mer")
        || id.contains("plage")
        || id.contains("beach")
        || id.contains("ground")
}

fn is_furniture_part(p: &IllustrationPart) -> bool {
    let id = p.id.to_ascii_lowercase();
    let role = p.role.to_ascii_lowercase();
    role == "furniture"
        || id.contains("sofa")
        || id.contains("couch")
        || id.contains("canap")
        || id.contains("cushion")
        || id.contains("coussin")
        || id.contains("pillow")
        || id.contains("fauteuil")
}

fn ellipse_only_blob(parts: &[IllustrationPart]) -> bool {
    let creatures: Vec<_> = parts
        .iter()
        .filter(|p| !is_background_part(p) && !is_furniture_part(p))
        .collect();
    if creatures.len() < 2 || !creatures.iter().copied().all(is_ellipse_part) {
        return false;
    }
    let has_break = parts.iter().any(|p| {
        !is_background_part(p)
            && (is_furniture_part(p)
                || matches!(
                    p.geometry,
                    IllustrationPartGeometry::Rect { .. } | IllustrationPartGeometry::Path { .. }
                ))
    });
    let body = creatures.iter().copied().find(|p| {
        let id = p.id.to_ascii_lowercase();
        p.role == "body" || id.contains("body") || id.contains("corps")
    });
    let aspect = body
        .map(|b| {
            let (_, _, w, h) = part_bbox(b);
            w.max(h) / w.min(h).max(0.01)
        })
        .unwrap_or(1.0);
    aspect < 1.28 && !has_break
}

fn is_ellipse_part(p: &IllustrationPart) -> bool {
    matches!(p.geometry, IllustrationPartGeometry::Ellipse { .. })
}

fn part_bbox(p: &IllustrationPart) -> (f32, f32, f32, f32) {
    match &p.geometry {
        IllustrationPartGeometry::Ellipse { x, y, w, h, .. }
        | IllustrationPartGeometry::Rect { x, y, w, h, .. } => (*x, *y, *w, *h),
        IllustrationPartGeometry::Path { points, .. } => {
            if points.is_empty() {
                return (0.3, 0.3, 0.4, 0.4);
            }
            let min_x = points.iter().map(|q| q.x).fold(f32::INFINITY, f32::min);
            let max_x = points.iter().map(|q| q.x).fold(f32::NEG_INFINITY, f32::max);
            let min_y = points.iter().map(|q| q.y).fold(f32::INFINITY, f32::min);
            let max_y = points.iter().map(|q| q.y).fold(f32::NEG_INFINITY, f32::max);
            (min_x, min_y, (max_x - min_x).max(0.05), (max_y - min_y).max(0.05))
        }
    }
}

fn subject_anchor(parts: &[IllustrationPart]) -> (f32, f32) {
    let mut best: Option<(f32, (f32, f32))> = None;
    for p in parts.iter().filter(|p| !is_background_part(p)) {
        let (x, y, w, h) = part_bbox(p);
        let area = w * h;
        let cx = x + w * 0.5;
        let cy = y + h * 0.5;
        if best.map(|(a, _)| area > a).unwrap_or(true) {
            best = Some((area, (cx, cy)));
        }
    }
    let (cx, cy) = best.map(|(_, c)| c).unwrap_or((0.50, 0.42));
    // Keep the puppet in a readable frame (agents often park masses off-canvas).
    if !(0.28..=0.72).contains(&cx) || !(0.28..=0.58).contains(&cy) {
        (0.50, 0.42)
    } else {
        (cx, cy)
    }
}

fn path_part(
    id: &str,
    role: &str,
    fill_index: u8,
    points: &[(f32, f32)],
    seed: u32,
) -> IllustrationPart {
    IllustrationPart {
        id: id.into(),
        role: role.into(),
        fill_index,
        fill: true,
        outline: true,
        seed,
        geometry: IllustrationPartGeometry::Path {
            points: points
                .iter()
                .map(|(x, y)| CanvasPoint { x: *x, y: *y })
                .collect(),
            closed: true,
        },
    }
}

/// Chaikin corner-cut so a 6-point silhouette reads as a curve, not a pentagon.
fn smooth_closed(points: &[(f32, f32)], iters: u32) -> Vec<(f32, f32)> {
    let mut pts = points.to_vec();
    for _ in 0..iters {
        let n = pts.len();
        if n < 3 {
            break;
        }
        let mut next = Vec::with_capacity(n * 2);
        for i in 0..n {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            next.push((a.0 * 0.75 + b.0 * 0.25, a.1 * 0.75 + b.1 * 0.25));
            next.push((a.0 * 0.25 + b.0 * 0.75, a.1 * 0.25 + b.1 * 0.75));
        }
        pts = next;
    }
    pts
}

/// Front-view sofa + sitting dog as authored contours, not stacked ellipses.
fn sitting_dog_on_sofa(cx: f32, cy: f32) -> Vec<IllustrationPart> {
    let sofa = smooth_closed(
        &[
            (cx - 0.40, cy + 0.26),
            (cx - 0.42, cy + 0.04),
            (cx - 0.34, cy - 0.08),
            (cx - 0.22, cy - 0.16),
            (cx + 0.22, cy - 0.16),
            (cx + 0.34, cy - 0.08),
            (cx + 0.42, cy + 0.04),
            (cx + 0.40, cy + 0.26),
        ],
        3,
    );
    let body = smooth_closed(
        &[
            (cx - 0.08, cy - 0.08),
            (cx - 0.16, cy - 0.02),
            (cx - 0.18, cy + 0.08),
            (cx - 0.14, cy + 0.16),
            (cx - 0.08, cy + 0.18),
            (cx + 0.10, cy + 0.18),
            (cx + 0.16, cy + 0.14),
            (cx + 0.18, cy + 0.06),
            (cx + 0.14, cy - 0.02),
            (cx + 0.06, cy - 0.08),
        ],
        2,
    );
    let head = smooth_closed(
        &[
            (cx - 0.06, cy - 0.20),
            (cx - 0.09, cy - 0.12),
            (cx - 0.07, cy - 0.04),
            (cx, cy - 0.01),
            (cx + 0.07, cy - 0.04),
            (cx + 0.09, cy - 0.12),
            (cx + 0.06, cy - 0.20),
            (cx, cy - 0.24),
        ],
        2,
    );
    let ear_l = smooth_closed(
        &[
            (cx - 0.07, cy - 0.20),
            (cx - 0.14, cy - 0.18),
            (cx - 0.16, cy - 0.08),
            (cx - 0.13, cy + 0.01),
            (cx - 0.09, cy - 0.02),
            (cx - 0.06, cy - 0.14),
        ],
        2,
    );
    let ear_r = smooth_closed(
        &[
            (cx + 0.07, cy - 0.20),
            (cx + 0.14, cy - 0.18),
            (cx + 0.16, cy - 0.08),
            (cx + 0.13, cy + 0.01),
            (cx + 0.09, cy - 0.02),
            (cx + 0.06, cy - 0.14),
        ],
        2,
    );
    let snout = smooth_closed(
        &[
            (cx - 0.045, cy - 0.10),
            (cx - 0.06, cy - 0.05),
            (cx, cy - 0.02),
            (cx + 0.06, cy - 0.05),
            (cx + 0.045, cy - 0.10),
        ],
        2,
    );
    let paws = smooth_closed(
        &[
            (cx - 0.12, cy + 0.14),
            (cx - 0.14, cy + 0.24),
            (cx - 0.08, cy + 0.26),
            (cx - 0.06, cy + 0.16),
            (cx - 0.02, cy + 0.16),
            (cx, cy + 0.26),
            (cx + 0.06, cy + 0.24),
            (cx + 0.04, cy + 0.14),
        ],
        1,
    );
    let tail = smooth_closed(
        &[
            (cx + 0.14, cy + 0.02),
            (cx + 0.24, cy - 0.04),
            (cx + 0.30, cy + 0.02),
            (cx + 0.26, cy + 0.10),
            (cx + 0.16, cy + 0.08),
        ],
        2,
    );
    vec![
        path_part("sofa_base", "furniture", 3, &sofa, 10),
        path_part("body", "body", 2, &body, 30),
        path_part("paw_fl", "body", 2, &paws, 44),
        path_part("head", "head", 2, &head, 32),
        path_part("ear_l", "head", 2, &ear_l, 33),
        path_part("ear_r", "head", 2, &ear_r, 34),
        path_part("snout", "head", 3, &snout, 35),
        path_part("tail", "body", 2, &tail, 42),
        ellipse_part("eye_l", "head", 0, cx - 0.045, cy - 0.16, 0.028, 0.032, 40),
        ellipse_part("eye_r", "head", 0, cx + 0.018, cy - 0.16, 0.028, 0.032, 41),
    ]
}

fn ellipse_part(
    id: &str,
    role: &str,
    fill_index: u8,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    seed: u32,
) -> IllustrationPart {
    IllustrationPart {
        id: id.into(),
        role: role.into(),
        fill_index,
        fill: true,
        outline: true,
        seed,
        geometry: IllustrationPartGeometry::Ellipse {
            x,
            y,
            w,
            h,
            rotation: 0.0,
        },
    }
}

fn rect_part(
    id: &str,
    role: &str,
    fill_index: u8,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    seed: u32,
    outline: bool,
) -> IllustrationPart {
    IllustrationPart {
        id: id.into(),
        role: role.into(),
        fill_index,
        fill: true,
        outline,
        seed,
        geometry: IllustrationPartGeometry::Rect {
            x,
            y,
            w,
            h,
            rotation: 0.0,
        },
    }
}

fn default_background_for_subject(subject: &str) -> Vec<IllustrationPart> {
    if subject.contains("plage")
        || subject.contains("beach")
        || subject.contains("mer")
        || subject.contains("sea")
        || subject.contains("sable")
    {
        vec![
            rect_part("bg_sky", "background", 0, 0.0, 0.0, 1.0, 0.55, 1, false),
            rect_part("bg_sea", "background", 1, 0.0, 0.52, 1.0, 0.22, 2, false),
            rect_part("bg_sand", "background", 3, 0.0, 0.70, 1.0, 0.30, 3, false),
        ]
    } else if subject_has_furniture(subject) {
        vec![
            rect_part("bg_wall", "background", 0, 0.0, 0.0, 1.0, 0.62, 1, false),
            rect_part("bg_floor", "background", 3, 0.0, 0.60, 1.0, 0.40, 2, false),
        ]
    } else {
        vec![rect_part(
            "bg",
            "background",
            0,
            0.0,
            0.0,
            1.0,
            1.0,
            1,
            false,
        )]
    }
}

fn puppet_parts_for_subject(subject: &str, anchor: (f32, f32)) -> Vec<IllustrationPart> {
    let (cx, cy) = anchor;
    let mut parts = Vec::new();

    let is_cat = subject.contains("chat") || subject.contains("cat") || subject.contains("kitten");
    let is_dog = subject.contains("chien") || subject.contains("dog") || subject.contains("puppy");
    let is_penguin = subject.contains("pingouin") || subject.contains("penguin");
    let wants_stretch = subject_wants_stretch(subject);
    let has_cushion = subject.contains("coussin")
        || subject.contains("cushion")
        || subject.contains("pillow");
    let has_sofa = subject.contains("canap")
        || subject.contains("sofa")
        || subject.contains("couch")
        || subject.contains("fauteuil");

    // Furniture sits behind the animal (drawn first = under).
    if is_dog && has_sofa && !wants_stretch {
        return sitting_dog_on_sofa(cx, cy);
    }
    if has_sofa {
        parts.push(rect_part(
            "sofa_base",
            "furniture",
            3,
            cx - 0.38,
            cy + 0.02,
            0.76,
            0.28,
            10,
            true,
        ));
        parts.push(rect_part(
            "sofa_back",
            "furniture",
            3,
            cx - 0.36,
            cy - 0.16,
            0.72,
            0.22,
            11,
            true,
        ));
        parts.push(ellipse_part(
            "sofa_arm_l",
            "furniture",
            2,
            cx - 0.40,
            cy - 0.02,
            0.12,
            0.22,
            12,
        ));
        parts.push(ellipse_part(
            "sofa_arm_r",
            "furniture",
            2,
            cx + 0.28,
            cy - 0.02,
            0.12,
            0.22,
            13,
        ));
    } else if has_cushion {
        parts.push(ellipse_part(
            "cushion",
            "furniture",
            3,
            cx - 0.36,
            cy + 0.06,
            0.72,
            0.34,
            10,
        ));
    }

    if subject.contains("velo")
        || subject.contains("vélo")
        || subject.contains("bike")
        || subject.contains("bicycle")
        || subject.contains("cycl")
    {
        parts.push(ellipse_part(
            "wheel_f",
            "body",
            3,
            cx - 0.28,
            cy + 0.12,
            0.16,
            0.16,
            20,
        ));
        parts.push(ellipse_part(
            "wheel_r",
            "body",
            3,
            cx + 0.10,
            cy + 0.12,
            0.16,
            0.16,
            21,
        ));
        parts.push(rect_part(
            "bike_frame",
            "body",
            3,
            cx - 0.18,
            cy + 0.02,
            0.36,
            0.06,
            22,
            true,
        ));
    }

    // Creature mass
    let (body_w, body_h) = if is_penguin {
        (0.22, 0.34)
    } else if (is_cat || is_dog) && wants_stretch {
        (0.52, 0.17)
    } else if is_dog {
        (0.36, 0.22)
    } else if is_cat {
        (0.30, 0.24)
    } else {
        (0.28, 0.26)
    };
    let body_x = cx - body_w * 0.5;
    let body_y = if has_sofa {
        cy - body_h * 0.05
    } else if has_cushion {
        cy - body_h * 0.20
    } else if wants_stretch {
        cy - body_h * 0.10
    } else {
        cy - body_h * 0.35
    };
    parts.push(ellipse_part(
        "body",
        "body",
        if is_cat || is_dog { 2 } else { 1 },
        body_x,
        body_y,
        body_w,
        body_h,
        30,
    ));

    // Legs / paws overlapping the support
    if is_dog || is_cat {
        let paw_y = body_y + body_h * 0.72;
        let paw_h = 0.07;
        let paw_w = if is_dog { 0.07 } else { 0.06 };
        parts.push(ellipse_part(
            "paw_fl",
            "body",
            2,
            body_x + body_w * 0.12,
            paw_y,
            paw_w,
            paw_h,
            44,
        ));
        parts.push(ellipse_part(
            "paw_fr",
            "body",
            2,
            body_x + body_w * 0.28,
            paw_y,
            paw_w,
            paw_h,
            45,
        ));
        parts.push(ellipse_part(
            "paw_bl",
            "body",
            2,
            body_x + body_w * 0.58,
            paw_y,
            paw_w,
            paw_h,
            46,
        ));
        parts.push(ellipse_part(
            "paw_br",
            "body",
            2,
            body_x + body_w * 0.74,
            paw_y,
            paw_w,
            paw_h,
            47,
        ));
    }

    if is_penguin {
        parts.push(ellipse_part(
            "belly",
            "body",
            2,
            body_x + body_w * 0.22,
            body_y + body_h * 0.22,
            body_w * 0.56,
            body_h * 0.55,
            31,
        ));
    }

    let head_w = if is_penguin {
        0.16
    } else if is_dog {
        0.20
    } else {
        0.18
    };
    let head_h = head_w;
    let head_x = if wants_stretch {
        body_x - head_w * 0.15
    } else {
        body_x + body_w * 0.55 - head_w * 0.5
    };
    let head_y = body_y - head_h * 0.45;
    parts.push(ellipse_part(
        "head",
        "head",
        if is_cat || is_dog { 2 } else { 1 },
        head_x,
        head_y,
        head_w,
        head_h,
        32,
    ));

    // Species ears (never give pointed cat ears to dogs).
    if is_cat {
        parts.push(ellipse_part(
            "ear_l",
            "head",
            2,
            head_x - 0.01,
            head_y - 0.04,
            0.07,
            0.09,
            33,
        ));
        parts.push(ellipse_part(
            "ear_r",
            "head",
            2,
            head_x + head_w - 0.06,
            head_y - 0.04,
            0.07,
            0.09,
            34,
        ));
    } else if is_dog {
        // Floppy ears hanging beside the head.
        parts.push(ellipse_part(
            "ear_l",
            "head",
            2,
            head_x - 0.04,
            head_y + head_h * 0.15,
            0.08,
            0.16,
            33,
        ));
        parts.push(ellipse_part(
            "ear_r",
            "head",
            2,
            head_x + head_w - 0.04,
            head_y + head_h * 0.15,
            0.08,
            0.16,
            34,
        ));
        // Snout
        parts.push(ellipse_part(
            "snout",
            "head",
            3,
            head_x + head_w * 0.28,
            head_y + head_h * 0.48,
            0.10,
            0.08,
            35,
        ));
    } else if !is_penguin {
        parts.push(ellipse_part(
            "ear_l",
            "head",
            2,
            head_x - 0.01,
            head_y - 0.03,
            0.06,
            0.07,
            33,
        ));
        parts.push(ellipse_part(
            "ear_r",
            "head",
            2,
            head_x + head_w - 0.05,
            head_y - 0.03,
            0.06,
            0.07,
            34,
        ));
    }
    if is_penguin {
        parts.push(ellipse_part(
            "beak",
            "head",
            3,
            head_x + head_w * 0.35,
            head_y + head_h * 0.45,
            0.07,
            0.04,
            35,
        ));
        parts.push(ellipse_part(
            "wing",
            "body",
            3,
            body_x - 0.04,
            body_y + 0.08,
            0.10,
            0.18,
            36,
        ));
    }

    // Eyes overlapping head
    let eye_y = head_y + head_h * 0.38;
    parts.push(ellipse_part(
        "eye_l",
        "head",
        0,
        head_x + head_w * 0.22,
        eye_y,
        0.035,
        0.04,
        40,
    ));
    parts.push(ellipse_part(
        "eye_r",
        "head",
        0,
        head_x + head_w * 0.58,
        eye_y,
        0.035,
        0.04,
        41,
    ));

    if is_cat || is_dog {
        parts.push(ellipse_part(
            "tail",
            "body",
            2,
            body_x + body_w * 0.82,
            body_y + body_h * 0.25,
            if is_dog { 0.14 } else { 0.18 },
            if is_dog { 0.10 } else { 0.07 },
            42,
        ));
    }

    if subject.contains("casque") || subject.contains("helmet") {
        parts.push(ellipse_part(
            "helmet",
            "head",
            2,
            head_x - 0.01,
            head_y - 0.02,
            head_w + 0.02,
            head_h * 0.45,
            43,
        ));
    }

    parts
}

fn ensure_scribble_and_construction(spec: &mut IllustrationSpec) {
    if spec.scribble_part.is_none() {
        if let Some(body) = spec.parts.iter().find(|p| {
            let id = p.id.to_ascii_lowercase();
            id == "body" || id.contains("body") || p.role == "body" || p.role == "main"
        }) {
            spec.scribble_part = Some(body.id.clone());
        }
    }
    // Never auto-enable construction guides on stills (rainbow fringe); blueprint mode can set it.
    spec.show_construction = false;
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
        let riso = IllustrationPaletteColors::for_preset(IllustrationPaletteId::RisoPop);
        assert!(riso.inks.len() >= 3);
        assert_eq!(riso.finish, IllustrationFinish::Riso);
    }

    #[test]
    fn review_flags_empty() {
        let r = review_illustration(&IllustrationDoc::default());
        assert!(r.score < 0.5);
        assert!(r.issues.iter().any(|i| i.kind == "missing_brief"));
    }

    #[test]
    fn enrich_expands_sparse_cat_on_cushion() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "Un chat stylisé sur un coussin".into(),
                look: IllustrationLook::Ink,
                palette: IllustrationPaletteId::RisoPop,
                ..Default::default()
            },
            parts: vec![
                rect_part("bg_1", "background", 0, 0.0, 0.0, 1.0, 1.0, 1, false),
                ellipse_part("cushion_1", "body", 1, 0.1, 0.3, 0.8, 0.6, 2),
                ellipse_part("cat_1", "main", 2, 0.3, 0.4, 0.5, 0.4, 3),
            ],
            ..Default::default()
        };
        enrich_illustration_puppet(&mut spec);
        assert!(spec.parts.len() >= 7);
        assert!(spec.parts.iter().any(|p| p.id == "head"));
        assert!(spec.parts.iter().any(|p| p.id == "body"));
        assert!(spec.parts.iter().any(|p| p.id == "ear_l"));
        assert!(spec.parts.iter().any(|p| p.id == "eye_l"));
        assert!(spec.parts.iter().any(|p| p.id == "cushion"));
        assert!(spec.scribble_part.is_some());
    }

    #[test]
    fn enrich_keeps_detailed_non_recipe_puppet() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "oiseau".into(),
                ..Default::default()
            },
            parts: vec![
                ellipse_part("body", "body", 1, 0.3, 0.4, 0.3, 0.25, 1),
                ellipse_part("head", "head", 1, 0.35, 0.25, 0.2, 0.2, 2),
                ellipse_part("ear_l", "head", 1, 0.34, 0.18, 0.06, 0.08, 3),
                ellipse_part("ear_r", "head", 1, 0.5, 0.18, 0.06, 0.08, 4),
                ellipse_part("eye_l", "head", 2, 0.38, 0.32, 0.03, 0.03, 5),
                ellipse_part("eye_r", "head", 2, 0.48, 0.32, 0.03, 0.03, 6),
            ],
            ..Default::default()
        };
        let n = spec.parts.len();
        enrich_illustration_puppet(&mut spec);
        assert_eq!(spec.parts.len(), n);
    }

    #[test]
    fn enrich_dog_on_sofa_has_furniture_and_floppy_ears() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "chien sur un canapé".into(),
                look: IllustrationLook::Pencil,
                palette: IllustrationPaletteId::PencilMinimal,
                ..Default::default()
            },
            parts: vec![],
            ..Default::default()
        };
        enrich_illustration_puppet(&mut spec);
        assert!(spec.parts.iter().any(|p| p.id.starts_with("sofa_")));
        assert!(spec.parts.iter().any(|p| p.id == "snout"));
        assert!(spec.parts.iter().any(|p| p.id == "paw_fl"));
        let body = spec.parts.iter().find(|p| p.id == "body").unwrap();
        assert!(
            matches!(body.geometry, IllustrationPartGeometry::Path { .. }),
            "sitting dog body must be an authored path, not an ellipse"
        );
        let ear = spec.parts.iter().find(|p| p.id == "ear_l").unwrap();
        let (_, _, w, h) = part_bbox(ear);
        assert!(h > w, "dog ears should hang (taller than wide), got {w}x{h}");
    }

    #[test]
    fn enrich_replaces_llm_snowman_cat_on_cushion() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "chat qui s'étire sur un coussin".into(),
                look: IllustrationLook::Pencil,
                palette: IllustrationPaletteId::PencilMinimal,
                ..Default::default()
            },
            parts: vec![
                rect_part("bg", "background", 0, 0.0, 0.0, 1.0, 1.0, 1, false),
                ellipse_part("cushion", "coussin", 1, 0.5, 0.7, 0.6, 0.3, 2),
                ellipse_part("body", "corps", 2, 0.5, 0.5, 0.4, 0.4, 3),
                ellipse_part("head", "tête", 2, 0.5, 0.25, 0.25, 0.25, 4),
                ellipse_part("ears", "oreilles", 2, 0.4, 0.15, 0.2, 0.1, 5),
                ellipse_part("eyes", "yeux", 3, 0.5, 0.2, 0.08, 0.05, 6),
                ellipse_part("tail", "queue", 2, 0.3, 0.55, 0.1, 0.1, 7),
            ],
            ..Default::default()
        };
        enrich_illustration_puppet(&mut spec);
        assert!(spec.parts.iter().any(|p| p.id == "ear_l"));
        assert!(spec.parts.iter().any(|p| p.id == "eye_l"));
        let body = spec.parts.iter().find(|p| p.id == "body").unwrap();
        let (_, _, w, h) = part_bbox(body);
        assert!(w > h * 1.2, "stretching cat body should be horizontal, got {w}x{h}");
    }

    #[test]
    fn review_requires_contact_sheet_before_pass() {
        let mut doc = IllustrationDoc {
            brief: IllustrationBrief {
                subject: "chat".into(),
                ..Default::default()
            },
            spec: Some(IllustrationSpec {
                parts: vec![
                    ellipse_part("body", "body", 1, 0.3, 0.4, 0.3, 0.25, 1),
                    ellipse_part("head", "head", 1, 0.35, 0.25, 0.18, 0.18, 2),
                ],
                ..Default::default()
            }),
            ..Default::default()
        };
        let r = review_illustration(&doc);
        assert!(r.issues.iter().any(|i| i.kind == "missing_contact_sheet"));
        doc.last_sheet_png = Some("/downloads/canvas/sheet.png".into());
        let r2 = review_illustration(&doc);
        assert!(!r2.issues.iter().any(|i| i.kind == "missing_contact_sheet"));
    }

    #[test]
    fn review_rejects_round_ellipse_snowman() {
        let doc = IllustrationDoc {
            brief: IllustrationBrief {
                subject: "chien".into(),
                ..Default::default()
            },
            last_sheet_png: Some("/downloads/canvas/sheet.png".into()),
            spec: Some(IllustrationSpec {
                parts: vec![
                    ellipse_part("body", "body", 1, 0.3, 0.35, 0.28, 0.26, 1),
                    ellipse_part("head", "head", 1, 0.38, 0.18, 0.2, 0.2, 2),
                    ellipse_part("ear", "head", 1, 0.36, 0.1, 0.08, 0.08, 3),
                ],
                ..Default::default()
            }),
            ..Default::default()
        };
        let r = review_illustration(&doc);
        assert!(
            r.issues.iter().any(|i| i.kind == "ellipse_only"),
            "round stacked ellipses must fail review"
        );
    }

    #[test]
    fn review_allows_dog_on_sofa_recipe() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "chien sur un canapé".into(),
                look: IllustrationLook::Pencil,
                palette: IllustrationPaletteId::PencilMinimal,
                ..Default::default()
            },
            parts: vec![],
            ..Default::default()
        };
        enrich_illustration_puppet(&mut spec);
        let doc = IllustrationDoc {
            brief: spec.brief.clone(),
            spec: Some(spec),
            last_sheet_png: Some("/downloads/canvas/sheet.png".into()),
            ..Default::default()
        };
        let r = review_illustration(&doc);
        assert!(
            !r.issues.iter().any(|i| i.kind == "ellipse_only" && i.severity == "error"),
            "sofa recipe is not a snowman: {:?}",
            r.issues
        );
    }

    #[test]
    fn palette_unknown_falls_back() {
        let raw = "\"warm_sunny_beach\"";
        let p: IllustrationPaletteId = serde_json::from_str(raw).unwrap();
        assert_eq!(p, IllustrationPaletteId::PaperInk);
        assert_eq!(
            serde_json::to_string(&IllustrationPaletteId::RisoPop).unwrap(),
            "\"risoPop\""
        );
    }

    #[test]
    fn look_aliases_parse() {
        let look: IllustrationLook = serde_json::from_str("\"encre\"").unwrap();
        assert_eq!(look, IllustrationLook::Ink);
    }
}
