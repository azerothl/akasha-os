//! Rich declarative UI runtime (issue #150 lot 1): state, subscriptions, demo jobs.
//!
//! Surface lock: job handles carry technical ids internally; the host paints human
//! progress copy only (see `i18n::job_state_human_label`). Never embed Create-specific
//! widgets or chrome here.

use aos_proto::rich_decl_ui::{RichInteractionEvent, RichJobHandle, RichJobProgress};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

static DEMO_JOBS: LazyLock<Mutex<DemoJobRegistry>> =
    LazyLock::new(|| Mutex::new(DemoJobRegistry::default()));

pub fn demo_jobs() -> &'static Mutex<DemoJobRegistry> {
    &DEMO_JOBS
}

/// Active job/event subscriptions for one module panel instance.
#[derive(Debug, Default)]
pub struct RichDeclSubscriptions {
    pub ids: HashSet<String>,
    pub jobs: HashMap<String, RichJobHandle>,
}

impl RichDeclSubscriptions {
    pub fn register(&mut self, id: impl Into<String>) {
        self.ids.insert(id.into());
    }

    pub fn unregister(&mut self, id: &str) {
        self.ids.remove(id);
        self.jobs.remove(id);
    }

    pub fn clear(&mut self) {
        self.ids.clear();
        self.jobs.clear();
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn set_job(&mut self, subscription_id: &str, job: RichJobHandle) {
        if self.ids.contains(subscription_id) {
            self.jobs.insert(subscription_id.to_string(), job);
        }
    }

    pub fn job(&self, subscription_id: &str) -> Option<&RichJobHandle> {
        self.jobs.get(subscription_id)
    }
}

/// Local pan/zoom state for an `image_view` widget (host-owned, no WASM round trips).
#[derive(Debug, Clone, Default)]
pub struct ImageViewInteractionState {
    pub zoom: f32,
    pub pan: [f32; 2],
    pub interaction_id: String,
    pub active: bool,
}

impl ImageViewInteractionState {
    pub fn apply_semantic(&mut self, event: &RichInteractionEvent) {
        match event.phase.as_str() {
            "start" => {
                self.interaction_id = event.interaction_id.clone();
                self.active = true;
            }
            "update" => {
                if event.interaction_id == self.interaction_id {
                    if let Some(z) = event.value.get("zoom").and_then(|v| v.as_f64()) {
                        self.zoom = z as f32;
                    }
                    if let (Some(x), Some(y)) = (
                        event.value.get("pan_x").and_then(|v| v.as_f64()),
                        event.value.get("pan_y").and_then(|v| v.as_f64()),
                    ) {
                        self.pan = [x as f32, y as f32];
                    }
                }
            }
            "commit" | "cancel" => {
                if event.interaction_id == self.interaction_id {
                    self.active = false;
                }
            }
            _ => {}
        }
    }
}

/// Controllable demo job for lot-1 harness (`jobs.demo.start` / `jobs.demo.cancel`).
#[derive(Debug)]
struct DemoJob {
    job_id: String,
    cancel: Arc<Mutex<bool>>,
}

#[derive(Default)]
pub struct DemoJobRegistry {
    jobs: HashMap<String, DemoJob>,
}

impl DemoJobRegistry {
    pub fn start(&mut self, steps: u32) -> RichJobHandle {
        let job_id = format!("demo-{}", steps);
        let cancel = Arc::new(Mutex::new(false));
        self.jobs.insert(
            job_id.clone(),
            DemoJob {
                job_id: job_id.clone(),
                cancel: cancel.clone(),
            },
        );
        RichJobHandle {
            job_id: Some(job_id),
            kind: Some("jobs.demo".into()),
            state: Some("queued".into()),
            progress: Some(RichJobProgress {
                completed: 0,
                total: steps,
                unit: Some("step".into()),
            }),
            result: None,
            error: None,
        }
    }

    pub fn cancel(&mut self, job_id: &str) -> bool {
        if let Some(job) = self.jobs.get(job_id) {
            *job.cancel.lock().unwrap() = true;
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, job_id: &str) {
        self.jobs.remove(job_id);
    }

    pub fn cancel_flag(&self, job_id: &str) -> Option<Arc<Mutex<bool>>> {
        self.jobs.get(job_id).map(|j| j.cancel.clone())
    }
}

pub fn demo_job_tick(job_id: &str, completed: u32, total: u32, cancelled: bool) -> RichJobHandle {
    if cancelled {
        return RichJobHandle {
            job_id: Some(job_id.into()),
            kind: Some("jobs.demo".into()),
            state: Some("cancelled".into()),
            progress: Some(RichJobProgress {
                completed,
                total,
                unit: Some("step".into()),
            }),
            result: None,
            error: None,
        };
    }
    if completed >= total {
        return RichJobHandle {
            job_id: Some(job_id.into()),
            kind: Some("jobs.demo".into()),
            state: Some("succeeded".into()),
            progress: Some(RichJobProgress {
                completed: total,
                total,
                unit: Some("step".into()),
            }),
            result: Some(json!({"ok": true})),
            error: None,
        };
    }
    RichJobHandle {
        job_id: Some(job_id.into()),
        kind: Some("jobs.demo".into()),
        state: Some("running".into()),
        progress: Some(RichJobProgress {
            completed,
            total,
            unit: Some("step".into()),
        }),
        result: None,
        error: None,
    }
}

/// Rate-limit job progress UI updates (≤10 Hz per job).
pub struct JobProgressThrottle {
    last: HashMap<String, Instant>,
}

impl std::fmt::Debug for JobProgressThrottle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobProgressThrottle")
            .field("active_jobs", &self.last.len())
            .finish()
    }
}

impl Default for JobProgressThrottle {
    fn default() -> Self {
        Self {
            last: HashMap::new(),
        }
    }
}

impl JobProgressThrottle {
    pub fn allow(&mut self, job_id: &str) -> bool {
        let now = Instant::now();
        let min_gap = Duration::from_millis(100);
        if let Some(prev) = self.last.get(job_id) {
            if now.duration_since(*prev) < min_gap {
                return false;
            }
        }
        self.last.insert(job_id.to_string(), now);
        true
    }

    pub fn remove(&mut self, job_id: &str) {
        self.last.remove(job_id);
    }

    pub fn clear(&mut self) {
        self.last.clear();
    }
}

pub fn init_state_from_schema(
    state: &aos_proto::rich_decl_ui::RichStateDecl,
) -> (HashMap<String, Value>, HashMap<String, Value>) {
    let mut local = HashMap::new();
    let mut document = HashMap::new();
    for (k, slot) in &state.local {
        if let Some(def) = &slot.default {
            local.insert(k.clone(), def.clone());
        } else {
            local.insert(k.clone(), default_for_slot(slot));
        }
    }
    for (k, slot) in &state.document {
        if let Some(def) = &slot.default {
            document.insert(k.clone(), def.clone());
        } else {
            document.insert(k.clone(), default_for_slot(slot));
        }
    }
    (local, document)
}

fn default_for_slot(slot: &aos_proto::rich_decl_ui::RichStateSlot) -> Value {
    match slot.slot_type.as_str() {
        "string" => Value::String(String::new()),
        "number" => json!(0),
        "boolean" => Value::Bool(false),
        "array" => Value::Array(Vec::new()),
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscription_cleanup_on_close() {
        let mut subs = RichDeclSubscriptions::default();
        subs.register("job_main");
        subs.register("job_secondary");
        subs.set_job(
            "job_main",
            RichJobHandle {
                job_id: Some("demo-1".into()),
                state: Some("running".into()),
                ..Default::default()
            },
        );
        assert_eq!(subs.len(), 2);
        subs.clear();
        assert!(subs.is_empty());
        assert!(subs.job("job_main").is_none());
    }

    #[test]
    fn image_view_semantic_phases_do_not_require_wasm() {
        let mut st = ImageViewInteractionState::default();
        st.apply_semantic(&RichInteractionEvent {
            phase: "start".into(),
            interaction_id: "iv-1".into(),
            value: json!({}),
        });
        st.apply_semantic(&RichInteractionEvent {
            phase: "update".into(),
            interaction_id: "iv-1".into(),
            value: json!({"zoom": 1.5, "pan_x": 4.0, "pan_y": -2.0}),
        });
        assert!((st.zoom - 1.5).abs() < f32::EPSILON);
        st.apply_semantic(&RichInteractionEvent {
            phase: "commit".into(),
            interaction_id: "iv-1".into(),
            value: json!({}),
        });
        assert!(!st.active);
    }

    #[test]
    fn demo_job_reaches_terminal_states() {
        let done = demo_job_tick("j1", 5, 5, false);
        assert_eq!(done.state.as_deref(), Some("succeeded"));
        let cancelled = demo_job_tick("j1", 2, 5, true);
        assert_eq!(cancelled.state.as_deref(), Some("cancelled"));
    }
}
