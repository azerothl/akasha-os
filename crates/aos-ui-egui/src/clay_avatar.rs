//! Procedural clay agent avatars — soft body, simple face, light idle motion.
//! Inspired by the Grok bot studio pattern (shape · face · colour · move),
//! drawn natively in egui (no external assets).

use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2,
};
use std::f32::consts::TAU;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClayShape {
    Circle,
    Pebble,
    Squircle,
    Capsule,
    Triangle,
    Hexagon,
    Cloud,
    Droplet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClayFace {
    Neutral,
    Attentive,
    Surprised,
    Excited,
    Happy,
    Laughing,
    Angry,
    Sad,
    Curious,
    Proud,
    Shy,
    Sleepy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClayAnim {
    Idle,
    Thinking,
    Wink,
    WideEyes,
    Alert,
    Sleep,
    Orbit,
    Burst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClaySpec {
    pub shape: ClayShape,
    pub face: ClayFace,
    pub anim: ClayAnim,
}

impl Default for ClaySpec {
    fn default() -> Self {
        Self {
            shape: ClayShape::Circle,
            face: ClayFace::Happy,
            anim: ClayAnim::Idle,
        }
    }
}

pub const CLAY_SHAPES: &[ClayShape] = &[
    ClayShape::Circle,
    ClayShape::Pebble,
    ClayShape::Squircle,
    ClayShape::Capsule,
    ClayShape::Triangle,
    ClayShape::Hexagon,
    ClayShape::Cloud,
    ClayShape::Droplet,
];

pub const CLAY_FACES: &[ClayFace] = &[
    ClayFace::Neutral,
    ClayFace::Attentive,
    ClayFace::Surprised,
    ClayFace::Excited,
    ClayFace::Happy,
    ClayFace::Laughing,
    ClayFace::Angry,
    ClayFace::Sad,
    ClayFace::Curious,
    ClayFace::Proud,
    ClayFace::Shy,
    ClayFace::Sleepy,
];

pub const CLAY_ANIMS: &[ClayAnim] = &[
    ClayAnim::Idle,
    ClayAnim::Thinking,
    ClayAnim::Wink,
    ClayAnim::WideEyes,
    ClayAnim::Alert,
    ClayAnim::Sleep,
    ClayAnim::Orbit,
    ClayAnim::Burst,
];

impl ClayShape {
    pub fn id(self) -> &'static str {
        match self {
            Self::Circle => "circle",
            Self::Pebble => "pebble",
            Self::Squircle => "squircle",
            Self::Capsule => "capsule",
            Self::Triangle => "triangle",
            Self::Hexagon => "hexagon",
            Self::Cloud => "cloud",
            Self::Droplet => "droplet",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "circle" => Self::Circle,
            "pebble" => Self::Pebble,
            "squircle" => Self::Squircle,
            "capsule" => Self::Capsule,
            "triangle" => Self::Triangle,
            "hexagon" => Self::Hexagon,
            "cloud" => Self::Cloud,
            "droplet" => Self::Droplet,
            _ => return None,
        })
    }
}

impl ClayFace {
    pub fn id(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Attentive => "attentive",
            Self::Surprised => "surprised",
            Self::Excited => "excited",
            Self::Happy => "happy",
            Self::Laughing => "laughing",
            Self::Angry => "angry",
            Self::Sad => "sad",
            Self::Curious => "curious",
            Self::Proud => "proud",
            Self::Shy => "shy",
            Self::Sleepy => "sleepy",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "neutral" => Self::Neutral,
            "attentive" => Self::Attentive,
            "surprised" => Self::Surprised,
            "excited" => Self::Excited,
            "happy" => Self::Happy,
            "laughing" => Self::Laughing,
            "angry" => Self::Angry,
            "sad" => Self::Sad,
            "curious" => Self::Curious,
            "proud" => Self::Proud,
            "shy" => Self::Shy,
            "sleepy" => Self::Sleepy,
            _ => return None,
        })
    }
}

impl ClayAnim {
    pub fn id(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Thinking => "thinking",
            Self::Wink => "wink",
            Self::WideEyes => "wide",
            Self::Alert => "alert",
            Self::Sleep => "sleep",
            Self::Orbit => "orbit",
            Self::Burst => "burst",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "idle" => Self::Idle,
            "thinking" => Self::Thinking,
            "wink" => Self::Wink,
            "wide" | "wide_eyes" => Self::WideEyes,
            "alert" => Self::Alert,
            "sleep" => Self::Sleep,
            "orbit" => Self::Orbit,
            "burst" => Self::Burst,
            _ => return None,
        })
    }
}

impl ClaySpec {
    pub fn encode(self) -> String {
        format!("clay:{}/{}/{}", self.shape.id(), self.face.id(), self.anim.id())
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let s = raw.trim();
        let body = s.strip_prefix("clay:")?;
        let mut parts = body.split('/');
        let shape = ClayShape::parse(parts.next()?.trim())?;
        let face = ClayFace::parse(parts.next().unwrap_or("happy").trim())
            .unwrap_or(ClayFace::Happy);
        let anim = ClayAnim::parse(parts.next().unwrap_or("idle").trim())
            .unwrap_or(ClayAnim::Idle);
        Some(Self { shape, face, anim })
    }

    /// Accept clay encodings and map legacy glyph ids to clay specs.
    pub fn resolve(raw: &str) -> Self {
        let s = raw.trim();
        if s.is_empty() {
            return Self::default();
        }
        if let Some(spec) = Self::parse(s) {
            return spec;
        }
        match s {
            "search" => Self {
                shape: ClayShape::Pebble,
                face: ClayFace::Curious,
                anim: ClayAnim::Thinking,
            },
            "code" => Self {
                shape: ClayShape::Squircle,
                face: ClayFace::Attentive,
                anim: ClayAnim::Idle,
            },
            "shield" => Self {
                shape: ClayShape::Hexagon,
                face: ClayFace::Angry,
                anim: ClayAnim::Alert,
            },
            "plan" => Self {
                shape: ClayShape::Capsule,
                face: ClayFace::Proud,
                anim: ClayAnim::Idle,
            },
            "chat" => Self {
                shape: ClayShape::Cloud,
                face: ClayFace::Happy,
                anim: ClayAnim::Idle,
            },
            "bolt" => Self {
                shape: ClayShape::Triangle,
                face: ClayFace::Excited,
                anim: ClayAnim::Burst,
            },
            "eye" => Self {
                shape: ClayShape::Circle,
                face: ClayFace::Surprised,
                anim: ClayAnim::WideEyes,
            },
            "book" => Self {
                shape: ClayShape::Squircle,
                face: ClayFace::Neutral,
                anim: ClayAnim::Idle,
            },
            "gear" => Self {
                shape: ClayShape::Hexagon,
                face: ClayFace::Attentive,
                anim: ClayAnim::Orbit,
            },
            "leaf" => Self {
                shape: ClayShape::Droplet,
                face: ClayFace::Shy,
                anim: ClayAnim::Idle,
            },
            "wave" => Self {
                shape: ClayShape::Pebble,
                face: ClayFace::Laughing,
                anim: ClayAnim::Idle,
            },
            "hash" => Self {
                shape: ClayShape::Squircle,
                face: ClayFace::Neutral,
                anim: ClayAnim::Idle,
            },
            "cube" => Self {
                shape: ClayShape::Squircle,
                face: ClayFace::Proud,
                anim: ClayAnim::Idle,
            },
            "mic" => Self {
                shape: ClayShape::Capsule,
                face: ClayFace::Excited,
                anim: ClayAnim::Idle,
            },
            "spark" | "initials" => Self::default(),
            _ => Self::default(),
        }
    }

    pub fn randomize(seed: u64) -> Self {
        let shape = CLAY_SHAPES[(seed as usize) % CLAY_SHAPES.len()];
        let face = CLAY_FACES[((seed >> 8) as usize) % CLAY_FACES.len()];
        let anim = CLAY_ANIMS[((seed >> 16) as usize) % CLAY_ANIMS.len()];
        Self { shape, face, anim }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum StudioTab {
    #[default]
    Look,
    Move,
}

/// Soft clay avatar. `animate` drives idle motion + continuous repaint.
pub fn show_clay_avatar(
    ui: &mut Ui,
    color: Color32,
    spec: ClaySpec,
    size: f32,
    selected: bool,
    animate: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let t = ui.input(|i| i.time) as f32;
    if animate {
        ui.ctx().request_repaint();
    }
    let pose = if animate {
        anim_pose(spec.anim, t)
    } else {
        AnimPose::default()
    };
    paint_clay(ui.painter(), rect, color, spec, pose, selected);
    response
}

struct AnimPose {
    offset: Vec2,
    scale: f32,
    rot: f32,
    wink_left: bool,
    wink_right: bool,
    eye_scale: f32,
    asleep: bool,
}

impl Default for AnimPose {
    fn default() -> Self {
        Self {
            offset: Vec2::ZERO,
            scale: 1.0,
            rot: 0.0,
            wink_left: false,
            wink_right: false,
            eye_scale: 1.0,
            asleep: false,
        }
    }
}

fn anim_pose(anim: ClayAnim, t: f32) -> AnimPose {
    let mut pose = AnimPose::default();
    match anim {
        ClayAnim::Idle => {
            pose.offset.y = (t * 2.2).sin() * 1.6;
            pose.scale = 1.0 + (t * 1.7).sin() * 0.025;
        }
        ClayAnim::Thinking => {
            pose.rot = (t * 1.8).sin() * 0.12;
            pose.offset.x = (t * 1.8).sin() * 2.0;
            pose.offset.y = (t * 2.4).cos() * 1.0;
        }
        ClayAnim::Wink => {
            pose.offset.y = (t * 2.0).sin() * 1.2;
            let phase = (t * 1.35).fract();
            pose.wink_right = (0.55..0.78).contains(&phase);
        }
        ClayAnim::WideEyes => {
            pose.eye_scale = 1.35 + (t * 3.0).sin().abs() * 0.08;
            pose.scale = 1.0 + (t * 2.5).sin() * 0.02;
        }
        ClayAnim::Alert => {
            let bounce = ((t * 6.0).sin().abs()) * 3.0;
            pose.offset.y = -bounce;
            pose.scale = 1.0 + (t * 6.0).sin().abs() * 0.04;
        }
        ClayAnim::Sleep => {
            pose.asleep = true;
            pose.offset.y = (t * 1.1).sin() * 1.0;
            pose.rot = (t * 0.7).sin() * 0.05;
        }
        ClayAnim::Orbit => {
            pose.rot = t * 1.4;
            pose.offset = Vec2::angled(t * 1.4) * 1.5;
        }
        ClayAnim::Burst => {
            let pulse = (t * 4.0).sin().abs();
            pose.scale = 1.0 + pulse * 0.08;
            pose.eye_scale = 1.0 + pulse * 0.15;
        }
    }
    pose
}

fn paint_clay(
    painter: &egui::Painter,
    rect: Rect,
    color: Color32,
    spec: ClaySpec,
    pose: AnimPose,
    selected: bool,
) {
    let c = rect.center() + pose.offset;
    let s = rect.width() * 0.42 * pose.scale;
    // Soft contact shadow
    painter.circle_filled(
        c + Vec2::new(0.0, s * 0.92),
        s * 0.55,
        Color32::from_black_alpha(38),
    );

    let body = clay_body_color(color);
    let shade = shade_color(body, 0.72);
    let hi = tint_color(body, 0.42);

    paint_shape(painter, c, s, spec.shape, body, shade, pose.rot);

    // Specular blob
    let hi_c = c + rotate(Vec2::new(-s * 0.28, -s * 0.32), pose.rot);
    painter.circle_filled(hi_c, s * 0.22, Color32::from_rgba_unmultiplied(hi.r(), hi.g(), hi.b(), 90));
    painter.circle_filled(
        hi_c + Vec2::new(-s * 0.04, -s * 0.04),
        s * 0.10,
        Color32::from_white_alpha(70),
    );

    paint_face(painter, c, s, spec.face, pose);

    if selected {
        painter.circle_stroke(c, s * 1.18, Stroke::new(2.0, color));
    }
}

fn clay_body_color(c: Color32) -> Color32 {
    // Slightly desaturate toward a clay matte.
    Color32::from_rgb(
        ((u16::from(c.r()) * 5 + 40) / 6) as u8,
        ((u16::from(c.g()) * 5 + 36) / 6) as u8,
        ((u16::from(c.b()) * 5 + 48) / 6) as u8,
    )
}

fn tint_color(c: Color32, toward_white: f32) -> Color32 {
    let t = toward_white.clamp(0.0, 1.0);
    Color32::from_rgb(
        (f32::from(c.r()) * (1.0 - t) + 255.0 * t) as u8,
        (f32::from(c.g()) * (1.0 - t) + 255.0 * t) as u8,
        (f32::from(c.b()) * (1.0 - t) + 255.0 * t) as u8,
    )
}

fn shade_color(c: Color32, factor: f32) -> Color32 {
    let f = factor.clamp(0.0, 1.0);
    Color32::from_rgb(
        (f32::from(c.r()) * f) as u8,
        (f32::from(c.g()) * f) as u8,
        (f32::from(c.b()) * f) as u8,
    )
}

fn rotate(v: Vec2, a: f32) -> Vec2 {
    let (sin, cos) = a.sin_cos();
    Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}

fn paint_shape(
    painter: &egui::Painter,
    c: Pos2,
    s: f32,
    shape: ClayShape,
    body: Color32,
    shade: Color32,
    rot: f32,
) {
    match shape {
        ClayShape::Circle => {
            painter.circle_filled(c + Vec2::new(0.0, s * 0.06), s * 1.02, shade);
            painter.circle_filled(c, s, body);
        }
        ClayShape::Pebble => {
            let r = Rect::from_center_size(c, Vec2::new(s * 2.05, s * 1.78));
            painter.rect_filled(
                r.translate(Vec2::new(0.0, s * 0.06)),
                CornerRadius::same((s * 0.95) as u8),
                shade,
            );
            painter.rect_filled(r, CornerRadius::same((s * 0.95) as u8), body);
        }
        ClayShape::Squircle => {
            let r = Rect::from_center_size(c, Vec2::splat(s * 1.9));
            painter.rect_filled(
                r.translate(Vec2::new(0.0, s * 0.06)),
                CornerRadius::same((s * 0.55) as u8),
                shade,
            );
            painter.rect_filled(r, CornerRadius::same((s * 0.55) as u8), body);
        }
        ClayShape::Capsule => {
            let r = Rect::from_center_size(c, Vec2::new(s * 1.55, s * 2.1));
            painter.rect_filled(
                r.translate(Vec2::new(0.0, s * 0.06)),
                CornerRadius::same((s * 0.78) as u8),
                shade,
            );
            painter.rect_filled(r, CornerRadius::same((s * 0.78) as u8), body);
        }
        ClayShape::Triangle => {
            let pts = rounded_poly(
                c,
                &[
                    Vec2::new(0.0, -s * 1.15),
                    Vec2::new(s * 1.12, s * 0.95),
                    Vec2::new(-s * 1.12, s * 0.95),
                ],
                rot,
                10,
            );
            painter.add(Shape::convex_polygon(pts.clone(), shade, Stroke::NONE));
            let pts2: Vec<_> = pts.iter().map(|p| *p - Vec2::new(0.0, s * 0.05)).collect();
            painter.add(Shape::convex_polygon(pts2, body, Stroke::NONE));
        }
        ClayShape::Hexagon => {
            let mut verts = Vec::with_capacity(6);
            for i in 0..6 {
                let a = rot + i as f32 * TAU / 6.0 - TAU / 12.0;
                verts.push(rotate(Vec2::angled(a) * s * 1.12, 0.0));
            }
            let pts = rounded_poly(c, &verts, 0.0, 8);
            painter.add(Shape::convex_polygon(
                pts.iter()
                    .map(|p| *p + Vec2::new(0.0, s * 0.05))
                    .collect::<Vec<_>>(),
                shade,
                Stroke::NONE,
            ));
            painter.add(Shape::convex_polygon(pts, body, Stroke::NONE));
        }
        ClayShape::Cloud => {
            let blobs = [
                (Vec2::new(-s * 0.45, s * 0.15), s * 0.62),
                (Vec2::new(s * 0.40, s * 0.18), s * 0.58),
                (Vec2::new(0.0, -s * 0.25), s * 0.72),
                (Vec2::new(0.0, s * 0.28), s * 0.70),
            ];
            for (off, r) in blobs {
                painter.circle_filled(c + rotate(off, rot) + Vec2::new(0.0, s * 0.05), r, shade);
            }
            for (off, r) in blobs {
                painter.circle_filled(c + rotate(off, rot), r * 0.98, body);
            }
        }
        ClayShape::Droplet => {
            let pts = rounded_poly(
                c,
                &[
                    Vec2::new(0.0, -s * 1.2),
                    Vec2::new(s * 0.95, s * 0.35),
                    Vec2::new(0.0, s * 1.05),
                    Vec2::new(-s * 0.95, s * 0.35),
                ],
                rot,
                12,
            );
            painter.add(Shape::convex_polygon(
                pts.iter()
                    .map(|p| *p + Vec2::new(0.0, s * 0.05))
                    .collect::<Vec<_>>(),
                shade,
                Stroke::NONE,
            ));
            painter.add(Shape::convex_polygon(pts, body, Stroke::NONE));
        }
    }
}

fn rounded_poly(center: Pos2, verts: &[Vec2], rot: f32, steps: usize) -> Vec<Pos2> {
    let n = verts.len();
    if n == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(n * steps);
    for i in 0..n {
        let a = rotate(verts[i], rot);
        let b = rotate(verts[(i + 1) % n], rot);
        for k in 0..steps {
            let t = k as f32 / steps as f32;
            // Ease toward mid for softer corners.
            let ease = t * t * (3.0 - 2.0 * t);
            let p = a * (1.0 - ease) + b * ease;
            out.push(center + p);
        }
    }
    out
}

fn paint_face(painter: &egui::Painter, c: Pos2, s: f32, face: ClayFace, pose: AnimPose) {
    let ink = Color32::from_rgb(18, 20, 28);
    let eye_y = match face {
        ClayFace::Sad | ClayFace::Shy => -s * 0.02,
        ClayFace::Proud => -s * 0.12,
        _ => -s * 0.08,
    };
    let eye_x = s * 0.28;
    let base_r = s * 0.11 * pose.eye_scale;
    let (left_closed, right_closed) = match face {
        ClayFace::Sleepy => (true, true),
        _ if pose.asleep => (true, true),
        _ => (pose.wink_left, pose.wink_right),
    };
    let eye_h = match face {
        ClayFace::Happy | ClayFace::Laughing => base_r * 0.55,
        ClayFace::Surprised | ClayFace::Excited => base_r * 1.25,
        ClayFace::Angry => base_r * 0.85,
        _ => base_r,
    };
    let eye_w = match face {
        ClayFace::Attentive => base_r * 0.75,
        ClayFace::Curious => base_r * 1.15,
        ClayFace::Surprised => base_r * 1.2,
        _ => base_r,
    };

    let left = c + Vec2::new(-eye_x, eye_y);
    let right = c + Vec2::new(eye_x, eye_y)
        + match face {
            ClayFace::Curious => Vec2::new(s * 0.04, -s * 0.03),
            ClayFace::Shy => Vec2::new(-s * 0.02, s * 0.02),
            _ => Vec2::ZERO,
        };

    draw_eye(painter, left, eye_w, eye_h, ink, left_closed, face);
    draw_eye(painter, right, eye_w, eye_h, ink, right_closed, face);

    // Brows
    match face {
        ClayFace::Angry => {
            painter.line_segment(
                [left + Vec2::new(-eye_w, -eye_h * 1.6), left + Vec2::new(eye_w, -eye_h * 0.9)],
                Stroke::new(s * 0.06, ink),
            );
            painter.line_segment(
                [
                    right + Vec2::new(-eye_w, -eye_h * 0.9),
                    right + Vec2::new(eye_w, -eye_h * 1.6),
                ],
                Stroke::new(s * 0.06, ink),
            );
        }
        ClayFace::Sad => {
            painter.line_segment(
                [left + Vec2::new(-eye_w, -eye_h * 0.9), left + Vec2::new(eye_w, -eye_h * 1.5)],
                Stroke::new(s * 0.05, ink),
            );
            painter.line_segment(
                [
                    right + Vec2::new(-eye_w, -eye_h * 1.5),
                    right + Vec2::new(eye_w, -eye_h * 0.9),
                ],
                Stroke::new(s * 0.05, ink),
            );
        }
        ClayFace::Proud => {
            painter.line_segment(
                [left + Vec2::new(-eye_w, -eye_h * 1.5), left + Vec2::new(eye_w, -eye_h * 1.5)],
                Stroke::new(s * 0.05, ink),
            );
            painter.line_segment(
                [
                    right + Vec2::new(-eye_w, -eye_h * 1.5),
                    right + Vec2::new(eye_w, -eye_h * 1.5),
                ],
                Stroke::new(s * 0.05, ink),
            );
        }
        _ => {}
    }

    // Mouth
    let mouth_c = c + Vec2::new(
        match face {
            ClayFace::Shy => s * 0.06,
            _ => 0.0,
        },
        s * 0.28,
    );
    match face {
        ClayFace::Neutral | ClayFace::Attentive | ClayFace::Curious => {
            painter.line_segment(
                [mouth_c + Vec2::new(-s * 0.12, 0.0), mouth_c + Vec2::new(s * 0.12, 0.0)],
                Stroke::new(s * 0.055, ink),
            );
        }
        ClayFace::Happy | ClayFace::Proud => {
            paint_arc(painter, mouth_c, s * 0.18, 0.15, TAU * 0.35, Stroke::new(s * 0.06, ink));
        }
        ClayFace::Laughing | ClayFace::Excited => {
            painter.circle_filled(mouth_c + Vec2::new(0.0, s * 0.02), s * 0.12, ink);
            painter.circle_filled(
                mouth_c + Vec2::new(0.0, -s * 0.02),
                s * 0.08,
                Color32::from_rgb(40, 30, 35),
            );
        }
        ClayFace::Surprised => {
            painter.circle_filled(mouth_c, s * 0.10, ink);
        }
        ClayFace::Sad | ClayFace::Shy => {
            paint_arc(
                painter,
                mouth_c + Vec2::new(0.0, s * 0.08),
                s * 0.16,
                TAU * 0.55,
                TAU * 0.35,
                Stroke::new(s * 0.055, ink),
            );
        }
        ClayFace::Angry => {
            painter.line_segment(
                [mouth_c + Vec2::new(-s * 0.14, s * 0.04), mouth_c + Vec2::new(s * 0.14, 0.0)],
                Stroke::new(s * 0.06, ink),
            );
        }
        ClayFace::Sleepy => {
            paint_arc(painter, mouth_c, s * 0.12, 0.2, TAU * 0.25, Stroke::new(s * 0.05, ink));
        }
    }

    if matches!(face, ClayFace::Shy) {
        let blush = Color32::from_rgba_unmultiplied(255, 120, 140, 70);
        painter.circle_filled(c + Vec2::new(-s * 0.42, s * 0.12), s * 0.12, blush);
        painter.circle_filled(c + Vec2::new(s * 0.42, s * 0.12), s * 0.12, blush);
    }
}

fn draw_eye(
    painter: &egui::Painter,
    center: Pos2,
    w: f32,
    h: f32,
    ink: Color32,
    closed: bool,
    face: ClayFace,
) {
    if closed {
        painter.line_segment(
            [center + Vec2::new(-w, 0.0), center + Vec2::new(w, 0.0)],
            Stroke::new(h.max(1.2), ink),
        );
        return;
    }
    if matches!(face, ClayFace::Happy | ClayFace::Laughing) {
        paint_arc(
            painter,
            center + Vec2::new(0.0, h * 0.2),
            w * 1.1,
            0.15,
            TAU * 0.35,
            Stroke::new(h.max(1.2), ink),
        );
        return;
    }
    let r = Rect::from_center_size(center, Vec2::new(w * 2.0, h * 2.0));
    painter.rect_filled(r, CornerRadius::same((h.max(w)) as u8), ink);
    painter.circle_filled(
        center + Vec2::new(-w * 0.25, -h * 0.25),
        (w * 0.28).max(0.8),
        Color32::from_white_alpha(160),
    );
}

fn paint_arc(painter: &egui::Painter, c: Pos2, r: f32, start: f32, sweep: f32, stroke: Stroke) {
    let steps = 10;
    let mut prev = c + Vec2::angled(start) * r;
    for i in 1..=steps {
        let a = start + sweep * (i as f32 / steps as f32);
        let p = c + Vec2::angled(a) * r;
        painter.line_segment([prev, p], stroke);
        prev = p;
    }
}

/// Studio editor: Look (shape/face/colour) · Move (animation) · Surprise.
pub fn clay_avatar_studio(
    ui: &mut Ui,
    avatar: &mut String,
    color_hex: &mut String,
    name: &str,
    fallback_color: Color32,
    labels: &ClayStudioLabels<'_>,
) {
    let mut spec = ClaySpec::resolve(avatar);
    // Normalize storage to clay encoding.
    if !avatar.starts_with("clay:") {
        *avatar = spec.encode();
    }

    let color = crate::chat_room::parse_agent_color_hex(color_hex)
        .map(|(r, g, b)| Color32::from_rgb(r, g, b))
        .unwrap_or(fallback_color);

    let tab_id = ui.id().with("clay_avatar_studio_tab");
    let mut tab = ui.data_mut(|d| *d.get_temp_mut_or_default::<StudioTab>(tab_id));

    ui.horizontal(|ui| {
        let preview = ui.allocate_ui(Vec2::splat(108.0), |ui| {
            show_clay_avatar(ui, color, spec, 96.0, true, true)
        });
        ui.add_space(10.0);
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(if name.trim().is_empty() {
                    "Agent"
                } else {
                    name
                })
                .strong()
                .size(15.0),
            );
            ui.weak(
                egui::RichText::new(format!(
                    "{} · {} · {}",
                    labels.shape_name(spec.shape),
                    labels.face_name(spec.face),
                    labels.anim_name(spec.anim)
                ))
                .small(),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(tab == StudioTab::Look, labels.look)
                    .clicked()
                {
                    tab = StudioTab::Look;
                }
                if ui
                    .selectable_label(tab == StudioTab::Move, labels.move_tab)
                    .clicked()
                {
                    tab = StudioTab::Move;
                }
                if ui.button(labels.surprise).clicked() {
                    let seed = (ui.input(|i| i.time) * 1000.0) as u64
                        ^ (name.len() as u64).wrapping_mul(2654435761);
                    spec = ClaySpec::randomize(seed);
                    *avatar = spec.encode();
                }
            });
            let _ = preview;
        });
    });
    ui.data_mut(|d| d.insert_temp(tab_id, tab));

    ui.add_space(8.0);
    match tab {
        StudioTab::Look => {
            ui.label(egui::RichText::new(labels.shape).small().strong());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::splat(6.0);
                for &shape in CLAY_SHAPES {
                    let on = spec.shape == shape;
                    let mut trial = spec;
                    trial.shape = shape;
                    let resp = show_clay_avatar(ui, color, trial, 34.0, on, false);
                    if resp.clicked() {
                        spec.shape = shape;
                        *avatar = spec.encode();
                    }
                    resp.on_hover_text(labels.shape_name(shape));
                }
            });
            ui.add_space(6.0);
            ui.label(egui::RichText::new(labels.face).small().strong());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::splat(6.0);
                for &face in CLAY_FACES {
                    let on = spec.face == face;
                    let mut trial = spec;
                    trial.face = face;
                    let resp = show_clay_avatar(ui, color, trial, 34.0, on, false);
                    if resp.clicked() {
                        spec.face = face;
                        *avatar = spec.encode();
                    }
                    resp.on_hover_text(labels.face_name(face));
                }
            });
            ui.add_space(6.0);
            ui.label(egui::RichText::new(labels.colour).small().strong());
            paint_color_row(ui, color_hex, fallback_color, labels.colour_auto);
        }
        StudioTab::Move => {
            ui.label(egui::RichText::new(labels.animations).small().strong());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::splat(6.0);
                for &anim in CLAY_ANIMS {
                    let on = spec.anim == anim;
                    let mut trial = spec;
                    trial.anim = anim;
                    let resp = show_clay_avatar(ui, color, trial, 40.0, on, on);
                    if resp.clicked() {
                        spec.anim = anim;
                        *avatar = spec.encode();
                    }
                    resp.on_hover_text(labels.anim_name(anim));
                }
            });
        }
    }
}

fn paint_color_row(ui: &mut Ui, color_hex: &mut String, fallback: Color32, auto_label: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(6.0);
        let auto_on = color_hex.trim().is_empty();
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
        if ui.is_rect_visible(rect) {
            ui.painter().circle_filled(
                rect.center(),
                9.0,
                mix(ui.visuals().panel_fill, fallback, 0.45),
            );
            ui.painter().circle_stroke(
                rect.center(),
                9.0,
                Stroke::new(if auto_on { 2.0 } else { 1.0 }, fallback),
            );
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "A",
                egui::FontId::proportional(9.0),
                fallback,
            );
        }
        if response.clicked() {
            color_hex.clear();
        }
        response.on_hover_text(auto_label);
        for &(r, g, b) in crate::chat_room::AGENT_COLOR_PRESETS {
            let swatch = Color32::from_rgb(r, g, b);
            let hex = crate::chat_room::format_agent_color_hex(r, g, b);
            let on = color_hex.eq_ignore_ascii_case(&hex);
            let (rect, response) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
            if ui.is_rect_visible(rect) {
                ui.painter().circle_filled(rect.center(), 9.0, swatch);
                if on {
                    ui.painter().circle_stroke(
                        rect.center(),
                        9.0,
                        Stroke::new(2.0, ui.visuals().strong_text_color()),
                    );
                }
            }
            if response.clicked() {
                *color_hex = hex;
            }
        }
    });
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    Color32::from_rgb(
        (f32::from(a.r()) * inv + f32::from(b.r()) * t) as u8,
        (f32::from(a.g()) * inv + f32::from(b.g()) * t) as u8,
        (f32::from(a.b()) * inv + f32::from(b.b()) * t) as u8,
    )
}

pub struct ClayStudioLabels<'a> {
    pub look: &'a str,
    pub move_tab: &'a str,
    pub surprise: &'a str,
    pub shape: &'a str,
    pub face: &'a str,
    pub colour: &'a str,
    pub colour_auto: &'a str,
    pub animations: &'a str,
    pub shapes: [&'a str; 8],
    pub faces: [&'a str; 12],
    pub anims: [&'a str; 8],
}

impl ClayStudioLabels<'_> {
    pub fn shape_name(&self, s: ClayShape) -> &str {
        self.shapes[s as usize]
    }
    pub fn face_name(&self, f: ClayFace) -> &str {
        self.faces[f as usize]
    }
    pub fn anim_name(&self, a: ClayAnim) -> &str {
        self.anims[a as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encode_parse() {
        let spec = ClaySpec {
            shape: ClayShape::Cloud,
            face: ClayFace::Curious,
            anim: ClayAnim::Thinking,
        };
        let enc = spec.encode();
        assert_eq!(enc, "clay:cloud/curious/thinking");
        assert_eq!(ClaySpec::parse(&enc), Some(spec));
    }

    #[test]
    fn legacy_glyph_maps() {
        let code = ClaySpec::resolve("code");
        assert_eq!(code.shape, ClayShape::Squircle);
        assert_eq!(ClaySpec::resolve("clay:droplet/shy/sleep").anim, ClayAnim::Sleep);
    }

    #[test]
    fn randomize_stays_in_catalog() {
        let s = ClaySpec::randomize(42);
        assert!(CLAY_SHAPES.contains(&s.shape));
        assert!(CLAY_FACES.contains(&s.face));
        assert!(CLAY_ANIMS.contains(&s.anim));
    }
}
