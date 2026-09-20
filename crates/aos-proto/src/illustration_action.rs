//! Verb → timeline. Pose numbers keep the skill meaning: walk, twitch, wing, flap, tuck, tilt.

use crate::illustration::{
    IllustrationBrief, IllustrationEngine, IllustrationLook, IllustrationPose,
    IllustrationTimelineBeat,
};

pub fn ease_io(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn lerp_pose(a: &IllustrationPose, b: &IllustrationPose, t: f32) -> IllustrationPose {
    let l = |x: f32, y: f32| x + (y - x) * t;
    IllustrationPose {
        walk: l(a.walk, b.walk),
        twitch: l(a.twitch, b.twitch),
        wing: l(a.wing, b.wing),
        flap: l(a.flap, b.flap),
        tuck: l(a.tuck, b.tuck),
        tilt: l(a.tilt, b.tilt),
    }
}

fn action_text(brief: &IllustrationBrief) -> String {
    format!("{} {}", brief.subject, brief.beats.join(" ")).to_ascii_lowercase()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Jump,
    Read,
    Ride,
    Walk,
    Stretch,
    Gallop,
    Hop,
    Idle,
}

fn detect(text: &str, engine: IllustrationEngine) -> Action {
    if text.contains("galop") || text.contains("gallop") {
        return Action::Gallop;
    }
    if text.contains("saut")
        || text.contains("jump")
        || text.contains("bond")
        || text.contains("leap")
    {
        return Action::Jump;
    }
    if text.contains(" hop") || text.contains("hoppe") {
        return Action::Hop;
    }
    if text.contains("lit ")
        || text.contains(" lit")
        || text.contains("read")
        || text.contains("journal")
        || text.contains("magazine")
        || text.contains("magasine")
    {
        return Action::Read;
    }
    if text.contains("velo")
        || text.contains("vélo")
        || text.contains("bike")
        || text.contains("roule")
        || text.contains("ride")
    {
        return Action::Ride;
    }
    if text.contains("marche") || text.contains("walk") || text.contains("pas ") {
        return Action::Walk;
    }
    if text.contains("étire") || text.contains("etire") || text.contains("stretch") {
        return Action::Stretch;
    }
    if engine == IllustrationEngine::Found {
        Action::Gallop
    } else {
        Action::Idle
    }
}

fn beat(name: &str, weight: f32, pose: IllustrationPose) -> IllustrationTimelineBeat {
    IllustrationTimelineBeat {
        name: name.into(),
        dur_s: weight,
        pose,
        camera: None,
        mode: None,
    }
}

fn pose(walk: f32, twitch: f32, wing: f32, flap: f32, tuck: f32, tilt: f32) -> IllustrationPose {
    IllustrationPose {
        walk,
        twitch,
        wing,
        flap,
        tuck,
        tilt,
    }
}

/// Beats for an empty timeline. Weights are scaled to `secs`, with a sign-off when long enough.
pub fn action_timeline(brief: &IllustrationBrief, secs: f32) -> Vec<IllustrationTimelineBeat> {
    let secs = secs.clamp(0.5, 12.0);
    let action = detect(&action_text(brief), brief.engine);
    let mut raw = match action {
        Action::Jump => vec![
            beat("anticipate", 0.22, pose(0.0, 0.15, 0.0, 0.2, 0.28, -0.35)),
            beat("airborne", 0.48, pose(0.05, 0.2, 0.3, 0.55, 1.0, 0.12)),
            beat("land", 0.30, pose(0.0, 0.85, 0.0, 0.15, 0.18, 0.05)),
        ],
        Action::Read => vec![
            beat("open", 0.4, pose(0.0, 0.1, 0.4, 0.15, 0.05, -0.08)),
            beat("read", 0.6, pose(0.0, 0.2, 0.15, 0.35, 0.0, 0.04)),
        ],
        Action::Ride => vec![
            beat("mount", 0.3, pose(0.4, 0.1, 0.2, 0.1, 0.12, 0.05)),
            beat("roll", 0.7, pose(1.0, 0.15, 0.1, 0.2, 0.05, 0.0)),
        ],
        Action::Walk => vec![
            beat("step_l", 0.5, pose(1.0, 0.2, 0.3, 0.15, 0.08, 0.06)),
            beat("step_r", 0.5, pose(-1.0, 0.2, -0.3, 0.15, 0.08, -0.06)),
        ],
        Action::Stretch => vec![
            beat("reach", 0.55, pose(0.1, 0.1, 0.8, 0.1, 0.05, 0.45)),
            beat("hold", 0.45, pose(0.0, 0.25, 0.2, 0.2, 0.0, 0.2)),
        ],
        Action::Gallop => vec![
            beat("contact", 0.3, pose(1.1, 0.2, 0.4, 0.2, 0.12, 0.08)),
            beat("air", 0.4, pose(0.2, 0.15, 0.2, 0.3, 0.9, 0.05)),
            beat("reach", 0.3, pose(-1.1, 0.25, -0.4, 0.2, 0.15, -0.06)),
        ],
        Action::Hop => vec![
            beat("load", 0.3, pose(0.0, 0.2, 0.0, 0.1, 0.35, -0.2)),
            beat("hop", 0.4, pose(0.1, 0.2, 0.5, 0.4, 1.0, 0.1)),
            beat("down", 0.3, pose(0.0, 0.7, 0.0, 0.1, 0.1, 0.0)),
        ],
        Action::Idle => vec![
            beat("settle", 0.45, pose(0.0, 0.2, 0.0, 0.12, 0.0, 0.0)),
            beat("breathe", 0.55, pose(0.0, 0.55, 0.1, 0.28, 0.0, 0.04)),
        ],
    };
    let sign = if secs > 3.0 {
        1.5_f32.min(secs * 0.35)
    } else {
        0.0
    };
    let body = (secs - sign).max(0.4);
    let sum = raw.iter().map(|b| b.dur_s).sum::<f32>().max(0.05);
    for b in &mut raw {
        b.dur_s = (b.dur_s / sum) * body;
    }
    if sign > 0.05 {
        raw.push(beat("signoff", sign, IllustrationPose::default()));
    }
    raw
}

/// Scale an existing sheet, or build one from the verb when empty.
pub fn resolve_timeline(
    beats: &[IllustrationTimelineBeat],
    secs: f32,
    brief: &IllustrationBrief,
) -> Vec<IllustrationTimelineBeat> {
    let secs = secs.clamp(0.5, 12.0);
    if beats.is_empty() {
        return action_timeline(brief, secs);
    }
    let sum = beats
        .iter()
        .map(|b| b.dur_s.max(0.05))
        .sum::<f32>()
        .max(0.05);
    let scale = secs / sum;
    beats
        .iter()
        .map(|b| {
            let mut beat = b.clone();
            beat.dur_s = (b.dur_s.max(0.05) * scale).max(0.05);
            beat
        })
        .collect()
}

pub fn sign_off_word(brief: &IllustrationBrief) -> String {
    let raw = brief
        .anchor
        .split_whitespace()
        .next()
        .filter(|w| !w.is_empty())
        .unwrap_or("fin");
    let ascii: String = raw
        .chars()
        .flat_map(|c| c.to_lowercase())
        .filter(|c| c.is_ascii_alphanumeric())
        .take(6)
        .collect();
    if ascii.is_empty() || matches!(ascii.as_str(), "center" | "centre" | "anchor" | "milieu") {
        "fin".into()
    } else {
        ascii
    }
}

/// The engine follows the user's words. An agent that sets `paper` on a cat
/// drawing is ignored: that renderer throws the composed puppet away.
pub fn apply_prompt_defaults(brief: &mut IllustrationBrief) {
    let text = action_text(brief);
    let sand = text.contains("sable")
        || text.contains("sand")
        || text.contains("péso")
        || text.contains("peso");
    let paper = text.contains("pop-up")
        || text.contains("popup")
        || text.contains("pop up")
        || text.contains("theatre de papier")
        || text.contains("théâtre de papier")
        || text.contains("papier dans");
    let found = text.contains("muybridge")
        || text.contains("rotoscope")
        || text.contains("vidéo")
        || text.contains("video");
    brief.engine = if sand {
        IllustrationEngine::Sand
    } else if paper {
        IllustrationEngine::Paper
    } else if found {
        IllustrationEngine::Found
    } else {
        IllustrationEngine::Flat
    };
    let photo = text.contains("doodle")
        || text.contains("sur cette photo")
        || text.contains("sur la photo");
    if photo || !brief.photo.trim().is_empty() {
        if brief.look == IllustrationLook::Ink || photo {
            brief.look = IllustrationLook::Doodle;
            if brief.palette == IllustrationLook::Ink.default_palette() || photo {
                brief.palette = IllustrationLook::Doodle.default_palette();
            }
        }
    }
}

pub fn video_trace_error(brief: &IllustrationBrief) -> Option<&'static str> {
    if brief.video.trim().is_empty() {
        None
    } else {
        Some(
            "mouvement trouvé : pas de traceur vidéo. Décris le cycle (marche, galop, saut, hop) sans fichier.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jump_timeline_tucks_and_fits() {
        let brief = IllustrationBrief {
            subject: "un chat qui saute sur un canapé".into(),
            ..Default::default()
        };
        let beats = action_timeline(&brief, 4.0);
        assert!(beats.iter().any(|b| b.pose.tuck >= 0.8));
        assert!(beats.iter().any(|b| b.name == "signoff"));
        let sum: f32 = beats.iter().map(|b| b.dur_s).sum();
        assert!((sum - 4.0).abs() < 0.08, "{sum}");
    }

    #[test]
    fn found_gallop_alternates() {
        let brief = IllustrationBrief {
            subject: "un cheval".into(),
            engine: IllustrationEngine::Found,
            ..Default::default()
        };
        let beats = action_timeline(&brief, 4.0);
        let walks: Vec<f32> = beats
            .iter()
            .filter(|b| b.name != "signoff")
            .map(|b| b.pose.walk)
            .collect();
        assert!(walks.iter().any(|w| *w > 0.5));
        assert!(walks.iter().any(|w| *w < -0.5));
    }

    #[test]
    fn video_path_is_refused() {
        let brief = IllustrationBrief {
            video: "/tmp/clip.mp4".into(),
            ..Default::default()
        };
        assert!(video_trace_error(&brief).is_some());
        assert!(video_trace_error(&IllustrationBrief::default()).is_none());
    }

    #[test]
    fn sand_word_selects_engine() {
        let mut brief = IllustrationBrief {
            subject: "une animation de sable".into(),
            ..Default::default()
        };
        apply_prompt_defaults(&mut brief);
        assert_eq!(brief.engine, IllustrationEngine::Sand);
    }

    #[test]
    fn cat_drawing_cannot_select_the_paper_engine() {
        let mut brief = IllustrationBrief {
            subject: "un chat en plein saut sur un canapé".into(),
            engine: IllustrationEngine::Paper,
            ..Default::default()
        };
        apply_prompt_defaults(&mut brief);
        assert_eq!(brief.engine, IllustrationEngine::Flat);
    }
}
