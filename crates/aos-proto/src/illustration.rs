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
    Doodle,
}

impl IllustrationLook {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ink => "ink",
            Self::Riso => "riso",
            Self::Screen => "screen",
            Self::Pencil => "pencil",
            Self::Blueprint => "blueprint",
            Self::Doodle => "doodle",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ink" | "encre" => Some(Self::Ink),
            "riso" => Some(Self::Riso),
            "screen" | "serigraphie" | "sérigraphie" => Some(Self::Screen),
            "pencil" | "crayon" => Some(Self::Pencil),
            "blueprint" | "plan" => Some(Self::Blueprint),
            "doodle" | "photo" => Some(Self::Doodle),
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
            Self::Doodle => IllustrationPaletteId::PaperInk,
        }
    }

    pub fn finish(self) -> IllustrationFinish {
        match self {
            Self::Ink | Self::Blueprint => IllustrationFinish::Ink,
            Self::Riso => IllustrationFinish::Riso,
            Self::Screen => IllustrationFinish::Screen,
            Self::Pencil => IllustrationFinish::Pencil,
            Self::Doodle => IllustrationFinish::Ink,
        }
    }
}

/// Where the frames come from. Flat is the drawn puppet; the other three are
/// the skill engines, bounded to this raster (no JS runtime, no video tracer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IllustrationEngine {
    #[default]
    Flat,
    Sand,
    Paper,
    Found,
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
    /// flat | sand | paper | found. Words in the subject fill this when left flat.
    #[serde(default)]
    pub engine: IllustrationEngine,
    /// Local photo for the doodle look. Empty unless the user supplied one.
    #[serde(default)]
    pub photo: String,
    /// A video path is refused: there is no rotoscope tracer.
    #[serde(default)]
    pub video: String,
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

/// Construction vocabulary used by the semantic illustration generator.
/// The plan is deliberately small and serializable: it records what must be
/// recognizable before the style pass, without exposing renderer internals.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum IllustrationArchetype {
    #[default]
    Subject,
    Human,
    Quadruped,
    Object,
    Furniture,
    Environment,
    /// A label the model invented. It does not reject the compose.
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IllustrationConstructionPlan {
    #[serde(default)]
    pub archetype: IllustrationArchetype,
    /// Ordered passes: skeleton, volumes, silhouette, identity, cleanup, style.
    #[serde(default)]
    pub passes: Vec<String>,
    /// Semantic landmarks that must survive at thumbnail size.
    #[serde(default)]
    pub must_read: Vec<String>,
    /// Relations such as "hand holds pipe" or "cat rests on sofa".
    #[serde(default)]
    pub relations: Vec<String>,
    /// Dominant gesture used to avoid a rigid vertical pose.
    #[serde(default)]
    pub action_line: String,
    /// Semantic point that should win the first-glance test.
    #[serde(default)]
    pub focal_point: String,
    /// Coarse value design, independent from the final palette.
    #[serde(default)]
    pub value_groups: Vec<String>,
    /// face, profile, three_quarters, back, top_down or low_angle.
    #[serde(default)]
    pub head_orientation: String,
    /// Result of the thumbnail / black silhouette check.
    #[serde(default)]
    pub silhouette_test: bool,
    /// Stable views and pose references for animation redraws.
    #[serde(default)]
    pub model_sheet: Vec<String>,
    /// Material rule applied after the volume pass.
    #[serde(default)]
    pub material_pass: String,
    /// Actual gesture curve used by the skeleton pass.
    #[serde(default)]
    pub action_line_points: Vec<CanvasPoint>,
    #[serde(default)]
    pub chest_oval: Option<IllustrationConstructionOval>,
    #[serde(default)]
    pub pelvis_oval: Option<IllustrationConstructionOval>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum IllustrationConstructionPhase {
    Skeleton,
    Volumes,
    Contours,
    Details,
    #[default]
    Final,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IllustrationSkeletonJoint {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default)]
    pub radius: f32,
    /// Orientation in radians. Only `head` is read. A rotation must not move `x` or `y`.
    #[serde(default)]
    pub pitch: f32,
    #[serde(default)]
    pub yaw: f32,
    #[serde(default)]
    pub roll: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IllustrationVolume {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default)]
    pub w: f32,
    #[serde(default)]
    pub h: f32,
    #[serde(default)]
    pub rotation: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IllustrationConstructionOval {
    #[serde(default)]
    pub cx: f32,
    #[serde(default)]
    pub cy: f32,
    #[serde(default)]
    pub w: f32,
    #[serde(default)]
    pub h: f32,
    #[serde(default)]
    pub rotation: f32,
}

/// A complete replacement drawing for a named beat. Unlike pose knobs, this
/// stores the whole silhouette and its overlaps, so a changed head angle or
/// compressed body is redrawn rather than rigidly transformed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IllustrationKeyDrawing {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub pose: IllustrationPose,
    #[serde(default)]
    pub parts: Vec<IllustrationPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IllustrationSpec {
    #[serde(default = "default_illust_version")]
    pub version: u32,
    #[serde(default)]
    pub brief: IllustrationBrief,
    #[serde(default)]
    pub parts: Vec<IllustrationPart>,
    /// Optional whole-pose drawings keyed by timeline beat name.
    #[serde(default)]
    pub key_drawings: Vec<IllustrationKeyDrawing>,
    /// Multi-pass construction contract used before the final style pass.
    #[serde(default)]
    pub construction: IllustrationConstructionPlan,
    /// Inspectable construction passes. `parts` remains the final artwork.
    #[serde(default)]
    pub construction_phase: IllustrationConstructionPhase,
    #[serde(default)]
    pub skeleton: Vec<IllustrationSkeletonJoint>,
    /// Path or volume id -> [origin joint, axis joint]. Bound coordinates are local:
    /// (0,0) is origin, (1,0) is axis joint, y is perpendicular in bone lengths.
    #[serde(default)]
    pub joint_bindings: std::collections::BTreeMap<String, [String; 2]>,
    /// Named two-bone contacts: [root, bend, effector, target landmark].
    /// The original chain provides lengths and bend side; root/target stay fixed.
    #[serde(default)]
    pub contacts: std::collections::BTreeMap<String, [String; 4]>,
    #[serde(default)]
    pub volumes: Vec<IllustrationVolume>,
    #[serde(default)]
    pub contours: Vec<IllustrationPart>,
    #[serde(default)]
    pub details: Vec<IllustrationPart>,
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
            key_drawings: Vec::new(),
            construction: IllustrationConstructionPlan::default(),
            construction_phase: IllustrationConstructionPhase::Final,
            skeleton: Vec::new(),
            joint_bindings: Default::default(),
            contacts: Default::default(),
            volumes: Vec::new(),
            contours: Vec::new(),
            details: Vec::new(),
            pose: IllustrationPose::default(),
            camera: IllustrationCamera::default(),
            mode: IllustrationRenderMode::Normal,
            show_construction: false,
            scribble_part: None,
        }
    }
}

impl IllustrationSpec {
    /// Resolve authored local contours using the current pose, without changing
    /// the source drawing. Missing or collapsed anchors are errors, never (0.5,0.5).
    pub fn resolve_joint_bindings(&self) -> Result<Self, String> {
        self.validate_skeleton()?;
        let mut resolved = self.clone();
        resolved.solve_contacts()?;
        for (id, anchors) in &self.joint_bindings {
            let joint = |name: &str| {
                resolved
                    .skeleton
                    .iter()
                    .find(|j| j.id == name)
                    .ok_or_else(|| format!("{id}: missing joint {name}"))
            };
            let a = joint(&anchors[0])?.clone();
            let b = joint(&anchors[1])?.clone();
            let dx = b.x - a.x;
            let dy = b.y - a.y;
            if !dx.is_finite() || !dy.is_finite() || dx.hypot(dy) < 0.00001 {
                return Err(format!("{id}: invalid or collapsed joint axis"));
            }
            let mut found = false;
            for part in resolved
                .parts
                .iter_mut()
                .chain(resolved.contours.iter_mut())
                .chain(resolved.details.iter_mut())
            {
                if part.id != *id {
                    continue;
                }
                found = true;
                let IllustrationPartGeometry::Path { points, .. } = &mut part.geometry else {
                    return Err(format!("{id}: joint binding requires a path"));
                };
                for p in points {
                    let (u, v) = (p.x, p.y);
                    p.x = a.x + u * dx - v * dy;
                    p.y = a.y + u * dy + v * dx;
                    if !p.x.is_finite() || !p.y.is_finite() {
                        return Err(format!("{id}: non-finite contour coordinate"));
                    }
                }
            }
            for volume in resolved.volumes.iter_mut().filter(|v| v.id == *id) {
                found = true;
                if !volume.x.is_finite()
                    || !volume.y.is_finite()
                    || !volume.w.is_finite()
                    || !volume.h.is_finite()
                    || !volume.rotation.is_finite()
                    || volume.w <= 0.0
                    || volume.h <= 0.0
                {
                    return Err(format!("{id}: invalid local volume"));
                }
                let u = volume.x + volume.w * 0.5;
                let v = volume.y + volume.h * 0.5;
                let length = dx.hypot(dy);
                volume.w *= length;
                volume.h *= length;
                volume.x = a.x + u * dx - v * dy - volume.w * 0.5;
                volume.y = a.y + u * dy + v * dx - volume.h * 0.5;
                volume.rotation += dy.atan2(dx);
            }
            if !found {
                return Err(format!("{id}: binding has no contour or volume"));
            }
        }
        resolved.joint_bindings.clear();
        Ok(resolved)
    }

    fn validate_skeleton(&self) -> Result<(), String> {
        let mut joints = std::collections::HashMap::new();
        for joint in &self.skeleton {
            if joint.id.trim().is_empty() || joints.insert(joint.id.as_str(), joint).is_some() {
                return Err(format!(
                    "skeleton: empty or duplicate joint id '{}'",
                    joint.id
                ));
            }
            if !joint.x.is_finite()
                || !joint.y.is_finite()
                || !joint.radius.is_finite()
                || joint.radius < 0.0
            {
                return Err(format!(
                    "skeleton: invalid coordinates/radius for {}",
                    joint.id
                ));
            }
        }
        for joint in &self.skeleton {
            let mut visited = std::collections::HashSet::new();
            let mut current = joint;
            loop {
                if !visited.insert(current.id.as_str()) {
                    return Err(format!("skeleton: parent cycle involving {}", current.id));
                }
                let Some(parent) = current.parent.as_deref() else {
                    break;
                };
                current = joints.get(parent).copied().ok_or_else(|| {
                    format!(
                        "skeleton: {} references missing parent {parent}",
                        current.id
                    )
                })?;
            }
        }
        Ok(())
    }

    fn solve_contacts(&mut self) -> Result<(), String> {
        // Simultaneous shared-chain solving is not supported. Reject conflicts
        // rather than silently letting map iteration order change a contact.
        let mut moved = std::collections::HashSet::new();
        for (name, ids) in &self.contacts {
            if ids.iter().collect::<std::collections::HashSet<_>>().len() != 4
                || !moved.insert(ids[1].clone())
                || !moved.insert(ids[2].clone())
            {
                return Err(format!("{name}: overlapping or repeated contact joints"));
            }
        }
        for (name, ids) in &self.contacts {
            if moved.contains(&ids[0]) || moved.contains(&ids[3]) {
                return Err(format!(
                    "{name}: contact root and target must be fixed landmarks"
                ));
            }
            let find = |id: &str| {
                self.skeleton
                    .iter()
                    .position(|j| j.id == id)
                    .ok_or_else(|| format!("{name}: missing contact joint {id}"))
            };
            let [ai, bi, ci, ti] = [
                find(&ids[0])?,
                find(&ids[1])?,
                find(&ids[2])?,
                find(&ids[3])?,
            ];
            let a = &self.skeleton[ai];
            let b = &self.skeleton[bi];
            let c = &self.skeleton[ci];
            let t = &self.skeleton[ti];
            if [a, b, c, t]
                .iter()
                .any(|j| !j.x.is_finite() || !j.y.is_finite())
            {
                return Err(format!("{name}: non-finite contact coordinate"));
            }
            let l1 = (b.x - a.x).hypot(b.y - a.y);
            let l2 = (c.x - b.x).hypot(c.y - b.y);
            let (dx, dy) = (t.x - a.x, t.y - a.y);
            let d = dx.hypot(dy);
            if l1 < 0.00001
                || l2 < 0.00001
                || d < 0.00001
                || d > l1 + l2 + 0.000001
                || d < (l1 - l2).abs() - 0.000001
            {
                return Err(format!(
                    "{name}: contact target unreachable without changing limb lengths"
                ));
            }
            let along = (l1 * l1 - l2 * l2 + d * d) / (2.0 * d);
            let height = (l1 * l1 - along * along).max(0.0).sqrt();
            let cross = dx * (b.y - a.y) - dy * (b.x - a.x);
            let side = if cross < 0.0 { -1.0 } else { 1.0 };
            let bend_x = a.x + along * dx / d - side * height * dy / d;
            let bend_y = a.y + along * dy / d + side * height * dx / d;
            let (target_x, target_y) = (t.x, t.y);
            self.skeleton[bi].x = bend_x;
            self.skeleton[bi].y = bend_y;
            self.skeleton[ci].x = target_x;
            self.skeleton[ci].y = target_y;
        }
        self.contacts.clear();
        Ok(())
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IllustrationImageStatus {
    Running,
    NeedsReview,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IllustrationImageRun {
    pub id: String,
    pub source_revision: u64,
    pub phase: IllustrationConstructionPhase,
    pub status: IllustrationImageStatus,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub source_png: Option<String>,
    #[serde(default)]
    pub candidate_png: Option<String>,
    /// Selection of a revision, not a claim of artistic approval.
    #[serde(default)]
    pub candidate_selected: Option<bool>,
    /// Single depicted instant; the full user request remains in doc.brief.
    #[serde(default)]
    pub frame_subject: Option<String>,
    /// Saved noise seed for reproducible variants. Legacy runs used 42.
    #[serde(default)]
    pub seed: Option<u32>,
    /// Archived guide used only to bootstrap the first image pass.
    #[serde(default)]
    pub pose_reference_png: Option<String>,
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
    pub image_run: Option<IllustrationImageRun>,
    #[serde(default)]
    pub timeline: IllustrationTimeline,
    #[serde(default)]
    pub lock: Option<IllustrationLock>,
    /// Last rendered still path under /downloads.
    #[serde(default)]
    pub last_png: Option<String>,
    /// Published construction previews: (phase, source revision, immutable PNG path).
    #[serde(default)]
    pub pass_previews: Vec<(IllustrationConstructionPhase, u64, String)>,
    #[serde(default)]
    pub last_sheet_png: Option<String>,
    /// Last generated character model sheet (views / expressions / pose refs).
    #[serde(default)]
    pub last_model_sheet_png: Option<String>,
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
pub struct IllustGenerateImageRequest {
    pub session_id: String,
    pub holder: String,
    /// Pose/layout only. Original identity and style come from the saved brief.
    pub construction: String,
    /// Identity/details for exactly one keyframe, without successive actions.
    #[serde(default)]
    pub frame_subject: Option<String>,
    /// Omit for a new variation; supply to reproduce an earlier run.
    #[serde(default)]
    pub seed: Option<u32>,
    /// Optional PNG in Akasha /downloads, not a host filesystem path.
    #[serde(default)]
    pub pose_reference_png: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustRefineImageRequest {
    pub session_id: String,
    pub holder: String,
    pub correction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IllustResolveImageRequest {
    pub session_id: String,
    pub holder: String,
    pub run_id: String,
    pub keep_candidate: bool,
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
    /// Total timeline length in seconds. Scales existing beats, or builds a default pair.
    #[serde(default)]
    pub duration_s: Option<f32>,
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
    if spec.construction_phase != IllustrationConstructionPhase::Final {
        issues.push(IllustReviewIssue {
            kind: "unfinished_construction".into(),
            severity: "error".into(),
            message:
                "Construction pass is still in progress. Continue composing before final export."
                    .into(),
        });
    }
    if let Err(message) = spec.resolve_joint_bindings() {
        issues.push(IllustReviewIssue {
            kind: "invalid_joint_binding".into(),
            severity: "error".into(),
            message,
        });
    }
    if spec.parts.is_empty() {
        issues.push(IllustReviewIssue {
            kind: "empty_parts".into(),
            severity: "error".into(),
            message: "Spec has no parts — silhouette cannot read.".into(),
        });
    }
    let subject_parts = spec.parts.iter().filter(|p| !is_background_part(p)).count();
    if !spec.parts.is_empty() && subject_parts < 5 {
        issues.push(IllustReviewIssue {
            kind: "sparse_puppet".into(),
            severity: "warning".into(),
            message:
                "Fewer than 5 subject parts — enrich to head/body/ears/eyes (skill puppet recipe)."
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
    if doc
        .last_sheet_png
        .as_ref()
        .map(|s| s.trim().is_empty())
        .unwrap_or(true)
    {
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
    if !spec.parts.is_empty() {
        if spec.construction.action_line.trim().is_empty() {
            issues.push(IllustReviewIssue {
                kind: "missing_action_line".into(),
                severity: "warning".into(),
                message: "Add an action line before volumes so the pose does not become rigid."
                    .into(),
            });
        }
        if spec.construction.focal_point.trim().is_empty() {
            issues.push(IllustReviewIssue {
                kind: "missing_focal_point".into(),
                severity: "warning".into(),
                message: "Name the first-glance focal point before applying style.".into(),
            });
        }
        if spec.construction.value_groups.len() < 3 {
            issues.push(IllustReviewIssue {
                kind: "missing_value_design".into(),
                severity: "warning".into(),
                message: "Define background, subject and focal value groups before color.".into(),
            });
        }
        if !spec.construction.silhouette_test {
            issues.push(IllustReviewIssue {
                kind: "silhouette_test_failed".into(),
                severity: "warning".into(),
                message: "The black silhouette / thumbnail test is not validated yet.".into(),
            });
        }
        if spec.construction.model_sheet.len() < 4 {
            issues.push(IllustReviewIssue {
                kind: "missing_model_sheet".into(),
                severity: "warning".into(),
                message: "Keep stable face/profile/three-quarter/back references for redraws."
                    .into(),
            });
        }
    }
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
                    message:
                        "Subject asks for a stretch but body is not horizontal (w should exceed h)."
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
                message:
                    "Brief mentions a cushion/sofa but no furniture part is present — recompose."
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
    // Taxonomic armature present: refuse ellipse-only (or unbound) dressing.
    if taxonomic_skeleton_present(&spec.skeleton) {
        let subject_parts: Vec<_> = spec
            .parts
            .iter()
            .filter(|p| !is_background_part(p) && !is_furniture_part(p))
            .collect();
        let has_bound_path = spec.parts.iter().any(|p| {
            !is_background_part(p)
                && matches!(
                    &p.geometry,
                    IllustrationPartGeometry::Path { points, .. } if points.len() >= 8
                )
                && (spec.joint_bindings.contains_key(&p.id)
                    || p.role == "body"
                    || p.role == "head"
                    || p.id.contains("body")
                    || p.id.contains("head"))
        });
        let all_subject_ellipses =
            !subject_parts.is_empty() && subject_parts.iter().copied().all(is_ellipse_part);
        if subject_parts.is_empty() || all_subject_ellipses || !has_bound_path {
            issues.push(IllustReviewIssue {
                kind: "skeleton_undressed".into(),
                severity: "error".into(),
                message: "A taxonomic skeleton is present but the subject is still undressed (empty, ellipse-only, or without bound path contours). Add joint-bound path geometry; do not replace the skeleton with a new ellipse puppet.".into(),
            });
        }
    }
    // A still must contain at least one authored contour. Primitive-only
    // scenes can be valid diagrams, but they are not an acceptable subject
    // drawing for the illustration pipeline: texture cannot turn a stack of
    // ovals into a recognizable character or object.
    if matches!(doc.brief.engine, IllustrationEngine::Flat | IllustrationEngine::Found)
        && subject_parts >= 3
        && !spec.parts.iter().any(|p| {
            !is_background_part(p)
                && matches!(&p.geometry, IllustrationPartGeometry::Path { points, .. } if points.len() >= 8)
        })
    {
        issues.push(IllustReviewIssue {
            kind: "no_authored_contour".into(),
            severity: "error".into(),
            message: "The subject has no authored contour. Redraw the main silhouette with path geometry before adding texture.".into(),
        });
    }
    let subject_line = doc.brief.subject.to_ascii_lowercase();
    if subject_has_furniture(&subject_line) && !spec.parts.iter().any(is_furniture_part) {
        issues.push(IllustReviewIssue {
            kind: "prop_missing".into(),
            severity: "error".into(),
            message: "The subject asks for a sofa or cushion, and no furniture part is present."
                .into(),
        });
    }
    let cat_or_dog = subject_line.contains("chat")
        || subject_line.contains("cat")
        || subject_line.contains("chien")
        || subject_line.contains("dog");
    if cat_or_dog {
        let has_ear = spec.parts.iter().any(|p| {
            let id = p.id.to_ascii_lowercase();
            id.contains("ear") || id.contains("oreille")
        });
        let has_eye = spec.parts.iter().any(|p| {
            let id = p.id.to_ascii_lowercase();
            id.contains("eye") || id.contains("oeil")
        });
        if !has_ear || !has_eye {
            issues.push(IllustReviewIssue {
                kind: "unreadable_puppet".into(),
                severity: "error".into(),
                message: "A cat or dog needs ears and eyes. Two ellipses are not a character."
                    .into(),
            });
        }
    }
    if wants_bike(&subject_line)
        && !spec
            .parts
            .iter()
            .any(|p| p.id.to_ascii_lowercase().contains("wheel"))
    {
        issues.push(IllustReviewIssue {
            kind: "prop_missing".into(),
            severity: "error".into(),
            message: "The subject asks for a bike, and no wheel part is present.".into(),
        });
    }
    if wants_paper(&subject_line)
        && !spec
            .parts
            .iter()
            .any(|p| p.id.to_ascii_lowercase().contains("paper"))
    {
        issues.push(IllustReviewIssue {
            kind: "prop_missing".into(),
            severity: "error".into(),
            message: "The subject asks for a newspaper or magazine, and no paper part is present."
                .into(),
        });
    }
    if split_scene_clauses(&subject_line).len() <= 1
        && spec.parts.iter().any(|p| {
            let id = p.id.to_ascii_lowercase();
            id.starts_with("c1_") || id.starts_with("c2_")
        })
    {
        issues.push(IllustReviewIssue {
            kind: "extra_characters".into(),
            severity: "error".into(),
            message: "Several puppets were stamped, but the subject names a single scene.".into(),
        });
    }
    if subject_wants_jump(&subject_line) {
        let beats = if doc.timeline.beats.is_empty() {
            crate::illustration_action::action_timeline(&doc.brief, 4.0)
        } else {
            doc.timeline.beats.clone()
        };
        let tucked = beats
            .iter()
            .any(|b| b.name != "signoff" && b.pose.tuck >= 0.35);
        if !tucked {
            issues.push(IllustReviewIssue {
                kind: "jump_missing".into(),
                severity: "error".into(),
                message: "The subject jumps, but no timeline beat tucks the body into the air."
                    .into(),
            });
        }
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

/// A known animal recipe may replace a bad drawing. Anything else — a person,
/// a pipe, a garden — keeps the parts the agent composed. The generic blob is
/// not a fallback for every sentence.
pub fn enrich_illustration_puppet(spec: &mut IllustrationSpec) {
    // Early passes must still become a visible drawing: pose, then masses,
    // then contours. A finished keyword recipe is only a last resort on an
    // empty final with no construction at all.
    if spec.construction_phase != IllustrationConstructionPhase::Final {
        stage_construction_phase(spec);
        return;
    }
    if !spec.skeleton.is_empty()
        || !spec.volumes.is_empty()
        || !spec.contours.is_empty()
        || !spec.details.is_empty()
    {
        return;
    }
    // Taxonomy poser: fill an empty primary skeleton (+ secondaries/contacts)
    // before recipes. Abstention leaves skeleton empty (OOD fallback).
    let taxonomy = crate::apply_taxonomy_skeleton(spec);
    let taxonomy_lock = crate::taxonomy_locks_puppet(&taxonomy) && !spec.skeleton.is_empty();
    let subject = spec.brief.subject.to_ascii_lowercase();
    if spec.construction == IllustrationConstructionPlan::default() {
        spec.construction = construction_plan_for_subject(&subject);
    }
    let has_drawing = spec.parts.iter().any(|p| !is_background_part(p));

    // Confident taxonomy: agent dresses on the armature. Do not replace with
    // species/person/keyword ellipse recipes that ignore the skeleton.
    // Multi-character briefs still use the legacy split recipes.
    if taxonomy_lock && !looks_multi_character(&subject) {
        if has_drawing {
            fix_center_anchored(&mut spec.parts);
        } else if !spec.parts.iter().any(is_scene_backdrop) {
            spec.parts = default_background_for_subject(&subject);
        }
        append_missing_props(spec, &subject);
        ensure_scribble_and_construction(spec);
        return;
    }

    if is_species_recipe(&subject) {
        if !needs_puppet_enrichment(&spec.parts, &subject) {
            append_missing_props(spec, &subject);
            ensure_scribble_and_construction(spec);
            return;
        }
        let backgrounds = spec
            .parts
            .iter()
            .filter(|p| is_scene_backdrop(p))
            .cloned()
            .collect::<Vec<_>>();
        let anchor = subject_anchor(&spec.parts);
        let mut parts = if backgrounds.is_empty() {
            default_background_for_subject(&subject)
        } else {
            backgrounds
        };
        parts.extend(puppet_parts_for_subject(&subject, anchor));
        let mut seen = std::collections::HashSet::new();
        parts.retain(|p| seen.insert(p.id.clone()));
        spec.parts = parts;
        add_authored_action_drawings(spec, &subject, anchor);
        ensure_scribble_and_construction(spec);
        return;
    }

    // A local model cannot author a silhouette. Five ellipses are not a drawing.
    if is_person_subject(&subject) && drawing_is_primitive(&spec.parts) {
        spec.parts = person_scene(&subject);
        // Person scenes are authored contours, but they can still require
        // semantic props such as a bicycle, paper or furniture from the brief.
        append_missing_props(spec, &subject);
        ensure_scribble_and_construction(spec);
        return;
    }

    if has_drawing {
        fix_center_anchored(&mut spec.parts);
        ensure_scribble_and_construction(spec);
        return;
    }

    spec.parts = keyword_scene(&subject);
    ensure_scribble_and_construction(spec);
}

fn is_species_recipe(subject: &str) -> bool {
    subject.contains("chat")
        || subject.contains("cat")
        || subject.contains("kitten")
        || subject.contains("chien")
        || subject.contains("dog")
        || subject.contains("puppy")
        || subject.contains("pingouin")
        || subject.contains("penguin")
        || is_elephant(subject)
}

fn looks_multi_character(subject: &str) -> bool {
    let markers = [
        "chat",
        "cat",
        "chien",
        "dog",
        "elephant",
        "éléphant",
        "pingouin",
        "penguin",
        "ours",
        "bear",
        "homme",
        "femme",
        "jardinier",
        "man",
        "woman",
    ];
    let hits = markers.iter().filter(|m| subject.contains(*m)).count();
    hits >= 2
}

/// Agents often send ellipse x,y as the centre. The raster reads top-left.
fn fix_center_anchored(parts: &mut [IllustrationPart]) {
    let refs: Vec<_> = parts.iter().filter(|p| !is_background_part(p)).collect();
    if !looks_center_anchored(&refs) {
        return;
    }
    for part in parts {
        if is_background_part(part) {
            continue;
        }
        match &mut part.geometry {
            IllustrationPartGeometry::Ellipse { x, y, w, h, .. }
            | IllustrationPartGeometry::Rect { x, y, w, h, .. } => {
                let centerish = (*x - 0.5).abs() < 0.12;
                let overflows = *x + *w > 1.02 || *y + *h > 1.02;
                if *w >= 0.08 && (centerish || overflows) {
                    *x = (*x - *w * 0.5).clamp(0.02, 0.9);
                    *y = (*y - *h * 0.5).clamp(0.02, 0.9);
                }
            }
            IllustrationPartGeometry::Path { .. } => {}
        }
    }
}

fn is_person_subject(subject: &str) -> bool {
    subject.contains("homme")
        || subject.contains("femme")
        || subject.contains("person")
        || subject.contains("humain")
        || subject.contains("human")
        || subject.contains("woman")
        || subject.contains("monsieur")
        || subject.contains("jardinier")
        || subject.contains("gardener")
        || subject.split_whitespace().any(|w| w == "man" || w == "men")
}

/// Fewer than two real contours. Ellipses and boxes are not a drawing.
fn drawing_is_primitive(parts: &[IllustrationPart]) -> bool {
    let drawn: Vec<_> = parts.iter().filter(|p| !is_background_part(p)).collect();
    if drawn.len() >= 10 {
        return false;
    }
    let rich = drawn
        .iter()
        .filter(|p| {
            matches!(
                &p.geometry,
                IllustrationPartGeometry::Path { points, .. } if points.len() >= 8
            )
        })
        .count();
    rich < 2
}

fn garden_tree(prefix: &str, cx: f32, top: f32, scale: f32, seed: u32) -> Vec<IllustrationPart> {
    let canopy = smooth_closed(
        &[
            (cx - 0.09 * scale, top + 0.08 * scale),
            (cx - 0.12 * scale, top),
            (cx - 0.04 * scale, top - 0.08 * scale),
            (cx + 0.03 * scale, top - 0.11 * scale),
            (cx + 0.11 * scale, top - 0.03 * scale),
            (cx + 0.13 * scale, top + 0.05 * scale),
            (cx + 0.05 * scale, top + 0.12 * scale),
            (cx - 0.02 * scale, top + 0.13 * scale),
        ],
        2,
    );
    let trunk = smooth_closed(
        &[
            (cx - 0.012 * scale, top + 0.08),
            (cx - 0.022 * scale, top + 0.38),
            (cx + 0.020 * scale, top + 0.40),
            (cx + 0.010 * scale, top + 0.10),
        ],
        1,
    );
    vec![
        path_part(&format!("{prefix}trunk"), "prop", 2, &trunk, seed),
        path_part(&format!("{prefix}canopy"), "prop", 1, &canopy, seed + 1),
    ]
}

fn bloom(i: usize, x: f32, y: f32, s: f32) -> Vec<IllustrationPart> {
    vec![
        rect_part(
            &format!("stem_{i}"),
            "prop",
            1,
            x - 0.006 * s,
            y + 0.02,
            0.012 * s,
            0.09,
            20 + i as u32,
            true,
        ),
        ellipse_part(
            &format!("flower_{i}"),
            "prop",
            3,
            x - 0.028 * s,
            y - 0.02,
            0.055 * s,
            0.05 * s,
            30 + i as u32,
        ),
    ]
}

/// Separate masses the eye can name: head, coat, arm, legs. One contour read as an animal.
#[derive(Clone, Copy)]
struct PoseJoint {
    id: &'static str,
    parent: Option<&'static str>,
    x: f32,
    y: f32,
    radius: f32,
}

fn human_pose_joints(sitting: bool) -> Vec<PoseJoint> {
    // Same eight-head canon as `human_standing` / `human_sitting` in the taxonomy poser.
    if sitting {
        vec![
            PoseJoint {
                id: "pelvis",
                parent: None,
                x: 0.46,
                y: 0.58,
                radius: 0.032,
            },
            PoseJoint {
                id: "waist",
                parent: Some("pelvis"),
                x: 0.46,
                y: 0.4675,
                radius: 0.024,
            },
            PoseJoint {
                id: "chest",
                parent: Some("waist"),
                x: 0.46,
                y: 0.40,
                radius: 0.038,
            },
            PoseJoint {
                id: "neck",
                parent: Some("chest"),
                x: 0.46,
                y: 0.328,
                radius: 0.02,
            },
            PoseJoint {
                id: "head",
                parent: Some("neck"),
                x: 0.46,
                y: 0.265,
                radius: 0.045,
            },
            PoseJoint {
                id: "shoulder_l",
                parent: Some("chest"),
                x: 0.37,
                y: 0.3595,
                radius: 0.02,
            },
            PoseJoint {
                id: "elbow_l",
                parent: Some("shoulder_l"),
                x: 0.316,
                y: 0.485,
                radius: 0.018,
            },
            PoseJoint {
                id: "hand_l",
                parent: Some("elbow_l"),
                x: 0.40,
                y: 0.56,
                radius: 0.016,
            },
            PoseJoint {
                id: "shoulder_r",
                parent: Some("chest"),
                x: 0.55,
                y: 0.3595,
                radius: 0.02,
            },
            PoseJoint {
                id: "elbow_r",
                parent: Some("shoulder_r"),
                x: 0.604,
                y: 0.485,
                radius: 0.018,
            },
            PoseJoint {
                id: "hand_r",
                parent: Some("elbow_r"),
                x: 0.52,
                y: 0.56,
                radius: 0.016,
            },
            PoseJoint {
                id: "hip_l",
                parent: Some("pelvis"),
                x: 0.415,
                y: 0.58,
                radius: 0.02,
            },
            PoseJoint {
                id: "knee_l",
                parent: Some("hip_l"),
                x: 0.561,
                y: 0.639,
                radius: 0.018,
            },
            PoseJoint {
                id: "ankle_l",
                parent: Some("knee_l"),
                x: 0.573,
                y: 0.796,
                radius: 0.016,
            },
            PoseJoint {
                id: "foot_l",
                parent: Some("ankle_l"),
                x: 0.621,
                y: 0.818,
                radius: 0.016,
            },
            PoseJoint {
                id: "hip_r",
                parent: Some("pelvis"),
                x: 0.505,
                y: 0.58,
                radius: 0.02,
            },
            PoseJoint {
                id: "knee_r",
                parent: Some("hip_r"),
                x: 0.657,
                y: 0.622,
                radius: 0.018,
            },
            PoseJoint {
                id: "ankle_r",
                parent: Some("knee_r"),
                x: 0.669,
                y: 0.779,
                radius: 0.016,
            },
            PoseJoint {
                id: "foot_r",
                parent: Some("ankle_r"),
                x: 0.717,
                y: 0.801,
                radius: 0.016,
            },
        ]
    } else {
        vec![
            PoseJoint {
                id: "pelvis",
                parent: None,
                x: 0.46,
                y: 0.48,
                radius: 0.032,
            },
            PoseJoint {
                id: "waist",
                parent: Some("pelvis"),
                x: 0.46,
                y: 0.3675,
                radius: 0.024,
            },
            PoseJoint {
                id: "chest",
                parent: Some("waist"),
                x: 0.46,
                y: 0.30,
                radius: 0.038,
            },
            PoseJoint {
                id: "neck",
                parent: Some("chest"),
                x: 0.46,
                y: 0.228,
                radius: 0.02,
            },
            PoseJoint {
                id: "head",
                parent: Some("neck"),
                x: 0.46,
                y: 0.165,
                radius: 0.045,
            },
            PoseJoint {
                id: "shoulder_l",
                parent: Some("chest"),
                x: 0.37,
                y: 0.2595,
                radius: 0.02,
            },
            PoseJoint {
                id: "elbow_l",
                parent: Some("shoulder_l"),
                x: 0.35,
                y: 0.3945,
                radius: 0.018,
            },
            PoseJoint {
                id: "hand_l",
                parent: Some("elbow_l"),
                x: 0.345,
                y: 0.507,
                radius: 0.016,
            },
            PoseJoint {
                id: "shoulder_r",
                parent: Some("chest"),
                x: 0.55,
                y: 0.2595,
                radius: 0.02,
            },
            PoseJoint {
                id: "elbow_r",
                parent: Some("shoulder_r"),
                x: 0.57,
                y: 0.3945,
                radius: 0.018,
            },
            PoseJoint {
                id: "hand_r",
                parent: Some("elbow_r"),
                x: 0.575,
                y: 0.507,
                radius: 0.016,
            },
            PoseJoint {
                id: "hip_l",
                parent: Some("pelvis"),
                x: 0.415,
                y: 0.48,
                radius: 0.02,
            },
            PoseJoint {
                id: "knee_l",
                parent: Some("hip_l"),
                x: 0.41,
                y: 0.6375,
                radius: 0.018,
            },
            PoseJoint {
                id: "ankle_l",
                parent: Some("knee_l"),
                x: 0.412,
                y: 0.795,
                radius: 0.016,
            },
            PoseJoint {
                id: "foot_l",
                parent: Some("ankle_l"),
                x: 0.44,
                y: 0.84,
                radius: 0.016,
            },
            PoseJoint {
                id: "hip_r",
                parent: Some("pelvis"),
                x: 0.505,
                y: 0.48,
                radius: 0.02,
            },
            PoseJoint {
                id: "knee_r",
                parent: Some("hip_r"),
                x: 0.51,
                y: 0.6375,
                radius: 0.018,
            },
            PoseJoint {
                id: "ankle_r",
                parent: Some("knee_r"),
                x: 0.508,
                y: 0.795,
                radius: 0.016,
            },
            PoseJoint {
                id: "foot_r",
                parent: Some("ankle_r"),
                x: 0.53,
                y: 0.84,
                radius: 0.016,
            },
        ]
    }
}

fn pose_joint(joints: &[PoseJoint], id: &str) -> (f32, f32) {
    joints
        .iter()
        .find(|joint| joint.id == id)
        .map(|joint| (joint.x, joint.y))
        .unwrap_or((0.5, 0.5))
}

#[allow(dead_code)]
fn capsule_part(
    id: &str,
    role: &str,
    a: (f32, f32),
    b: (f32, f32),
    radius: f32,
    fill_index: u8,
    seed: u32,
) -> IllustrationPart {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let nx = -dy / len * radius;
    let ny = dx / len * radius;
    let contour = smooth_closed(
        &[
            (a.0 + nx, a.1 + ny),
            (b.0 + nx, b.1 + ny),
            (b.0 + dx / len * radius, b.1 + dy / len * radius),
            (b.0 - nx, b.1 - ny),
            (a.0 - nx, a.1 - ny),
            (a.0 - dx / len * radius, a.1 - dy / len * radius),
        ],
        1,
    );
    path_part(id, role, fill_index, &contour, seed)
}

#[allow(dead_code)]
fn human_silhouette_from_skeleton(sitting: bool) -> IllustrationPart {
    let points = if sitting {
        vec![
            (0.405, 0.16),
            (0.48, 0.125),
            (0.555, 0.17),
            (0.565, 0.27),
            (0.525, 0.345),
            (0.405, 0.385),
            (0.345, 0.455),
            (0.355, 0.585),
            (0.425, 0.625),
            (0.50, 0.625),
            (0.61, 0.65),
            (0.69, 0.67),
            (0.705, 0.73),
            (0.665, 0.765),
            (0.59, 0.755),
            (0.59, 0.85),
            (0.655, 0.895),
            (0.625, 0.925),
            (0.56, 0.875),
            (0.545, 0.75),
            (0.425, 0.72),
            (0.34, 0.68),
            (0.30, 0.62),
            (0.33, 0.53),
            (0.34, 0.44),
            (0.38, 0.38),
        ]
    } else {
        vec![
            (0.385, 0.16),
            (0.46, 0.125),
            (0.535, 0.17),
            (0.55, 0.28),
            (0.515, 0.35),
            (0.56, 0.39),
            (0.70, 0.30),
            (0.73, 0.35),
            (0.58, 0.49),
            (0.53, 0.62),
            (0.50, 0.70),
            (0.54, 0.88),
            (0.50, 0.91),
            (0.45, 0.70),
            (0.40, 0.70),
            (0.35, 0.90),
            (0.31, 0.88),
            (0.34, 0.66),
            (0.31, 0.58),
            (0.32, 0.45),
            (0.37, 0.38),
        ]
    };
    path_part(
        "human_silhouette",
        "body",
        2,
        &smooth_closed(&points, 1),
        100,
    )
}

fn human_scene_from_skeleton(sitting: bool) -> Vec<IllustrationPart> {
    let joints = human_pose_joints(sitting);
    let chest = pose_joint(&joints, "chest");
    let pelvis = pose_joint(&joints, "pelvis");
    let neck = pose_joint(&joints, "neck");
    let head = pose_joint(&joints, "head");
    let shoulder_l = pose_joint(&joints, "shoulder_l");
    let elbow_l = pose_joint(&joints, "elbow_l");
    let hand_l = pose_joint(&joints, "hand_l");
    let hip_l = pose_joint(&joints, "hip_l");
    let knee_l = pose_joint(&joints, "knee_l");
    let ankle_l = pose_joint(&joints, "ankle_l");
    let shoulder_r = pose_joint(&joints, "shoulder_r");
    let elbow_r = pose_joint(&joints, "elbow_r");
    let hand_r = pose_joint(&joints, "hand_r");
    let torso = smooth_closed(
        &[
            (shoulder_l.0 - 0.025, shoulder_l.1),
            (shoulder_l.0 + 0.025, shoulder_l.1 - 0.045),
            (neck.0 - 0.055, neck.1 + 0.015),
            (neck.0 + 0.055, neck.1 + 0.015),
            (shoulder_r.0 - 0.015, shoulder_r.1 - 0.035),
            (shoulder_r.0 + 0.030, shoulder_r.1),
            (chest.0 + 0.105, chest.1 + 0.085),
            (pelvis.0 + 0.105, pelvis.1 + 0.025),
            (pelvis.0 - 0.105, pelvis.1 + 0.025),
            (chest.0 - 0.105, chest.1 + 0.085),
        ],
        2,
    );
    let mut parts = vec![
        path_part("body", "body", 2, &torso, 100),
        capsule_part("upper_arm_l", "arm", shoulder_l, elbow_l, 0.040, 2, 101),
        capsule_part("forearm_l", "arm", elbow_l, hand_l, 0.034, 2, 102),
        capsule_part("thigh_l", "leg", hip_l, knee_l, 0.055, 2, 105),
        capsule_part("shin_l", "leg", knee_l, ankle_l, 0.042, 2, 106),
        capsule_part("neck", "head", neck, head, 0.045, 1, 107),
        organic_oval_part(
            "head",
            "head",
            1,
            head.0 - 0.075,
            head.1 - 0.075,
            0.15,
            0.16,
            108,
            0.08,
        ),
        path_part(
            "hair",
            "head",
            0,
            &smooth_closed(
                &[
                    (head.0 - 0.073, head.1 - 0.015),
                    (head.0 - 0.070, head.1 - 0.070),
                    (head.0 - 0.035, head.1 - 0.092),
                    (head.0 + 0.018, head.1 - 0.095),
                    (head.0 + 0.070, head.1 - 0.065),
                    (head.0 + 0.075, head.1 - 0.015),
                    (head.0 + 0.035, head.1 - 0.030),
                    (head.0 - 0.030, head.1 - 0.032),
                ],
                1,
            ),
            109,
        ),
        ellipse_part(
            "ear",
            "head",
            1,
            head.0 - 0.080,
            head.1 + 0.015,
            0.026,
            0.050,
            110,
        ),
        path_part(
            "nose",
            "head",
            0,
            &[
                (head.0 + 0.043, head.1 + 0.035),
                (head.0 + 0.072, head.1 + 0.055),
                (head.0 + 0.043, head.1 + 0.063),
            ],
            111,
        ),
        organic_oval_part(
            "hand_l",
            "hand",
            1,
            hand_l.0 - 0.025,
            hand_l.1 - 0.020,
            0.05,
            0.04,
            110,
            0.08,
        ),
        ellipse_part(
            "eye",
            "head",
            0,
            head.0 + 0.025,
            head.1 - 0.02,
            0.024,
            0.028,
            112,
        ),
    ];
    if sitting {
        parts.push(capsule_part(
            "upper_arm_r",
            "arm",
            shoulder_r,
            elbow_r,
            0.040,
            2,
            112,
        ));
        parts.push(capsule_part(
            "forearm_r",
            "arm",
            elbow_r,
            hand_r,
            0.034,
            2,
            113,
        ));
        parts.push(organic_oval_part(
            "hand_r",
            "hand",
            1,
            hand_r.0 - 0.025,
            hand_r.1 - 0.020,
            0.05,
            0.04,
            114,
            0.08,
        ));
        let hip_r = pose_joint(&joints, "hip_r");
        let knee_r = pose_joint(&joints, "knee_r");
        let ankle_r = pose_joint(&joints, "ankle_r");
        parts.push(capsule_part("thigh_r", "leg", hip_r, knee_r, 0.050, 2, 115));
        parts.push(capsule_part(
            "shin_r", "leg", knee_r, ankle_r, 0.040, 2, 116,
        ));
        parts.push(capsule_part(
            "foot_l",
            "foot",
            ankle_l,
            (ankle_l.0 + 0.065, ankle_l.1 + 0.005),
            0.032,
            1,
            117,
        ));
        parts.push(capsule_part(
            "foot_r",
            "foot",
            ankle_r,
            (ankle_r.0 - 0.065, ankle_r.1 + 0.005),
            0.032,
            1,
            118,
        ));
    } else {
        let hip_r = pose_joint(&joints, "hip_r");
        let knee_r = pose_joint(&joints, "knee_r");
        let ankle_r = pose_joint(&joints, "ankle_r");
        parts.push(capsule_part("thigh_r", "leg", hip_r, knee_r, 0.055, 2, 119));
        parts.push(capsule_part(
            "shin_r", "leg", knee_r, ankle_r, 0.042, 2, 120,
        ));
    }
    parts
}

fn standing_man() -> Vec<IllustrationPart> {
    human_scene_from_skeleton(false)
}

fn seated_man() -> Vec<IllustrationPart> {
    human_scene_from_skeleton(true)
}

/// Clothing is an envelope laid over the anatomical masses. It follows the
/// pose instead of replacing it with a second disconnected mannequin.
fn seated_clothing() -> Vec<IllustrationPart> {
    let jacket = smooth_closed(
        &[
            (0.355, 0.405),
            (0.415, 0.355),
            (0.470, 0.345),
            (0.525, 0.355),
            (0.585, 0.415),
            (0.560, 0.535),
            (0.555, 0.625),
            (0.370, 0.625),
            (0.365, 0.535),
        ],
        2,
    );
    let shirt = smooth_closed(
        &[
            (0.435, 0.365),
            (0.470, 0.395),
            (0.505, 0.365),
            (0.525, 0.505),
            (0.425, 0.505),
        ],
        1,
    );
    let trouser_l = smooth_closed(
        &[
            (0.440, 0.605),
            (0.505, 0.600),
            (0.650, 0.650),
            (0.650, 0.705),
            (0.525, 0.730),
            (0.445, 0.680),
        ],
        1,
    );
    let trouser_r = smooth_closed(
        &[
            (0.445, 0.605),
            (0.380, 0.600),
            (0.270, 0.665),
            (0.275, 0.720),
            (0.395, 0.725),
            (0.465, 0.675),
        ],
        1,
    );
    vec![
        path_part("jacket", "clothing", 2, &jacket, 130),
        path_part("shirt_front", "clothing", 3, &shirt, 131),
        capsule_part(
            "sleeve_l",
            "clothing",
            (0.36, 0.405),
            (0.54, 0.40),
            0.050,
            2,
            132,
        ),
        capsule_part(
            "sleeve_r",
            "clothing",
            (0.56, 0.415),
            (0.64, 0.52),
            0.050,
            2,
            133,
        ),
        path_part("trouser_l", "clothing", 1, &trouser_l, 134),
        path_part("trouser_r", "clothing", 1, &trouser_r, 135),
    ]
}

#[allow(dead_code)]
fn legacy_standing_man() -> Vec<IllustrationPart> {
    let head = smooth_closed(
        &[
            (0.36, 0.24),
            (0.38, 0.15),
            (0.46, 0.12),
            (0.54, 0.15),
            (0.57, 0.21),
            (0.58, 0.24),
            (0.66, 0.27),
            (0.58, 0.30),
            (0.55, 0.34),
            (0.47, 0.37),
            (0.39, 0.34),
            (0.35, 0.28),
        ],
        1,
    );
    let hair = smooth_closed(
        &[
            (0.34, 0.24),
            (0.36, 0.13),
            (0.46, 0.09),
            (0.56, 0.13),
            (0.55, 0.19),
            (0.44, 0.20),
            (0.36, 0.22),
        ],
        1,
    );
    let torso = smooth_closed(
        &[
            (0.40, 0.36),
            (0.32, 0.44),
            (0.30, 0.58),
            (0.34, 0.68),
            (0.52, 0.70),
            (0.56, 0.56),
            (0.54, 0.44),
            (0.48, 0.36),
        ],
        2,
    );
    let arm = smooth_closed(
        &[
            (0.46, 0.42),
            (0.58, 0.36),
            (0.70, 0.34),
            (0.72, 0.39),
            (0.60, 0.43),
            (0.46, 0.50),
        ],
        1,
    );
    let leg_back = smooth_closed(&[(0.34, 0.64), (0.30, 0.84), (0.36, 0.88), (0.40, 0.66)], 1);
    let leg_front = smooth_closed(&[(0.44, 0.66), (0.48, 0.86), (0.55, 0.88), (0.52, 0.66)], 1);
    let brow = [(0.49, 0.188), (0.56, 0.178), (0.56, 0.190), (0.49, 0.200)];
    vec![
        path_part("body", "body", 2, &torso, 50),
        path_part("leg_back", "leg", 2, &leg_back, 51),
        path_part("leg_front", "leg", 2, &leg_front, 52),
        path_part("arm", "arm", 2, &arm, 53),
        path_part("head", "head", 1, &head, 54),
        path_part("hair", "head", 0, &hair, 55),
        ellipse_part("ear", "head", 1, 0.39, 0.22, 0.032, 0.042, 58),
        ellipse_part("eye", "head", 0, 0.50, 0.20, 0.024, 0.028, 57),
        path_part("brow", "head", 0, &brow, 59),
    ]
}

/// Seated human construction: a pelvis/thigh block and bent lower legs make
/// the relation to an armchair readable before face or clothing details.
#[allow(dead_code)]
fn legacy_seated_man() -> Vec<IllustrationPart> {
    let head = smooth_closed(
        &[
            (0.39, 0.25),
            (0.40, 0.16),
            (0.47, 0.12),
            (0.55, 0.15),
            (0.58, 0.23),
            (0.55, 0.31),
            (0.47, 0.35),
            (0.40, 0.32),
        ],
        1,
    );
    let torso = smooth_closed(
        &[
            (0.40, 0.36),
            (0.34, 0.45),
            (0.35, 0.61),
            (0.46, 0.66),
            (0.57, 0.60),
            (0.55, 0.45),
            (0.49, 0.36),
        ],
        1,
    );
    let thigh = smooth_closed(
        &[
            (0.38, 0.58),
            (0.48, 0.57),
            (0.65, 0.63),
            (0.66, 0.71),
            (0.48, 0.72),
            (0.35, 0.66),
        ],
        1,
    );
    let shin = smooth_closed(
        &[
            (0.57, 0.68),
            (0.66, 0.67),
            (0.69, 0.83),
            (0.63, 0.88),
            (0.56, 0.84),
        ],
        1,
    );
    let arm = smooth_closed(
        &[
            (0.48, 0.42),
            (0.58, 0.40),
            (0.68, 0.32),
            (0.72, 0.36),
            (0.62, 0.48),
            (0.51, 0.52),
        ],
        1,
    );
    vec![
        path_part("body", "body", 2, &torso, 70),
        path_part("thigh", "leg", 2, &thigh, 71),
        path_part("shin", "leg", 2, &shin, 72),
        path_part("arm", "arm", 2, &arm, 73),
        path_part("head", "head", 1, &head, 74),
        path_part(
            "hair",
            "head",
            0,
            &smooth_closed(
                &[
                    (0.38, 0.24),
                    (0.39, 0.14),
                    (0.48, 0.09),
                    (0.57, 0.14),
                    (0.55, 0.20),
                    (0.46, 0.19),
                ],
                1,
            ),
            75,
        ),
        ellipse_part("eye", "head", 0, 0.50, 0.20, 0.024, 0.028, 76),
    ]
}

fn armchair_parts() -> Vec<IllustrationPart> {
    vec![
        path_part(
            "armchair_back",
            "furniture",
            2,
            &smooth_closed(
                &[
                    (0.22, 0.36),
                    (0.27, 0.30),
                    (0.65, 0.30),
                    (0.72, 0.38),
                    (0.68, 0.70),
                    (0.25, 0.70),
                ],
                1,
            ),
            80,
        ),
        path_part(
            "armchair_seat",
            "furniture",
            3,
            &smooth_closed(
                &[
                    (0.25, 0.60),
                    (0.68, 0.60),
                    (0.75, 0.68),
                    (0.68, 0.75),
                    (0.23, 0.75),
                    (0.17, 0.68),
                ],
                1,
            ),
            81,
        ),
        path_part(
            "armchair_arm_l",
            "furniture",
            2,
            &smooth_closed(&[(0.18, 0.46), (0.28, 0.43), (0.32, 0.70), (0.22, 0.74)], 1),
            82,
        ),
        path_part(
            "armchair_arm_r",
            "furniture",
            2,
            &smooth_closed(&[(0.64, 0.43), (0.74, 0.46), (0.79, 0.71), (0.69, 0.72)], 1),
            83,
        ),
    ]
}
/// Profile, not a stack of ellipses. Props come from the words in the brief.
fn person_scene(subject: &str) -> Vec<IllustrationPart> {
    let subject = subject.to_ascii_lowercase();
    let garden = subject.contains("jardin")
        || subject.contains("garden")
        || subject.contains("fleur")
        || subject.contains("flower")
        || subject.contains("arbre")
        || subject.contains("tree")
        || subject.contains("luxuri");
    let sitting = subject.contains("assis")
        || subject.contains("banc")
        || subject.contains("bench")
        || subject.contains("fauteuil")
        || subject.contains("armchair")
        || subject.contains("sitting")
        || subject.contains("seated");
    let armchair = subject.contains("fauteuil") || subject.contains("armchair");
    let pipe = subject.contains("pipe") || subject.contains("fum");
    let aged = subject.contains("âgé")
        || subject.contains("vieux")
        || subject.contains("vieillard")
        || subject.contains("elderly");

    let mut parts = vec![
        rect_part("bg_sky", "background", 0, 0.0, 0.0, 1.0, 0.78, 1, false),
        rect_part("bg_lawn", "background", 1, 0.0, 0.74, 1.0, 0.26, 2, false),
    ];
    if garden {
        parts.extend(garden_tree("l_", 0.13, 0.22, 1.15, 10));
        parts.extend(garden_tree("r_", 0.86, 0.16, 0.92, 14));
        for (i, (x, y, s)) in [
            (0.06_f32, 0.78, 0.85),
            (0.15, 0.80, 1.15),
            (0.78, 0.79, 0.95),
            (0.90, 0.77, 1.05),
        ]
        .iter()
        .enumerate()
        {
            parts.extend(bloom(i, *x, *y, *s));
        }
    }
    if sitting {
        if armchair {
            parts.extend(armchair_parts());
        } else {
            parts.push(rect_part(
                "bench_seat",
                "furniture",
                2,
                0.16,
                0.66,
                0.64,
                0.045,
                40,
                true,
            ));
            parts.push(rect_part(
                "bench_back",
                "furniture",
                2,
                0.18,
                0.42,
                0.045,
                0.26,
                41,
                true,
            ));
            parts.push(rect_part(
                "bench_leg",
                "furniture",
                2,
                0.22,
                0.70,
                0.028,
                0.12,
                42,
                true,
            ));
            parts.push(rect_part(
                "bench_leg_r",
                "furniture",
                2,
                0.70,
                0.70,
                0.028,
                0.12,
                43,
                true,
            ));
        }
    }

    parts.extend(if sitting {
        seated_man()
    } else {
        standing_man()
    });
    if sitting {
        parts.extend(seated_clothing());
    }
    if aged {
        let beard = path_part(
            "beard",
            "head",
            0,
            &smooth_closed(
                &[
                    (0.44, 0.28),
                    (0.44, 0.33),
                    (0.49, 0.36),
                    (0.55, 0.33),
                    (0.55, 0.28),
                ],
                1,
            ),
            56,
        );
        parts.push(beard);
    }
    if pipe {
        let stem = [(0.58, 0.305), (0.73, 0.328), (0.73, 0.340), (0.58, 0.318)];
        let bowl = smooth_closed(
            &[
                (0.69, 0.32),
                (0.76, 0.31),
                (0.77, 0.40),
                (0.70, 0.41),
                (0.68, 0.34),
            ],
            1,
        );
        parts.push(path_part("pipe_stem", "prop", 2, &stem, 60));
        parts.push(path_part("pipe_bowl", "prop", 2, &bowl, 61));
    }
    parts
}

/// A sentence that is not a cat, dog, elephant or penguin. Props come from the words.
fn keyword_scene(subject: &str) -> Vec<IllustrationPart> {
    if is_person_subject(subject) {
        return person_scene(subject);
    }
    let mut parts = default_background_for_subject(subject);
    if subject.contains("pipe") || subject.contains("fum") {
        parts.push(rect_part(
            "pipe", "prop", 3, 0.52, 0.30, 0.14, 0.025, 50, true,
        ));
        parts.push(ellipse_part(
            "pipe_bowl",
            "prop",
            3,
            0.64,
            0.27,
            0.05,
            0.05,
            51,
        ));
    }
    if subject.contains("banc") || subject.contains("bench") {
        parts.push(rect_part(
            "bench",
            "furniture",
            4,
            0.22,
            0.68,
            0.56,
            0.06,
            60,
            true,
        ));
        parts.push(rect_part(
            "bench_leg",
            "furniture",
            4,
            0.28,
            0.74,
            0.04,
            0.12,
            61,
            true,
        ));
    }
    if subject.contains("fleur") || subject.contains("flower") {
        parts.push(ellipse_part(
            "flowers", "prop", 2, 0.08, 0.72, 0.16, 0.10, 70,
        ));
    }
    if subject.contains("arbre") || subject.contains("tree") || subject.contains("jardin") {
        parts.push(rect_part(
            "trunk", "prop", 4, 0.78, 0.48, 0.06, 0.32, 80, true,
        ));
        parts.push(ellipse_part(
            "canopy", "prop", 3, 0.68, 0.22, 0.26, 0.28, 81,
        ));
    }
    if parts.iter().all(is_background_part) {
        parts.push(ellipse_part("body", "body", 1, 0.32, 0.32, 0.36, 0.28, 30));
    }
    parts
}

fn needs_puppet_enrichment(parts: &[IllustrationPart], subject: &str) -> bool {
    if !puppet_reads(parts, subject) {
        return true;
    }
    let subject_parts: Vec<_> = parts.iter().filter(|p| !is_background_part(p)).collect();
    stretch_mismatch(subject, &subject_parts)
}

/// A kept drawing must already be a character: head, body, eyes, and for a cat
/// or a dog, ears and legs. A two-point tail is not enough. A sofa painted as
/// the background does not count as furniture.
fn puppet_reads(parts: &[IllustrationPart], subject: &str) -> bool {
    let drawn: Vec<_> = parts.iter().filter(|p| !is_background_part(p)).collect();
    let creatures: Vec<_> = drawn
        .iter()
        .copied()
        .filter(|p| !is_furniture_part(p))
        .collect();
    if creatures.len() < 5 {
        return false;
    }
    let has = |needles: &[&str]| {
        creatures.iter().any(|p| {
            let id = p.id.to_ascii_lowercase();
            let role = p.role.to_ascii_lowercase();
            needles.iter().any(|n| id.contains(n) || role == *n)
        })
    };
    if !has(&["head", "tete", "tête"]) || !has(&["body", "corps"]) || !has(&["eye", "oeil", "œil"])
    {
        return false;
    }
    let cat_or_dog = subject.contains("chat")
        || subject.contains("cat")
        || subject.contains("kitten")
        || subject.contains("chien")
        || subject.contains("dog")
        || subject.contains("puppy");
    if cat_or_dog && (!has(&["ear", "oreille"]) || !has(&["paw", "patte", "leg", "snout"])) {
        return false;
    }
    if subject_has_furniture(subject) && !drawn.iter().copied().any(is_furniture_part) {
        return false;
    }
    if wants_bike(subject)
        && !drawn
            .iter()
            .any(|p| p.id.to_ascii_lowercase().contains("wheel"))
    {
        return false;
    }
    if wants_paper(subject)
        && !drawn
            .iter()
            .any(|p| p.id.to_ascii_lowercase().contains("paper"))
    {
        return false;
    }
    !looks_center_anchored(&creatures)
}

fn stretch_mismatch(subject: &str, subject_parts: &[&IllustrationPart]) -> bool {
    if !subject_wants_stretch(subject) {
        return false;
    }
    let Some(body) = subject_parts.iter().find(|p| {
        let id = p.id.to_ascii_lowercase();
        id.contains("body") || id.contains("corps") || p.role == "body"
    }) else {
        return true;
    };
    let (_, _, w, h) = part_bbox(body);
    w < h * 1.25
}

/// Add a missing sofa, paper or bike without touching the character already drawn.
fn append_missing_props(spec: &mut IllustrationSpec, subject: &str) {
    let (cx, cy) = subject_anchor(&spec.parts);
    let mut extra = Vec::new();
    if subject_has_furniture(subject) && !spec.parts.iter().any(is_furniture_part) {
        extra.extend(furniture_parts(subject, cx, cy));
    }
    if wants_bike(subject)
        && !spec
            .parts
            .iter()
            .any(|p| p.id.to_ascii_lowercase().contains("wheel"))
    {
        extra.extend(bike_parts(cx, cy));
    }
    if wants_paper(subject)
        && !spec
            .parts
            .iter()
            .any(|p| p.id.to_ascii_lowercase().contains("paper"))
    {
        extra.push(paper_part(cx, cy));
    }
    if extra.is_empty() {
        return;
    }
    let mut seen: std::collections::HashSet<String> =
        spec.parts.iter().map(|p| p.id.clone()).collect();
    extra.retain(|p| seen.insert(p.id.clone()));
    let mut parts = extra;
    parts.append(&mut spec.parts);
    spec.parts = parts;
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
        || is_elephant(subject)
        || wants_bike(subject)
        || wants_paper(subject)
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
        || subject.contains("couche")
        || subject.contains("couché")
        || subject.contains("lay")
}

fn subject_wants_jump(subject: &str) -> bool {
    let s = subject.to_ascii_lowercase();
    s.contains("saut")
        || s.contains("jump")
        || s.contains("bond")
        || s.contains("leap")
        || s.contains(" hop")
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
    if is_background_part(p) {
        return false;
    }
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

fn is_scene_backdrop(p: &IllustrationPart) -> bool {
    if !is_background_part(p) {
        return false;
    }
    let id = p.id.to_ascii_lowercase();
    id.starts_with("bg")
        || id.contains("sky")
        || id.contains("ciel")
        || id.contains("floor")
        || id.contains("wall")
        || id.contains("ground")
        || id.contains("sol")
        || id.contains("mer")
        || id.contains("sea")
        || id.contains("sand")
        || id.contains("sable")
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

fn taxonomic_skeleton_present(skeleton: &[IllustrationSkeletonJoint]) -> bool {
    if skeleton.len() < 8 {
        return false;
    }
    let has = |id: &str| skeleton.iter().any(|j| j.id == id);
    // Primary humanoid / quadruped armatures from the taxonomy poser.
    (has("pelvis") && has("chest") && (has("hand_l") || has("paw_fl")))
        || skeleton.iter().any(|j| {
            j.id.starts_with("pipe_") || j.id.starts_with("seat_") || j.id.starts_with("garden_")
        })
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
            (
                min_x,
                min_y,
                (max_x - min_x).max(0.05),
                (max_y - min_y).max(0.05),
            )
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

fn construction_plan_for_subject(subject: &str) -> IllustrationConstructionPlan {
    let is_human = is_person_subject(subject);
    let is_animal = is_species_recipe(subject);
    let is_furniture = subject_has_furniture(subject)
        || subject.contains("fauteuil")
        || subject.contains("armchair");
    let archetype = if is_human {
        IllustrationArchetype::Human
    } else if is_animal {
        IllustrationArchetype::Quadruped
    } else if is_furniture {
        IllustrationArchetype::Furniture
    } else {
        IllustrationArchetype::Subject
    };
    let mut must_read = Vec::new();
    let mut relations = Vec::new();
    if is_human {
        must_read.extend(
            ["tête", "torse", "bras", "jambes"]
                .into_iter()
                .map(|s| s.to_string()),
        );
    }
    if is_animal {
        must_read.extend(
            ["tête", "corps", "oreilles", "pattes"]
                .into_iter()
                .map(|s| s.to_string()),
        );
    }
    if subject.contains("pipe") || subject.contains("fum") {
        must_read.push("pipe visible près de la bouche".into());
        relations.push("main/tête -> pipe".into());
    }
    if subject.contains("jardin") || subject.contains("garden") {
        must_read.push("jardin en arrière-plan".into());
        relations.push("regard -> jardin".into());
    }
    if is_furniture {
        must_read.push("meuble lisible sous le sujet".into());
        relations.push("sujet -> assis/allongé sur meuble".into());
    }
    if subject_wants_jump(subject) {
        relations.push("corps en mouvement -> support".into());
    }
    let action_line = if subject_wants_jump(subject) {
        "j_inverse".into()
    } else if subject.contains("regard") || subject.contains("observe") || subject.contains("look")
    {
        "courbe_ouverte_vers_le_point_focal".into()
    } else if subject.contains("court") || subject.contains("run") || subject.contains("marche") {
        "diagonale".into()
    } else if is_human || is_animal {
        "s_ou_c_organique".into()
    } else {
        "neutre".into()
    };
    let focal_point = if subject.contains("pipe") || subject.contains("fum") {
        "visage_et_pipe".into()
    } else if subject_wants_jump(subject) {
        "contact_sujet_support".into()
    } else if is_human || is_animal {
        "tete_et_silhouette".into()
    } else {
        "objet_principal".into()
    };
    let head_orientation =
        if subject.contains("regard") || subject.contains("observe") || subject.contains("look") {
            "trois_quarts_vers_le_jardin_ou_la_cible".into()
        } else {
            "trois_quarts".into()
        };
    let material_pass = if is_animal && (subject.contains("fourr") || subject.contains("poil")) {
        "volume_puis_touffes_dirigees_par_la_lumiere_et_la_gravite".into()
    } else if is_animal {
        "volume_puis_texture_suggeree".into()
    } else {
        "aplats_puis_accents_de_matiere".into()
    };
    IllustrationConstructionPlan {
        archetype,
        passes: [
            "intention",
            "squelette",
            "volumes",
            "silhouette",
            "détails_identitaires",
            "nettoyage",
            "style",
        ]
        .into_iter()
        .map(|s| s.to_string())
        .collect(),
        must_read,
        relations,
        action_line,
        focal_point,
        value_groups: vec![
            "fond_simplifie_et_moins_contraste".into(),
            "sujet_principal_contraste_moyen_fort".into(),
            "accent_le_plus_fort_sur_le_point_focal".into(),
        ],
        head_orientation,
        silhouette_test: false,
        model_sheet: vec![
            "face".into(),
            "profil".into(),
            "trois_quarts".into(),
            "dos".into(),
            "expression_neutre".into(),
            "pose_principale".into(),
        ],
        material_pass,
        action_line_points: Vec::new(),
        chest_oval: None,
        pelvis_oval: None,
    }
}

/// Build complete redraws for action beats. This is intentionally a redraw
/// from the same construction recipe, not a transform of one raster pose.
fn add_authored_action_drawings(spec: &mut IllustrationSpec, subject: &str, anchor: (f32, f32)) {
    if !subject_wants_jump(subject) || !spec.key_drawings.is_empty() {
        return;
    }
    let background = spec
        .parts
        .iter()
        .filter(|p| is_scene_backdrop(p))
        .cloned()
        .collect::<Vec<_>>();
    let redraw = |id: &str, variant: &str| {
        let mut parts = background.clone();
        parts.extend(puppet_clause(variant, anchor.0, anchor.1, 1.0, ""));
        IllustrationKeyDrawing {
            id: id.into(),
            pose: IllustrationPose::default(),
            parts,
        }
    };
    let settled = subject
        .replace("saute", "")
        .replace("jump", "")
        .replace("bond", "");
    let lying = format!("{settled} allongé");
    spec.key_drawings = vec![
        redraw("anticipate", &settled),
        redraw("airborne", subject),
        redraw("land", &lying),
    ];
}

/// Build a deliberately imperfect, closed contour for a drawn mass.
///
/// This is intentionally different from `ellipse_part`: the contour has a
/// stable asymmetric gesture and is smoothed before it reaches the rasterizer.
/// It gives the composer a useful low-level primitive while keeping the
/// silhouette authored rather than mechanically oval.
fn organic_oval_part(
    id: &str,
    role: &str,
    fill_index: u8,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    seed: u32,
    gesture: f32,
) -> IllustrationPart {
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    let mut points = Vec::with_capacity(12);
    for i in 0..12 {
        let a = i as f32 / 12.0 * std::f32::consts::TAU;
        let phase = (seed % 7) as f32 * 0.17;
        let wobble = 1.0 + gesture * (a * 2.0 + phase).sin();
        let shoulder = if a.sin() < 0.0 {
            1.0 + gesture * 0.35
        } else {
            1.0
        };
        points.push((
            cx + a.cos() * w * 0.5 * wobble,
            cy + a.sin() * h * 0.5 * shoulder * wobble,
        ));
    }
    IllustrationPart {
        id: id.into(),
        role: role.into(),
        fill_index,
        fill: true,
        outline: true,
        seed,
        geometry: IllustrationPartGeometry::Path {
            points: smooth_closed(&points, 2)
                .into_iter()
                .map(|(px, py)| CanvasPoint { x: px, y: py })
                .collect(),
            closed: true,
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

fn is_elephant(subject: &str) -> bool {
    let s = subject.to_ascii_lowercase();
    s.contains("elephant") || s.contains("éléphant") || s.contains("eleph")
}

fn wants_bike(subject: &str) -> bool {
    subject.contains("velo")
        || subject.contains("vélo")
        || subject.contains("bike")
        || subject.contains("bicycle")
        || subject.contains("cycl")
}

fn wants_paper(subject: &str) -> bool {
    subject.contains("journal")
        || subject.contains("newspaper")
        || subject.contains("gazette")
        || subject.contains("magazine")
        || subject.contains("magasine")
        || subject.contains("revue")
}

fn furniture_parts(subject: &str, cx: f32, cy: f32) -> Vec<IllustrationPart> {
    let cushion =
        subject.contains("coussin") || subject.contains("cushion") || subject.contains("pillow");
    if cushion
        && !(subject.contains("canap")
            || subject.contains("sofa")
            || subject.contains("couch")
            || subject.contains("fauteuil"))
    {
        return vec![ellipse_part(
            "cushion",
            "furniture",
            3,
            cx - 0.36,
            cy + 0.06,
            0.72,
            0.34,
            10,
        )];
    }
    vec![
        rect_part(
            "sofa_base",
            "furniture",
            3,
            cx - 0.38,
            cy + 0.02,
            0.76,
            0.28,
            10,
            true,
        ),
        rect_part(
            "sofa_back",
            "furniture",
            3,
            cx - 0.36,
            cy - 0.16,
            0.72,
            0.22,
            11,
            true,
        ),
    ]
}

fn bike_parts(cx: f32, cy: f32) -> Vec<IllustrationPart> {
    vec![
        ellipse_part("wheel_f", "body", 3, cx - 0.28, cy + 0.12, 0.16, 0.16, 20),
        ellipse_part("wheel_r", "body", 3, cx + 0.10, cy + 0.12, 0.16, 0.16, 21),
        rect_part(
            "bike_frame",
            "body",
            3,
            cx - 0.18,
            cy + 0.02,
            0.36,
            0.06,
            22,
            true,
        ),
    ]
}

fn paper_part(cx: f32, cy: f32) -> IllustrationPart {
    rect_part(
        "paper",
        "prop",
        0,
        cx - 0.10,
        cy - 0.02,
        0.22,
        0.16,
        50,
        true,
    )
}

/// A comma opens another character only when that fragment names one.
/// "pattes tendues" stays in the same scene. A single scene keeps the full sentence.
fn split_scene_clauses(subject: &str) -> Vec<String> {
    let mut text = subject.to_string();
    for sep in [" et ", " and ", ";", "\n"] {
        text = text.replace(sep, ",");
    }
    let clauses: Vec<String> = text
        .split(',')
        .map(|s| s.trim())
        .filter(|s| s.len() > 2)
        .map(|s| s.to_string())
        .collect();
    let scenes: Vec<String> = clauses
        .into_iter()
        .filter(|c| clause_opens_scene(c))
        .collect();
    if scenes.len() <= 1 {
        vec![subject.to_string()]
    } else {
        scenes
    }
}

fn clause_opens_scene(clause: &str) -> bool {
    is_known_puppet_subject(&clause.to_ascii_lowercase())
}

fn puppet_parts_for_subject(subject: &str, anchor: (f32, f32)) -> Vec<IllustrationPart> {
    let clauses = split_scene_clauses(subject);
    if clauses.len() <= 1 {
        return puppet_clause(subject, anchor.0, anchor.1, 1.0, "");
    }
    let n = clauses.len().min(3);
    let mut out = Vec::new();
    for (i, clause) in clauses.iter().take(n).enumerate() {
        let cx = (i as f32 + 0.5) / n as f32;
        let scale = if n >= 3 { 0.56 } else { 0.72 };
        out.extend(puppet_clause(clause, cx, 0.48, scale, &format!("c{i}_")));
    }
    out
}

fn puppet_clause(
    subject: &str,
    cx: f32,
    cy: f32,
    scale: f32,
    prefix: &str,
) -> Vec<IllustrationPart> {
    let subject = subject.to_ascii_lowercase();
    let mut parts = Vec::new();

    let is_cat = subject.contains("chat") || subject.contains("cat") || subject.contains("kitten");
    let is_dog = subject.contains("chien") || subject.contains("dog") || subject.contains("puppy");
    let is_penguin = subject.contains("pingouin") || subject.contains("penguin");
    let wants_stretch = subject_wants_stretch(&subject);
    let wants_jump = subject_wants_jump(&subject);
    let has_cushion =
        subject.contains("coussin") || subject.contains("cushion") || subject.contains("pillow");
    let has_sofa = subject.contains("canap")
        || subject.contains("sofa")
        || subject.contains("couch")
        || subject.contains("fauteuil");

    // Furniture sits behind the animal (drawn first = under).
    if is_dog && has_sofa && !wants_stretch {
        let mut parts = sitting_dog_on_sofa(cx, cy);
        stamp_parts(&mut parts, cx, cy, scale, prefix);
        return parts;
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

    if wants_bike(&subject) {
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
        parts.push(rect_part(
            "handlebar",
            "body",
            3,
            cx + 0.08,
            cy - 0.10,
            0.16,
            0.03,
            23,
            true,
        ));
    }

    // Creature mass
    if is_elephant(&subject) {
        parts.extend(elephant_parts(cx, cy));
    } else {
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
        let body_y = if wants_jump && !wants_stretch {
            cy - body_h - 0.16
        } else if has_sofa {
            cy - body_h * 0.05
        } else if has_cushion {
            cy - body_h * 0.20
        } else if wants_stretch {
            cy - body_h * 0.10
        } else {
            cy - body_h * 0.35
        };
        // Creature masses are authored contours, not perfect ovals. Perfect
        // ellipses are cheap to generate but immediately read as a snowman at
        // review size; the reference workflow asks for designed silhouettes.
        parts.push(organic_oval_part(
            "body",
            "body",
            if is_cat || is_dog { 2 } else { 1 },
            body_x,
            body_y,
            body_w,
            body_h,
            30,
            if wants_stretch { 0.10 } else { 0.06 },
        ));

        if is_dog || is_cat {
            if wants_jump && !wants_stretch {
                let y0 = body_y + body_h * 0.55;
                parts.push(path_part(
                    "paw_fl",
                    "leg",
                    2,
                    &[
                        (body_x + 0.04, y0),
                        (body_x + 0.01, y0 - 0.12),
                        (body_x + 0.10, y0 - 0.13),
                    ],
                    44,
                ));
                parts.push(path_part(
                    "paw_fr",
                    "leg",
                    2,
                    &[
                        (body_x + 0.16, y0),
                        (body_x + 0.20, y0 - 0.11),
                        (body_x + 0.28, y0 - 0.12),
                    ],
                    45,
                ));
                parts.push(path_part(
                    "paw_bl",
                    "leg",
                    2,
                    &[
                        (body_x + body_w * 0.55, y0),
                        (body_x + body_w * 0.72, y0 - 0.08),
                        (body_x + body_w * 0.62, y0 + 0.02),
                    ],
                    46,
                ));
                parts.push(path_part(
                    "paw_br",
                    "leg",
                    2,
                    &[
                        (body_x + body_w * 0.72, y0),
                        (body_x + body_w * 0.90, y0 - 0.06),
                        (body_x + body_w * 0.80, y0 + 0.03),
                    ],
                    47,
                ));
            } else {
                let paw_y = body_y + body_h * 0.72;
                let paw_h = 0.07;
                let paw_w = if is_dog { 0.07 } else { 0.06 };
                parts.push(organic_oval_part(
                    "paw_fl",
                    "body",
                    2,
                    body_x + body_w * 0.12,
                    paw_y,
                    paw_w,
                    paw_h,
                    44,
                    0.16,
                ));
                parts.push(organic_oval_part(
                    "paw_fr",
                    "body",
                    2,
                    body_x + body_w * 0.28,
                    paw_y,
                    paw_w,
                    paw_h,
                    45,
                    0.16,
                ));
                parts.push(organic_oval_part(
                    "paw_bl",
                    "body",
                    2,
                    body_x + body_w * 0.58,
                    paw_y,
                    paw_w,
                    paw_h,
                    46,
                    0.16,
                ));
                parts.push(organic_oval_part(
                    "paw_br",
                    "body",
                    2,
                    body_x + body_w * 0.74,
                    paw_y,
                    paw_w,
                    paw_h,
                    47,
                    0.16,
                ));
            }
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
        parts.push(organic_oval_part(
            "head",
            "head",
            if is_cat || is_dog { 2 } else { 1 },
            head_x,
            head_y,
            head_w,
            head_h,
            32,
            if is_cat { 0.10 } else { 0.04 },
        ));

        // Species ears (never give pointed cat ears to dogs).
        if is_cat {
            parts.push(pointed_ear("ear_l", head_x + 0.01, head_y - 0.02, -1.0, 33));
            parts.push(pointed_ear(
                "ear_r",
                head_x + head_w - 0.01,
                head_y - 0.02,
                1.0,
                34,
            ));
        } else if is_dog {
            // Floppy ears hanging beside the head.
            parts.push(organic_oval_part(
                "ear_l",
                "head",
                2,
                head_x - 0.04,
                head_y + head_h * 0.15,
                0.08,
                0.16,
                33,
                0.22,
            ));
            parts.push(organic_oval_part(
                "ear_r",
                "head",
                2,
                head_x + head_w - 0.04,
                head_y + head_h * 0.15,
                0.08,
                0.16,
                34,
                0.22,
            ));
            // Snout
            parts.push(organic_oval_part(
                "snout",
                "head",
                3,
                head_x + head_w * 0.28,
                head_y + head_h * 0.48,
                0.10,
                0.08,
                35,
                0.10,
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
            parts.push(organic_oval_part(
                "tail",
                "body",
                2,
                body_x + body_w * 0.82,
                body_y + body_h * 0.25,
                if is_dog { 0.14 } else { 0.18 },
                if is_dog { 0.10 } else { 0.07 },
                42,
                0.28,
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
    }

    if wants_paper(&subject) {
        parts.push(rect_part(
            "paper",
            "prop",
            0,
            cx - 0.10,
            cy - 0.02,
            0.22,
            0.16,
            50,
            true,
        ));
    }

    stamp_parts(&mut parts, cx, cy, scale, prefix);
    parts
}

fn pointed_ear(id: &str, tip_x: f32, tip_y: f32, dir: f32, seed: u32) -> IllustrationPart {
    path_part(
        id,
        "head",
        2,
        &[
            (tip_x, tip_y),
            (tip_x + dir * 0.015, tip_y + 0.09),
            (tip_x - dir * 0.055, tip_y + 0.07),
        ],
        seed,
    )
}

fn elephant_parts(cx: f32, cy: f32) -> Vec<IllustrationPart> {
    let trunk = smooth_closed(
        &[
            (cx - 0.02, cy - 0.08),
            (cx + 0.04, cy - 0.06),
            (cx + 0.07, cy + 0.02),
            (cx + 0.05, cy + 0.12),
            (cx + 0.01, cy + 0.16),
            (cx - 0.03, cy + 0.10),
            (cx - 0.04, cy + 0.02),
        ],
        2,
    );
    vec![
        ellipse_part("body", "body", 2, cx - 0.20, cy - 0.02, 0.42, 0.28, 30),
        ellipse_part("ear_l", "head", 2, cx - 0.24, cy - 0.30, 0.20, 0.22, 33),
        ellipse_part("ear_r", "head", 2, cx + 0.06, cy - 0.30, 0.20, 0.22, 34),
        ellipse_part("head", "head", 2, cx - 0.10, cy - 0.24, 0.24, 0.20, 32),
        path_part("trunk", "head", 2, &trunk, 36),
        rect_part(
            "leg_fl",
            "body",
            2,
            cx - 0.16,
            cy + 0.18,
            0.07,
            0.16,
            44,
            true,
        ),
        rect_part(
            "leg_fr",
            "body",
            2,
            cx - 0.04,
            cy + 0.18,
            0.07,
            0.16,
            45,
            true,
        ),
        rect_part(
            "leg_bl",
            "body",
            2,
            cx + 0.06,
            cy + 0.18,
            0.07,
            0.16,
            46,
            true,
        ),
        rect_part(
            "leg_br",
            "body",
            2,
            cx + 0.16,
            cy + 0.18,
            0.07,
            0.16,
            47,
            true,
        ),
        ellipse_part("eye_l", "head", 0, cx - 0.04, cy - 0.16, 0.03, 0.03, 40),
        ellipse_part("eye_r", "head", 0, cx + 0.06, cy - 0.16, 0.03, 0.03, 41),
    ]
}

fn stamp_parts(parts: &mut [IllustrationPart], cx: f32, cy: f32, scale: f32, prefix: &str) {
    for part in parts.iter_mut() {
        if !prefix.is_empty() {
            part.id = format!("{prefix}{}", part.id);
        }
        if (scale - 1.0).abs() < 0.01 {
            continue;
        }
        match &mut part.geometry {
            IllustrationPartGeometry::Ellipse { x, y, w, h, .. }
            | IllustrationPartGeometry::Rect { x, y, w, h, .. } => {
                let pcx = *x + *w * 0.5;
                let pcy = *y + *h * 0.5;
                *w *= scale;
                *h *= scale;
                *x = cx + (pcx - cx) * scale - *w * 0.5;
                *y = cy + (pcy - cy) * scale - *h * 0.5;
            }
            IllustrationPartGeometry::Path { points, .. } => {
                for p in points {
                    p.x = cx + (p.x - cx) * scale;
                    p.y = cy + (p.y - cy) * scale;
                }
            }
        }
    }
}

fn silhouette_test_passes(parts: &[IllustrationPart]) -> bool {
    let subject: Vec<_> = parts.iter().filter(|p| !is_background_part(p)).collect();
    let contours = subject
        .iter()
        .filter(|p| matches!(&p.geometry, IllustrationPartGeometry::Path { points, .. } if points.len() >= 8))
        .count();
    let named = subject
        .iter()
        .filter(|p| {
            let id = p.id.to_ascii_lowercase();
            id.contains("body")
                || id.contains("head")
                || id.contains("tail")
                || id.contains("armchair")
                || id.contains("sofa")
        })
        .count();
    // Some valid archetypes (elephant, furniture) use one dominant contour
    // plus several attached volumes; do not reject them for lacking two
    // separate paths.
    contours >= 1 && named >= 2
}

fn skeleton_xy(joints: &[IllustrationSkeletonJoint], id: &str) -> Option<(f32, f32)> {
    joints
        .iter()
        .find(|joint| joint.id == id)
        .map(|joint| (joint.x, joint.y))
}

fn cylinder_volume(
    joints: &[IllustrationSkeletonJoint],
    id: &str,
    from: &str,
    to: &str,
    radius: f32,
) -> Option<IllustrationVolume> {
    let (ax, ay) = skeleton_xy(joints, from)?;
    let (bx, by) = skeleton_xy(joints, to)?;
    Some(IllustrationVolume {
        id: id.into(),
        kind: "cylinder".into(),
        x: ax.min(bx) - radius,
        y: ay.min(by) - radius,
        w: (ax - bx).abs() + radius * 2.0,
        h: (ay - by).abs() + radius * 2.0,
        rotation: (by - ay).atan2(bx - ax),
    })
}

fn human_volumes_from_skeleton(joints: &[IllustrationSkeletonJoint]) -> Vec<IllustrationVolume> {
    let mut volumes = Vec::new();
    if let Some((x, y)) = skeleton_xy(joints, "chest") {
        volumes.push(IllustrationVolume {
            id: "rib_cage".into(),
            kind: "rib_cage".into(),
            x: x - 0.12,
            y: y - 0.12,
            w: 0.24,
            h: 0.25,
            rotation: 0.0,
        });
    }
    if let Some((x, y)) = skeleton_xy(joints, "pelvis") {
        volumes.push(IllustrationVolume {
            id: "pelvis".into(),
            kind: "pelvis".into(),
            x: x - 0.11,
            y: y - 0.07,
            w: 0.22,
            h: 0.14,
            rotation: 0.0,
        });
    }
    if let Some((x, y)) = skeleton_xy(joints, "head") {
        volumes.push(IllustrationVolume {
            id: "head".into(),
            kind: "head_sphere".into(),
            x: x - 0.075,
            y: y - 0.08,
            w: 0.15,
            h: 0.16,
            rotation: 0.0,
        });
    }
    for (id, from, to, radius) in [
        ("upper_arm_l", "shoulder_l", "elbow_l", 0.045),
        ("forearm_l", "elbow_l", "hand_l", 0.040),
        ("upper_arm_r", "shoulder_r", "elbow_r", 0.045),
        ("forearm_r", "elbow_r", "hand_r", 0.040),
        ("thigh_l", "hip_l", "knee_l", 0.060),
        ("shin_l", "knee_l", "ankle_l", 0.045),
        ("thigh_r", "hip_r", "knee_r", 0.060),
        ("shin_r", "knee_r", "ankle_r", 0.045),
    ] {
        if let Some(volume) = cylinder_volume(joints, id, from, to, radius) {
            volumes.push(volume);
        }
    }
    volumes
}

fn ellipse_path(x: f32, y: f32, w: f32, h: f32, rotation: f32) -> Vec<CanvasPoint> {
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    let (s, c) = rotation.sin_cos();
    (0..12)
        .map(|i| {
            let t = i as f32 / 12.0 * std::f32::consts::TAU;
            let lx = t.cos() * w * 0.5;
            let ly = t.sin() * h * 0.5;
            CanvasPoint {
                x: cx + lx * c - ly * s,
                y: cy + lx * s + ly * c,
            }
        })
        .collect()
}

fn contours_from_volumes(volumes: &[IllustrationVolume]) -> Vec<IllustrationPart> {
    volumes
        .iter()
        .enumerate()
        .map(|(i, volume)| IllustrationPart {
            id: format!("contour_{}", volume.id),
            role: "contour".into(),
            fill_index: 0,
            fill: false,
            outline: true,
            seed: i as u32,
            geometry: IllustrationPartGeometry::Path {
                points: ellipse_path(volume.x, volume.y, volume.w, volume.h, volume.rotation),
                closed: true,
            },
        })
        .collect()
}

/// Fill only the layers that belong to the current construction phase.
/// Later layers stay empty so the panel can show the drawing being built.
fn stage_construction_phase(spec: &mut IllustrationSpec) {
    let authored_skeleton = !spec.skeleton.is_empty();
    if !authored_skeleton {
        let _ = crate::apply_taxonomy_skeleton(spec);
        ensure_construction_passes(spec);
    }
    let human = is_person_subject(&spec.brief.subject.to_ascii_lowercase());
    match spec.construction_phase {
        IllustrationConstructionPhase::Skeleton => {
            if !authored_skeleton {
                spec.volumes.clear();
                spec.contours.clear();
                spec.details.clear();
                spec.parts.retain(is_background_part);
            }
        }
        IllustrationConstructionPhase::Volumes => {
            if spec.volumes.is_empty() && human {
                spec.volumes = human_volumes_from_skeleton(&spec.skeleton);
            }
            if !authored_skeleton {
                spec.contours.clear();
                spec.details.clear();
                spec.parts.retain(is_background_part);
            }
        }
        IllustrationConstructionPhase::Contours | IllustrationConstructionPhase::Details => {
            if spec.volumes.is_empty() && human {
                spec.volumes = human_volumes_from_skeleton(&spec.skeleton);
            }
            if spec.contours.is_empty() {
                spec.contours = contours_from_volumes(&spec.volumes);
            }
            if !authored_skeleton && spec.construction_phase == IllustrationConstructionPhase::Contours
            {
                spec.details.clear();
            }
            if !authored_skeleton {
                spec.parts.retain(is_background_part);
            }
        }
        IllustrationConstructionPhase::Final => {}
    }
}

fn ensure_construction_passes(spec: &mut IllustrationSpec) {
    let subject = spec.brief.subject.to_ascii_lowercase();
    let human = is_person_subject(&subject);
    let known_species = is_species_recipe(&subject);
    if spec.skeleton.is_empty() && (human || known_species) {
        let anchor = subject_anchor(&spec.parts);
        let sitting = subject.contains("assis")
            || subject.contains("assise")
            || subject.contains("sitting")
            || subject.contains("seated")
            || subject.contains("fauteuil");
        let joints: Vec<PoseJoint> = if human {
            human_pose_joints(sitting)
        } else {
            vec![
                PoseJoint {
                    id: "pelvis",
                    parent: None,
                    x: anchor.0 + 0.08,
                    y: anchor.1 + 0.04,
                    radius: 0.04,
                },
                PoseJoint {
                    id: "chest",
                    parent: Some("pelvis"),
                    x: anchor.0,
                    y: anchor.1,
                    radius: 0.055,
                },
                PoseJoint {
                    id: "neck",
                    parent: Some("chest"),
                    x: anchor.0 - 0.05,
                    y: anchor.1 - 0.08,
                    radius: 0.025,
                },
                PoseJoint {
                    id: "head",
                    parent: Some("neck"),
                    x: anchor.0 - 0.08,
                    y: anchor.1 - 0.16,
                    radius: 0.065,
                },
                PoseJoint {
                    id: "paw_front",
                    parent: Some("chest"),
                    x: anchor.0 - 0.16,
                    y: anchor.1 + 0.11,
                    radius: 0.022,
                },
                PoseJoint {
                    id: "paw_back",
                    parent: Some("pelvis"),
                    x: anchor.0 + 0.20,
                    y: anchor.1 + 0.12,
                    radius: 0.022,
                },
                PoseJoint {
                    id: "tail_root",
                    parent: Some("pelvis"),
                    x: anchor.0 + 0.24,
                    y: anchor.1,
                    radius: 0.02,
                },
            ]
        };
        spec.skeleton = joints
            .into_iter()
            .map(|joint| IllustrationSkeletonJoint {
                id: joint.id.into(),
                parent: joint.parent.map(str::to_string),
                x: joint.x,
                y: joint.y,
                radius: joint.radius,
                ..Default::default()
            })
            .collect();
    }
    if spec.construction.action_line_points.is_empty() {
        let ids = ["head", "chest", "pelvis", "knee_l", "ankle_l"];
        spec.construction.action_line_points = ids
            .iter()
            .filter_map(|id| {
                spec.skeleton
                    .iter()
                    .find(|joint| joint.id == *id)
                    .map(|joint| CanvasPoint {
                        x: joint.x,
                        y: joint.y,
                    })
            })
            .collect();
    }
    if spec.construction.chest_oval.is_none() {
        if let Some(joint) = spec.skeleton.iter().find(|joint| joint.id == "chest") {
            spec.construction.chest_oval = Some(IllustrationConstructionOval {
                cx: joint.x,
                cy: joint.y,
                w: if human { 0.22 } else { 0.28 },
                h: if human { 0.24 } else { 0.18 },
                rotation: 0.0,
            });
        }
    }
    if spec.construction.pelvis_oval.is_none() {
        if let Some(joint) = spec.skeleton.iter().find(|joint| joint.id == "pelvis") {
            spec.construction.pelvis_oval = Some(IllustrationConstructionOval {
                cx: joint.x,
                cy: joint.y,
                w: if human { 0.20 } else { 0.24 },
                h: if human { 0.14 } else { 0.15 },
                rotation: 0.0,
            });
        }
    }
    if spec.volumes.is_empty() {
        if human {
            spec.volumes = human_volumes_from_skeleton(&spec.skeleton);
        } else {
            spec.volumes = spec
                .parts
                .iter()
                .filter(|p| !is_background_part(p))
                .filter_map(|part| {
                    let (x, y, w, h) = part_bbox(part);
                    if w <= 0.0 || h <= 0.0 {
                        return None;
                    }
                    let id = part.id.to_ascii_lowercase();
                    let kind = if id.contains("body") {
                        "organic_mass"
                    } else if id.contains("head") {
                        "head_sphere"
                    } else if part.role == "leg" || part.role == "arm" {
                        "cylinder"
                    } else {
                        "organic"
                    };
                    Some(IllustrationVolume {
                        id: part.id.clone(),
                        kind: kind.into(),
                        x,
                        y,
                        w,
                        h,
                        rotation: 0.0,
                    })
                })
                .collect();
        }
    }
    if spec.contours.is_empty() {
        spec.contours = spec
            .parts
            .iter()
            .filter(|p| matches!(&p.geometry, IllustrationPartGeometry::Path { .. }))
            .cloned()
            .collect();
    }
    if spec.details.is_empty() {
        spec.details = spec
            .parts
            .iter()
            .filter(|p| {
                let id = p.id.to_ascii_lowercase();
                id.contains("eye")
                    || id.contains("ear")
                    || id.contains("pipe")
                    || id.contains("beard")
                    || id.contains("paper")
                    || id.contains("flower")
                    || id.contains("handle")
            })
            .cloned()
            .collect();
    }
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
    ensure_construction_passes(spec);
    spec.construction.silhouette_test = silhouette_test_passes(&spec.parts);
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
    let parts = doc.spec.as_ref().map(|s| s.parts.len()).unwrap_or(0);
    let lock = doc.lock.as_ref().map(|l| l.holder.as_str()).unwrap_or("-");
    format!(
        "illust rev={} look={} palette={} subject={:?} parts={} lock={} png={} sheet={} model_sheet={}",
        doc.revision,
        look,
        pal,
        doc.brief.subject,
        parts,
        lock,
        doc.last_png.as_deref().unwrap_or("-"),
        doc.last_sheet_png.as_deref().unwrap_or("-"),
        doc.last_model_sheet_png.as_deref().unwrap_or("-"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_archetype_does_not_reject_the_spec() {
        let spec: IllustrationSpec = serde_json::from_str(
            r#"{"construction":{"archetype":"Vieil homme assis"}}"#,
        )
        .unwrap();
        assert_eq!(spec.construction.archetype, IllustrationArchetype::Other);
        let known: IllustrationSpec = serde_json::from_str(
            r#"{"construction":{"archetype":"human"}}"#,
        )
        .unwrap();
        assert_eq!(known.construction.archetype, IllustrationArchetype::Human);
    }

    #[test]
    fn authored_construction_survives_preview_enrichment() {
        let mut spec = IllustrationSpec::default();
        spec.brief.subject = "un jardinier assis fumant une pipe".into();
        spec.construction_phase = IllustrationConstructionPhase::Skeleton;
        spec.skeleton.push(IllustrationSkeletonJoint {
            id: "elbow".into(),
            x: 0.7,
            y: 0.6,
            ..Default::default()
        });
        spec.construction.relations.push("hand holds pipe".into());
        let before = serde_json::to_value(&spec).unwrap();
        enrich_illustration_puppet(&mut spec);
        assert_eq!(serde_json::to_value(&spec).unwrap(), before);
        assert!(spec.parts.is_empty(), "preview must not invent a finished character");
    }

    #[test]
    fn empty_passes_stage_pose_then_volumes_without_a_finished_puppet() {
        let subject = "vieux jardinier assis dans un fauteuil, fumant la pipe";
        let mut skeleton = IllustrationSpec::default();
        skeleton.brief.subject = subject.into();
        skeleton.construction_phase = IllustrationConstructionPhase::Skeleton;
        enrich_illustration_puppet(&mut skeleton);
        assert!(
            skeleton.skeleton.iter().any(|j| j.id == "pelvis"),
            "skeleton pass must pose the figure"
        );
        assert!(skeleton.volumes.is_empty());
        assert!(
            skeleton.parts.iter().all(is_background_part),
            "skeleton pass must not dump a dressed character"
        );

        let mut volumes = skeleton.clone();
        volumes.construction_phase = IllustrationConstructionPhase::Volumes;
        volumes.volumes.clear();
        enrich_illustration_puppet(&mut volumes);
        assert!(volumes.skeleton.iter().any(|j| j.id == "pelvis"));
        assert!(
            volumes.volumes.iter().any(|v| v.kind == "rib_cage"),
            "volumes pass must add masses on the posed skeleton"
        );
        assert!(volumes.parts.iter().all(is_background_part));
    }

    #[test]
    fn review_rejects_unfinished_pass_even_with_renderable_parts() {
        let mut spec = IllustrationSpec::default();
        spec.brief.subject = "un jardinier assis".into();
        enrich_illustration_puppet(&mut spec);
        spec.construction_phase = IllustrationConstructionPhase::Volumes;
        let doc = IllustrationDoc {
            brief: spec.brief.clone(),
            spec: Some(spec),
            ..Default::default()
        };
        assert!(review_illustration(&doc)
            .issues
            .iter()
            .any(|issue| issue.kind == "unfinished_construction" && issue.severity == "error"));
    }

    #[test]
    fn bound_contour_tracks_pose_in_all_drawing_passes() {
        let mut spec = IllustrationSpec::default();
        spec.skeleton = vec![
            IllustrationSkeletonJoint {
                id: "elbow".into(),
                x: 0.4,
                y: 0.5,
                ..Default::default()
            },
            IllustrationSkeletonJoint {
                id: "wrist".into(),
                x: 0.6,
                y: 0.5,
                ..Default::default()
            },
        ];
        let sleeve = path_part(
            "sleeve",
            "clothing",
            1,
            &[(0.0, -0.1), (1.0, -0.1), (1.0, 0.1), (0.0, 0.1)],
            1,
        );
        spec.parts = vec![sleeve.clone()];
        spec.contours = vec![sleeve.clone()];
        spec.details = vec![sleeve];
        spec.joint_bindings
            .insert("sleeve".into(), ["elbow".into(), "wrist".into()]);
        spec.volumes.push(IllustrationVolume {
            id: "sleeve".into(),
            kind: "cylinder".into(),
            x: 0.0,
            y: -0.1,
            w: 1.0,
            h: 0.2,
            rotation: 0.0,
        });
        let horizontal = spec.resolve_joint_bindings().unwrap();
        spec.skeleton[1].x = 0.4;
        spec.skeleton[1].y = 0.3;
        let vertical = spec.resolve_joint_bindings().unwrap();
        assert_ne!(horizontal.parts, vertical.parts);
        assert_eq!(vertical.parts, vertical.contours);
        assert_eq!(vertical.parts, vertical.details);
        let mass = &vertical.volumes[0];
        assert!((mass.x + mass.w * 0.5 - 0.4).abs() < 0.00001);
        assert!((mass.y + mass.h * 0.5 - 0.4).abs() < 0.00001);
        assert!((mass.w - 0.2).abs() < 0.00001);
        assert!((mass.rotation + std::f32::consts::FRAC_PI_2).abs() < 0.00001);
        let IllustrationPartGeometry::Path { points, .. } = &vertical.parts[0].geometry else {
            panic!()
        };
        assert!((points[1].x - 0.38).abs() < 0.00001);
        assert!((points[1].y - 0.30).abs() < 0.00001);
        assert_eq!(vertical.resolve_joint_bindings().unwrap(), vertical);
        assert!(!spec.joint_bindings.is_empty(), "source stays editable");
        spec.skeleton.pop();
        assert!(spec
            .resolve_joint_bindings()
            .unwrap_err()
            .contains("missing joint"));
    }

    #[test]
    fn contact_moves_hand_to_target_preserving_both_bone_lengths() {
        let mut spec = IllustrationSpec::default();
        spec.skeleton = [
            ("shoulder", 0.4, 0.4),
            ("elbow", 0.4, 0.6),
            ("hand", 0.6, 0.6),
            ("grip", 0.65, 0.35),
        ]
        .into_iter()
        .map(|(id, x, y)| IllustrationSkeletonJoint {
            id: id.into(),
            x,
            y,
            ..Default::default()
        })
        .collect();
        spec.contacts.insert(
            "pipe".into(),
            [
                "shoulder".into(),
                "elbow".into(),
                "hand".into(),
                "grip".into(),
            ],
        );
        let solved = spec.resolve_joint_bindings().unwrap();
        let j = &solved.skeleton;
        assert_eq!((j[2].x, j[2].y), (j[3].x, j[3].y));
        assert!(((j[1].x - j[0].x).hypot(j[1].y - j[0].y) - 0.2).abs() < 0.00001);
        assert!(((j[2].x - j[1].x).hypot(j[2].y - j[1].y) - 0.2).abs() < 0.00001);
        assert_eq!(solved.resolve_joint_bindings().unwrap(), solved);
        spec.skeleton[3].x = 0.95;
        assert!(spec
            .resolve_joint_bindings()
            .unwrap_err()
            .contains("unreachable"));
    }

    #[test]
    fn invalid_skeleton_topology_is_rejected_before_rendering() {
        let mut spec = IllustrationSpec::default();
        spec.skeleton = vec![IllustrationSkeletonJoint {
            id: "head".into(),
            parent: Some("missing".into()),
            ..Default::default()
        }];
        assert!(spec
            .resolve_joint_bindings()
            .unwrap_err()
            .contains("missing parent"));
        spec.skeleton[0].parent = Some("head".into());
        assert!(spec.resolve_joint_bindings().unwrap_err().contains("cycle"));
        spec.skeleton[0].parent = None;
        spec.skeleton.push(spec.skeleton[0].clone());
        assert!(spec
            .resolve_joint_bindings()
            .unwrap_err()
            .contains("duplicate"));
    }

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
        assert!(
            !spec.skeleton.is_empty(),
            "taxonomy must place a carnivore skeleton instead of expanding a snowman recipe"
        );
        assert!(spec.skeleton.iter().any(|j| j.id.starts_with("paw_")));
        assert!(
            spec.parts
                .iter()
                .any(|p| p.id.contains("cushion") || is_furniture_part(p)),
            "cushion/furniture prop must survive or be appended"
        );
    }

    #[test]
    fn construction_plan_carries_identity_and_relations() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "un vieux jardinier fumant la pipe dans un fauteuil regardant son jardin"
                    .into(),
                ..Default::default()
            },
            ..Default::default()
        };
        enrich_illustration_puppet(&mut spec);
        assert_eq!(spec.construction.archetype, IllustrationArchetype::Human);
        assert!(spec.construction.passes.contains(&"squelette".into()));
        assert!(spec
            .construction
            .must_read
            .iter()
            .any(|x| x.contains("pipe")));
        assert!(spec
            .construction
            .relations
            .iter()
            .any(|x| x.contains("jardin")));
        assert!(spec.parts.iter().any(|p| p.id == "armchair_back"));
        assert!(spec.parts.iter().any(|p| p.id == "pipe_bowl"));
    }

    #[test]
    fn jumping_cat_gets_complete_redrawn_action_keys() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "un chat saute sur un canapé et se couche".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        enrich_illustration_puppet(&mut spec);
        assert!(
            taxonomic_skeleton_present(&spec.skeleton),
            "taxonomy armature replaces recipe key-drawings for a known cat"
        );
        assert!(spec.skeleton.iter().any(|j| j.id.starts_with("paw_")));
        assert!(
            spec.parts
                .iter()
                .any(|p| p.id.starts_with("sofa_") || is_furniture_part(p)),
            "sofa prop still appended for the brief"
        );
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
        assert!(
            !spec.skeleton.is_empty(),
            "taxonomy must place a dog quadruped skeleton"
        );
        assert!(spec.skeleton.iter().any(|j| j.id.starts_with("paw_")));
        assert!(
            spec.parts
                .iter()
                .any(|p| p.id.starts_with("sofa_") || is_furniture_part(p)),
            "sofa prop must be appended for the brief"
        );
    }

    #[test]
    fn animal_recipe_uses_authored_contours_for_main_masses() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "un chat sur un coussin".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        enrich_illustration_puppet(&mut spec);
        assert!(
            taxonomic_skeleton_present(&spec.skeleton),
            "taxonomy armature replaces the old ellipse recipe for known animals"
        );
        assert!(
            spec.parts.iter().any(is_furniture_part)
                || spec
                    .parts
                    .iter()
                    .any(|p| p.id.contains("cushion") || p.id.contains("coussin")),
            "cushion prop should still be present for dressing"
        );
    }

    #[test]
    fn enrich_splits_elephant_cat_bike_and_dog_paper() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "un elephant qui lit un journal, un chat qui fait du velo et un chien qui lit un magasine"
                    .into(),
                ..Default::default()
            },
            ..Default::default()
        };
        enrich_illustration_puppet(&mut spec);
        let ids: Vec<&str> = spec.parts.iter().map(|p| p.id.as_str()).collect();
        assert!(ids
            .iter()
            .any(|id| id.starts_with("c0_") && id.contains("trunk")));
        assert!(ids
            .iter()
            .any(|id| id.starts_with("c1_") && id.contains("wheel")));
        assert!(ids
            .iter()
            .any(|id| id.starts_with("c1_") && id.contains("ear_l")));
        assert!(ids
            .iter()
            .any(|id| id.starts_with("c2_") && id.contains("snout")));
        assert!(ids.iter().filter(|id| id.ends_with("paper")).count() >= 2);
        let trunk = spec.parts.iter().find(|p| p.id.contains("trunk")).unwrap();
        let ear = spec
            .parts
            .iter()
            .find(|p| p.id.starts_with("c0_") && p.id.contains("ear_l"))
            .unwrap();
        let (_, _, ew, eh) = part_bbox(ear);
        assert!(
            ew > 0.08 && eh > 0.08,
            "elephant ears should be large, got {ew}x{eh}"
        );
        assert!(matches!(
            trunk.geometry,
            IllustrationPartGeometry::Path { .. }
        ));
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
        assert!(
            taxonomic_skeleton_present(&spec.skeleton),
            "taxonomy must keep a cat armature instead of expanding the snowman"
        );
        assert!(
            spec.parts.iter().any(|p| p.id == "body" || p.id == "head"),
            "agent ellipses may remain until dressed"
        );
        assert!(
            !spec.parts.iter().any(|p| p.id == "ear_l"),
            "species recipe must not wipe in ears over a taxonomic skeleton"
        );
        assert!(spec
            .parts
            .iter()
            .any(|p| p.id == "cushion" || p.id.contains("coussin")));
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
            !r.issues
                .iter()
                .any(|i| i.kind == "ellipse_only" && i.severity == "error"),
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
        let doodle: IllustrationLook = serde_json::from_str("\"doodle\"").unwrap();
        assert_eq!(doodle, IllustrationLook::Doodle);
    }

    #[test]
    fn enrich_keeps_jumping_cat_despite_comma_anchor() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "un chat qui saute sur un canapé".into(),
                anchor: "chat en plein saut, pattes tendues, canapé en bas de planche".into(),
                ..Default::default()
            },
            parts: vec![
                rect_part("bg", "background", 0, 0.0, 0.0, 1.0, 1.0, 1, false),
                rect_part("sofa", "furniture", 3, 0.15, 0.62, 0.7, 0.22, 2, true),
                ellipse_part("body", "body", 2, 0.28, 0.22, 0.28, 0.2, 3),
                ellipse_part("head", "head", 2, 0.36, 0.08, 0.16, 0.14, 4),
                path_part(
                    "paw_fl",
                    "leg",
                    2,
                    &[(0.30, 0.40), (0.34, 0.28), (0.38, 0.40)],
                    5,
                ),
                path_part(
                    "tail",
                    "body",
                    2,
                    &[(0.50, 0.30), (0.62, 0.22), (0.58, 0.36)],
                    6,
                ),
                ellipse_part("eye_l", "head", 0, 0.40, 0.12, 0.03, 0.03, 7),
                ellipse_part("ear_l", "head", 2, 0.34, 0.02, 0.05, 0.08, 8),
                ellipse_part("ear_r", "head", 2, 0.46, 0.02, 0.05, 0.08, 9),
            ],
            ..Default::default()
        };
        let before: Vec<String> = spec.parts.iter().map(|p| p.id.clone()).collect();
        enrich_illustration_puppet(&mut spec);
        for id in &before {
            assert!(
                spec.parts.iter().any(|p| p.id == *id),
                "enrichment dropped {id}"
            );
        }
        assert!(!spec.parts.iter().any(|p| p.id.starts_with("c1_")));
        assert!(!spec.parts.iter().any(|p| p.id.starts_with("c2_")));
    }

    #[test]
    fn enrich_adds_sofa_without_replacing_the_cat() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "un chat qui saute sur un canapé".into(),
                ..Default::default()
            },
            parts: vec![
                ellipse_part("body", "body", 2, 0.28, 0.22, 0.28, 0.2, 3),
                ellipse_part("head", "head", 2, 0.36, 0.08, 0.16, 0.14, 4),
                path_part(
                    "paw_fl",
                    "leg",
                    2,
                    &[(0.30, 0.40), (0.34, 0.28), (0.38, 0.40)],
                    5,
                ),
            ],
            ..Default::default()
        };
        enrich_illustration_puppet(&mut spec);
        assert!(spec.parts.iter().any(|p| p.id == "head"));
        assert!(spec.parts.iter().any(|p| p.id.starts_with("sofa_")));
        assert!(!spec.parts.iter().any(|p| p.id.starts_with("c0_")));
    }

    #[test]
    fn empty_jump_recipe_lifts_the_body() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "un chat qui saute".into(),
                ..Default::default()
            },
            parts: vec![],
            ..Default::default()
        };
        enrich_illustration_puppet(&mut spec);
        assert!(
            taxonomic_skeleton_present(&spec.skeleton),
            "jumping cat uses taxonomy armature; agent dresses the leap"
        );
        assert!(spec.skeleton.iter().any(|j| j.id == "paw_fl"));
    }

    #[test]
    fn enrich_replaces_two_ellipses_named_as_a_jumping_cat() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "un chat en plein saut sur un canapé".into(),
                engine: IllustrationEngine::Paper,
                ..Default::default()
            },
            parts: vec![
                rect_part("canapé", "background", 0, 0.1, 0.4, 0.8, 0.5, 1, false),
                ellipse_part("chat_corps", "body", 1, 0.4, 0.3, 0.3, 0.2, 2),
                ellipse_part("chat_tete", "head", 2, 0.65, 0.2, 0.15, 0.15, 3),
                path_part("chat_queue", "tail", 3, &[(0.35, 0.35), (0.20, 0.45)], 4),
                rect_part("sol", "ground", 1, 0.0, 0.9, 1.0, 0.1, 5, false),
            ],
            ..Default::default()
        };
        enrich_illustration_puppet(&mut spec);
        assert!(taxonomic_skeleton_present(&spec.skeleton));
        assert!(
            spec.parts
                .iter()
                .any(|p| p.id.starts_with("sofa_") || is_furniture_part(p)),
            "sofa prop from brief"
        );
        assert!(
            !spec.parts.iter().any(|p| p.id == "ear_l"),
            "must not replace agent parts with a recipe puppet over taxonomy"
        );
    }

    #[test]
    fn enrich_keeps_a_man_smoking_in_a_garden() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "un homme âgé assis sur un banc, fumant une pipe, dans un jardin avec des fleurs et des arbres".into(),
                look: IllustrationLook::Pencil,
                ..Default::default()
            },
            parts: vec![
                ellipse_part("body", "body", 1, 0.5, 0.6, 0.3, 0.5, 1),
                ellipse_part("head", "head", 1, 0.5, 0.3, 0.15, 0.15, 2),
                rect_part("pipe", "pipe", 2, 0.55, 0.35, 0.1, 0.03, 3, true),
                rect_part("bench", "bench", 3, 0.3, 0.8, 0.4, 0.1, 4, true),
                ellipse_part("flowers", "flowers", 2, 0.2, 0.85, 0.2, 0.1, 5),
                ellipse_part("tree", "tree", 3, 0.8, 0.4, 0.2, 0.6, 6),
            ],
            ..Default::default()
        };
        enrich_illustration_puppet(&mut spec);
        assert!(taxonomic_skeleton_present(&spec.skeleton));
        assert!(spec.skeleton.iter().any(|j| j.id == "hand_r"));
        assert!(spec.skeleton.iter().any(|j| j.id.starts_with("pipe_")));
        assert!(spec
            .skeleton
            .iter()
            .any(|j| j.id.starts_with("seat_") || j.id.starts_with("garden_")));
        assert!(spec.contacts.contains_key("hold_pipe"));
        assert!(
            !spec.parts.iter().any(|p| p.id == "ear_l"),
            "a person must not become the generic animal"
        );
    }

    #[test]
    fn enrich_fills_taxonomy_skeleton_for_gardener() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "un jardinier assis sur un fauteuil, fumant une pipe dans un jardin"
                    .into(),
                look: IllustrationLook::Pencil,
                ..Default::default()
            },
            ..Default::default()
        };
        enrich_illustration_puppet(&mut spec);
        assert!(
            !spec.skeleton.is_empty(),
            "taxonomy must place a sitting human skeleton"
        );
        assert!(
            spec.skeleton.iter().any(|j| j.id == "pelvis"),
            "human skeleton needs a pelvis root"
        );
        assert!(
            spec.skeleton.iter().any(|j| j.id == "hand_l"),
            "sitting pose needs reachable hands"
        );
    }

    #[test]
    fn enrich_fills_taxonomy_skeleton_for_cat() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "un chat allongé sur un coussin".into(),
                look: IllustrationLook::Pencil,
                ..Default::default()
            },
            ..Default::default()
        };
        enrich_illustration_puppet(&mut spec);
        assert!(
            !spec.skeleton.is_empty(),
            "taxonomy must place a carnivore quadruped skeleton"
        );
        assert!(spec.skeleton.iter().any(|j| j.id == "pelvis"));
        assert!(spec.skeleton.iter().any(|j| j.id.starts_with("paw_")));
    }

    #[test]
    fn enrich_ood_subject_leaves_skeleton_empty() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "un ornithorynque quantique sur un trampoline en plasma".into(),
                look: IllustrationLook::Pencil,
                ..Default::default()
            },
            ..Default::default()
        };
        enrich_illustration_puppet(&mut spec);
        assert!(
            spec.skeleton.is_empty(),
            "OOD subject must abstain instead of inventing a ghost skeleton"
        );
    }

    #[test]
    fn review_flags_skeleton_undressed_ellipse_puppet() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "un jardinier assis sur un fauteuil".into(),
                ..Default::default()
            },
            parts: vec![
                ellipse_part("body", "body", 1, 0.4, 0.4, 0.3, 0.4, 1),
                ellipse_part("head", "head", 1, 0.45, 0.2, 0.15, 0.15, 2),
            ],
            ..Default::default()
        };
        enrich_illustration_puppet(&mut spec);
        assert!(taxonomic_skeleton_present(&spec.skeleton));
        let doc = IllustrationDoc {
            brief: spec.brief.clone(),
            last_sheet_png: Some("/downloads/illustration/sheet.png".into()),
            last_model_sheet_png: Some("/downloads/illustration/model.png".into()),
            spec: Some(spec),
            ..Default::default()
        };
        let r = review_illustration(&doc);
        assert!(
            r.issues.iter().any(|i| i.kind == "skeleton_undressed"),
            "ellipse dressing on a taxonomic skeleton must fail review: {:?}",
            r.issues
        );
    }

    #[test]
    fn review_flags_missing_sofa_and_extra_puppets() {
        let doc = IllustrationDoc {
            brief: IllustrationBrief {
                subject: "un chat sur un canapé".into(),
                ..Default::default()
            },
            last_sheet_png: Some("/downloads/illustration/sheet.png".into()),
            spec: Some(IllustrationSpec {
                parts: vec![
                    ellipse_part("body", "body", 1, 0.3, 0.35, 0.4, 0.22, 1),
                    ellipse_part("head", "head", 1, 0.38, 0.18, 0.16, 0.16, 2),
                    path_part(
                        "c1_tail",
                        "body",
                        1,
                        &[(0.6, 0.4), (0.7, 0.3), (0.66, 0.48)],
                        3,
                    ),
                ],
                ..Default::default()
            }),
            ..Default::default()
        };
        let r = review_illustration(&doc);
        assert!(r.issues.iter().any(|i| i.kind == "prop_missing"));
        assert!(r.issues.iter().any(|i| i.kind == "extra_characters"));
    }

    #[test]
    fn review_flags_jump_without_tuck() {
        let doc = IllustrationDoc {
            brief: IllustrationBrief {
                subject: "un chat qui saute".into(),
                ..Default::default()
            },
            last_sheet_png: Some("/downloads/illustration/sheet.png".into()),
            timeline: IllustrationTimeline {
                beats: vec![IllustrationTimelineBeat {
                    name: "sway".into(),
                    dur_s: 2.0,
                    pose: IllustrationPose {
                        tilt: 0.8,
                        ..Default::default()
                    },
                    camera: None,
                    mode: None,
                }],
            },
            spec: Some(IllustrationSpec {
                parts: vec![
                    ellipse_part("body", "body", 1, 0.3, 0.3, 0.3, 0.22, 1),
                    ellipse_part("head", "head", 1, 0.36, 0.16, 0.16, 0.16, 2),
                    path_part(
                        "paw_fl",
                        "leg",
                        1,
                        &[(0.3, 0.5), (0.34, 0.4), (0.4, 0.5)],
                        3,
                    ),
                ],
                ..Default::default()
            }),
            ..Default::default()
        };
        let r = review_illustration(&doc);
        assert!(r.issues.iter().any(|i| i.kind == "jump_missing"));
    }
}
