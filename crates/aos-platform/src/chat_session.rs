//! Sessions de conversation persistées (Preview PC.6).

use aos_proto::{
    align_canvas_op_body, canvas_layer_effective_locked, canvas_op_bbox, ensure_canvas_layers,
    normalize_canvas_color, normalize_canvas_op_coords, resolve_canvas_op_style_ex,
    set_canvas_op_body_dash, set_canvas_op_body_gradient, set_canvas_op_body_opacity,
    set_canvas_op_rotation, translate_canvas_op_body, usable_canvas_bbox, CanvasAspect, CanvasDoc,
    CanvasEdit, CanvasLayer, CanvasLinearGradient, CanvasOp, CanvasOpBody, CanvasPenStyle,
    ChatAttachment, ChatRoomConductorPolicy, ChatRoomMember, ChatSessionMessage, ChatSessionMeta,
    ChatSessionMode,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum SessionError {
    NotFound(String),
    Io(String),
    BadRequest(String),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(s) => write!(f, "session inconnue: {s}"),
            Self::Io(s) => write!(f, "io: {s}"),
            Self::BadRequest(s) => write!(f, "{s}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MetaFile {
    id: String,
    title: String,
    created_ms: u64,
    updated_ms: u64,
    archived: bool,
    #[serde(default)]
    model_id: Option<String>,
    #[serde(default)]
    mode: ChatSessionMode,
    #[serde(default)]
    members: Vec<ChatRoomMember>,
    #[serde(default)]
    conductor_policy: ChatRoomConductorPolicy,
    #[serde(default)]
    canvas_open: bool,
    #[serde(default)]
    canvas_aspect: CanvasAspect,
}

/// Magasin de sessions chat sous `var/sessions/<id>/`.
pub struct ChatSessionStore {
    root: PathBuf,
}

impl ChatSessionStore {
    pub fn open(root: impl AsRef<Path>) -> std::io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    fn dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    fn load_meta(&self, id: &str) -> Result<MetaFile, SessionError> {
        let p = self.dir(id).join("meta.yaml");
        let raw = fs::read_to_string(&p).map_err(|_| SessionError::NotFound(id.into()))?;
        serde_yaml::from_str(&raw).map_err(|e| SessionError::Io(e.to_string()))
    }

    fn save_meta(&self, meta: &MetaFile) -> Result<(), SessionError> {
        let dir = self.dir(&meta.id);
        fs::create_dir_all(&dir).map_err(|e| SessionError::Io(e.to_string()))?;
        let raw = serde_yaml::to_string(meta).map_err(|e| SessionError::Io(e.to_string()))?;
        fs::write(dir.join("meta.yaml"), raw).map_err(|e| SessionError::Io(e.to_string()))
    }

    fn count_messages(&self, id: &str) -> usize {
        let p = self.dir(id).join("messages.jsonl");
        fs::read_to_string(p)
            .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0)
    }

    fn to_public(&self, m: MetaFile) -> ChatSessionMeta {
        let message_count = self.count_messages(&m.id);
        ChatSessionMeta {
            id: m.id,
            title: m.title,
            created_ms: m.created_ms,
            updated_ms: m.updated_ms,
            archived: m.archived,
            message_count,
            model_id: m.model_id,
            mode: m.mode,
            members: m.members,
            conductor_policy: m.conductor_policy,
            canvas_open: m.canvas_open,
            canvas_aspect: m.canvas_aspect,
        }
    }

    fn canvas_path(&self, id: &str) -> PathBuf {
        self.dir(id).join("canvas.json")
    }

    fn load_canvas(&self, id: &str) -> Result<CanvasDoc, SessionError> {
        let p = self.canvas_path(id);
        if !p.exists() {
            let mut doc = CanvasDoc {
                session_id: id.into(),
                next_seq: 1,
                ops: Vec::new(),
                pen: CanvasPenStyle::default(),
                ..Default::default()
            };
            ensure_canvas_layers(&mut doc);
            return Ok(doc);
        }
        let raw = fs::read_to_string(&p).map_err(|e| SessionError::Io(e.to_string()))?;
        let mut doc: CanvasDoc =
            serde_json::from_str(&raw).map_err(|e| SessionError::Io(e.to_string()))?;
        if doc.session_id.is_empty() {
            doc.session_id = id.into();
        }
        if doc.next_seq == 0 {
            doc.next_seq = doc.ops.iter().map(|o| o.seq).max().unwrap_or(0) + 1;
        }
        ensure_canvas_layers(&mut doc);
        Ok(doc)
    }

    fn save_canvas(&self, doc: &CanvasDoc) -> Result<(), SessionError> {
        let dir = self.dir(&doc.session_id);
        fs::create_dir_all(&dir).map_err(|e| SessionError::Io(e.to_string()))?;
        let raw = serde_json::to_string_pretty(doc).map_err(|e| SessionError::Io(e.to_string()))?;
        fs::write(self.canvas_path(&doc.session_id), raw).map_err(|e| SessionError::Io(e.to_string()))
    }

    /// Lecture du document canvas (+ filtre optionnel `after_seq`).
    pub fn canvas_get(
        &self,
        id: &str,
        after_seq: Option<u64>,
    ) -> Result<(ChatSessionMeta, CanvasDoc, Vec<CanvasOp>), SessionError> {
        let meta = self.to_public(self.load_meta(id)?);
        let doc = self.load_canvas(id)?;
        let ops = match after_seq {
            Some(after) => doc.ops.iter().filter(|o| o.seq > after).cloned().collect(),
            None => doc.ops.clone(),
        };
        Ok((meta, doc, ops))
    }

    /// Applique une op ; ouvre automatiquement le canvas si besoin.
    pub fn canvas_apply(
        &self,
        id: &str,
        author_id: &str,
        body: CanvasOpBody,
    ) -> Result<(ChatSessionMeta, CanvasDoc, Option<CanvasOp>), SessionError> {
        if author_id.trim().is_empty() {
            return Err(SessionError::BadRequest("author_id requis".into()));
        }
        let _ = self.load_meta(id)?;
        let mut doc = self.load_canvas(id)?;
        doc.session_id = id.into();
        ensure_canvas_layers(&mut doc);
        let mut body = body;
        let applied = match &mut body {
            CanvasOpBody::Undo => {
                if let Some(pos) = doc.ops.iter().rposition(|o| o.author_id == author_id) {
                    doc.ops.remove(pos);
                }
                None
            }
            CanvasOpBody::Clear => {
                doc.ops.clear();
                None
            }
            _ => {
                if canvas_layer_effective_locked(&doc, &doc.active_layer_id) {
                    return Err(SessionError::BadRequest("calque verrouillé".into()));
                }
                normalize_canvas_op_coords(&mut body);
                resolve_canvas_op_style_ex(&mut body, &doc.pen);
                let op = CanvasOp {
                    seq: doc.next_seq,
                    author_id: author_id.into(),
                    ts_ms: Self::now_ms(),
                    layer_id: doc.active_layer_id.clone(),
                    body,
                };
                doc.next_seq = doc.next_seq.saturating_add(1);
                doc.ops.push(op.clone());
                Some(op)
            }
        };
        self.save_canvas(&doc)?;
        let mut meta = self.load_meta(id)?;
        if !meta.canvas_open {
            meta.canvas_open = true;
        }
        meta.updated_ms = Self::now_ms();
        self.save_meta(&meta)?;
        Ok((self.to_public(meta), doc, applied))
    }

    /// Replace the session canvas document from an exported sidecar (round-trip import).
    pub fn canvas_import(
        &self,
        id: &str,
        mut doc: CanvasDoc,
        aspect: Option<CanvasAspect>,
    ) -> Result<(ChatSessionMeta, CanvasDoc), SessionError> {
        let mut meta = self.load_meta(id)?;
        doc.session_id = id.into();
        ensure_canvas_layers(&mut doc);
        if doc.next_seq == 0 {
            doc.next_seq = doc.ops.iter().map(|o| o.seq).max().unwrap_or(0).saturating_add(1);
        }
        for op in &mut doc.ops {
            normalize_canvas_op_coords(&mut op.body);
            resolve_canvas_op_style_ex(&mut op.body, &doc.pen);
        }
        self.save_canvas(&doc)?;
        if let Some(a) = aspect {
            meta.canvas_aspect = a;
            self.save_meta(&meta)?;
        }
        Ok((self.to_public(meta), doc))
    }

    /// In-place object / layer mutation. Does not append a paint op.
    pub fn canvas_edit(
        &self,
        id: &str,
        author_id: &str,
        edit: CanvasEdit,
    ) -> Result<(ChatSessionMeta, CanvasDoc), SessionError> {
        let _ = author_id;
        let _ = self.load_meta(id)?;
        let mut doc = self.load_canvas(id)?;
        doc.session_id = id.into();
        ensure_canvas_layers(&mut doc);
        match edit {
            CanvasEdit::Delete { seq } => {
                let Some(pos) = doc.ops.iter().position(|o| o.seq == seq) else {
                    return Err(SessionError::BadRequest("seq inconnue".into()));
                };
                if canvas_layer_effective_locked(&doc, &doc.ops[pos].layer_id) {
                    return Err(SessionError::BadRequest("calque verrouillé".into()));
                }
                doc.ops.remove(pos);
            }
            CanvasEdit::Move { seq, dx, dy } => {
                let layer_id = doc
                    .ops
                    .iter()
                    .find(|o| o.seq == seq)
                    .map(|o| o.layer_id.clone())
                    .ok_or_else(|| SessionError::BadRequest("seq inconnue".into()))?;
                if canvas_layer_effective_locked(&doc, &layer_id) {
                    return Err(SessionError::BadRequest("calque verrouillé".into()));
                }
                let op = doc
                    .ops
                    .iter_mut()
                    .find(|o| o.seq == seq)
                    .expect("seq");
                translate_canvas_op_body(&mut op.body, dx, dy);
            }
            CanvasEdit::Reorder { seq, z } => {
                let Some(pos) = doc.ops.iter().position(|o| o.seq == seq) else {
                    return Err(SessionError::BadRequest("seq inconnue".into()));
                };
                if canvas_layer_effective_locked(&doc, &doc.ops[pos].layer_id) {
                    return Err(SessionError::BadRequest("calque verrouillé".into()));
                }
                let op = doc.ops.remove(pos);
                let z = z.clamp(0, doc.ops.len() as i64) as usize;
                doc.ops.insert(z, op);
            }
            CanvasEdit::Restyle {
                seq,
                color,
                width,
                fill,
                rotation,
                opacity,
                dash,
                gradient,
            } => {
                let layer_id = doc
                    .ops
                    .iter()
                    .find(|o| o.seq == seq)
                    .map(|o| o.layer_id.clone())
                    .ok_or_else(|| SessionError::BadRequest("seq inconnue".into()))?;
                if canvas_layer_effective_locked(&doc, &layer_id) {
                    return Err(SessionError::BadRequest("calque verrouillé".into()));
                }
                let op = doc
                    .ops
                    .iter_mut()
                    .find(|o| o.seq == seq)
                    .expect("seq");
                restyle_op_body(
                    &mut op.body,
                    color.as_deref(),
                    width,
                    fill,
                    rotation,
                    opacity,
                    dash,
                    gradient,
                )?;
            }
            CanvasEdit::Rotate { seq, rotation } => {
                let layer_id = doc
                    .ops
                    .iter()
                    .find(|o| o.seq == seq)
                    .map(|o| o.layer_id.clone())
                    .ok_or_else(|| SessionError::BadRequest("seq inconnue".into()))?;
                if canvas_layer_effective_locked(&doc, &layer_id) {
                    return Err(SessionError::BadRequest("calque verrouillé".into()));
                }
                let op = doc
                    .ops
                    .iter_mut()
                    .find(|o| o.seq == seq)
                    .expect("seq");
                set_canvas_op_rotation(&mut op.body, rotation)
                    .map_err(SessionError::BadRequest)?;
            }
            CanvasEdit::Align {
                seq,
                to_seq,
                edges,
            } => {
                if edges.is_empty() {
                    return Err(SessionError::BadRequest("edges requis".into()));
                }
                let src_idx = doc
                    .ops
                    .iter()
                    .position(|o| o.seq == seq)
                    .ok_or_else(|| SessionError::BadRequest("seq inconnue".into()))?;
                if canvas_layer_effective_locked(&doc, &doc.ops[src_idx].layer_id) {
                    return Err(SessionError::BadRequest("calque verrouillé".into()));
                }
                let src_bbox = canvas_op_bbox(&doc.ops[src_idx].body)
                    .ok_or_else(|| SessionError::BadRequest("bbox source".into()))?;
                let target = if let Some(to) = to_seq {
                    let other = doc
                        .ops
                        .iter()
                        .find(|o| o.seq == to)
                        .ok_or_else(|| SessionError::BadRequest("to_seq inconnue".into()))?;
                    canvas_op_bbox(&other.body)
                        .ok_or_else(|| SessionError::BadRequest("bbox cible".into()))?
                } else {
                    usable_canvas_bbox()
                };
                align_canvas_op_body(&mut doc.ops[src_idx].body, src_bbox, target, &edges);
            }
            CanvasEdit::LayerCreate { name, parent_id } => {
                if let Some(parent) = parent_id.as_deref() {
                    if !doc.layers.iter().any(|l| l.id == parent) {
                        return Err(SessionError::BadRequest("parent inconnu".into()));
                    }
                }
                let n = doc.next_layer_id.max(2);
                let layer_id = format!("lyr-{n}");
                doc.next_layer_id = n.saturating_add(1);
                let label = name
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| format!("Layer {}", doc.layers.len() + 1));
                doc.layers.push(CanvasLayer {
                    id: layer_id.clone(),
                    name: label,
                    parent_id,
                    visible: true,
                    locked: false,
                    opacity: 1.0,
                });
                doc.active_layer_id = layer_id;
            }
            CanvasEdit::LayerRename { id: layer_id, name } => {
                let Some(layer) = doc.layers.iter_mut().find(|l| l.id == layer_id) else {
                    return Err(SessionError::BadRequest("calque inconnu".into()));
                };
                if name.trim().is_empty() {
                    return Err(SessionError::BadRequest("nom requis".into()));
                }
                layer.name = name;
            }
            CanvasEdit::LayerSet {
                id: layer_id,
                visible,
                locked,
                opacity,
            } => {
                let Some(layer) = doc.layers.iter_mut().find(|l| l.id == layer_id) else {
                    return Err(SessionError::BadRequest("calque inconnu".into()));
                };
                if let Some(v) = visible {
                    layer.visible = v;
                }
                if let Some(v) = locked {
                    layer.locked = v;
                }
                if let Some(v) = opacity {
                    layer.opacity = v.clamp(0.0, 1.0);
                }
            }
            CanvasEdit::LayerReorder {
                id: layer_id,
                parent_id,
                z,
            } => {
                if !doc.layers.iter().any(|l| l.id == layer_id) {
                    return Err(SessionError::BadRequest("calque inconnu".into()));
                }
                if let Some(parent) = parent_id.as_deref() {
                    if parent == layer_id || !doc.layers.iter().any(|l| l.id == parent) {
                        return Err(SessionError::BadRequest("parent invalide".into()));
                    }
                }
                if let Some(layer) = doc.layers.iter_mut().find(|l| l.id == layer_id) {
                    layer.parent_id = parent_id;
                }
                let pos = doc
                    .layers
                    .iter()
                    .position(|l| l.id == layer_id)
                    .expect("layer");
                let layer = doc.layers.remove(pos);
                let z = z.clamp(0, doc.layers.len() as i64) as usize;
                doc.layers.insert(z, layer);
            }
            CanvasEdit::LayerDelete { id: layer_id } => {
                if doc.layers.len() <= 1 {
                    return Err(SessionError::BadRequest("dernier calque".into()));
                }
                let Some(idx) = doc.layers.iter().position(|l| l.id == layer_id) else {
                    return Err(SessionError::BadRequest("calque inconnu".into()));
                };
                let removed = doc.layers.remove(idx);
                let fallback = removed
                    .parent_id
                    .clone()
                    .filter(|p| doc.layers.iter().any(|l| l.id == *p))
                    .unwrap_or_else(|| doc.layers[0].id.clone());
                for child in doc.layers.iter_mut().filter(|l| l.parent_id.as_deref() == Some(layer_id.as_str()))
                {
                    child.parent_id = removed.parent_id.clone();
                }
                for op in &mut doc.ops {
                    if op.layer_id == layer_id {
                        op.layer_id = fallback.clone();
                    }
                }
                if doc.active_layer_id == layer_id {
                    doc.active_layer_id = fallback;
                }
            }
            CanvasEdit::LayerActivate { id: layer_id } => {
                if !doc.layers.iter().any(|l| l.id == layer_id) {
                    return Err(SessionError::BadRequest("calque inconnu".into()));
                }
                doc.active_layer_id = layer_id;
            }
        }
        self.save_canvas(&doc)?;
        let mut meta = self.load_meta(id)?;
        if !meta.canvas_open {
            meta.canvas_open = true;
        }
        meta.updated_ms = Self::now_ms();
        self.save_meta(&meta)?;
        Ok((self.to_public(meta), doc))
    }

    /// Met à jour le crayon de session (couleur / épaisseur).
    pub fn canvas_set_style(
        &self,
        id: &str,
        color: Option<&str>,
        width: Option<f32>,
    ) -> Result<(ChatSessionMeta, CanvasDoc), SessionError> {
        let _ = self.load_meta(id)?;
        let mut doc = self.load_canvas(id)?;
        doc.session_id = id.into();
        if let Some(c) = color {
            let normalized = normalize_canvas_color(c)
                .ok_or_else(|| SessionError::BadRequest("color invalide (#RRGGBB)".into()))?;
            doc.pen.color = normalized;
        }
        if let Some(w) = width {
            if w <= 0.0 {
                return Err(SessionError::BadRequest("width doit être > 0".into()));
            }
            doc.pen.width = w.clamp(0.001, 0.25);
        }
        self.save_canvas(&doc)?;
        let mut meta = self.load_meta(id)?;
        meta.updated_ms = Self::now_ms();
        self.save_meta(&meta)?;
        Ok((self.to_public(meta), doc))
    }

    pub fn canvas_set_open(
        &self,
        id: &str,
        open: bool,
    ) -> Result<ChatSessionMeta, SessionError> {
        let mut meta = self.load_meta(id)?;
        meta.canvas_open = open;
        meta.updated_ms = Self::now_ms();
        self.save_meta(&meta)?;
        if open {
            // Ensure canvas.json exists so UI poll has a stable document.
            let doc = self.load_canvas(id)?;
            self.save_canvas(&doc)?;
        }
        Ok(self.to_public(meta))
    }

    pub fn canvas_set_aspect(
        &self,
        id: &str,
        aspect: CanvasAspect,
    ) -> Result<ChatSessionMeta, SessionError> {
        let mut meta = self.load_meta(id)?;
        meta.canvas_aspect = aspect;
        meta.updated_ms = Self::now_ms();
        self.save_meta(&meta)?;
        Ok(self.to_public(meta))
    }

    pub fn create(
        &self,
        title: Option<String>,
        model_id: Option<String>,
    ) -> Result<ChatSessionMeta, SessionError> {
        let ts = Self::now_ms();
        let id = format!("sess-{ts}");
        let title = title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| format!("Session {}", &id[5..]));
        let meta = MetaFile {
            id: id.clone(),
            title,
            created_ms: ts,
            updated_ms: ts,
            archived: false,
            model_id,
            mode: ChatSessionMode::Direct,
            members: vec![],
            conductor_policy: ChatRoomConductorPolicy::default(),
            canvas_open: false,
            canvas_aspect: CanvasAspect::default(),
        };
        self.save_meta(&meta)?;
        let _ = fs::write(self.dir(&id).join("messages.jsonl"), "");
        Ok(self.to_public(meta))
    }

    pub fn list(&self, include_archived: bool) -> Result<Vec<ChatSessionMeta>, SessionError> {
        let mut out = Vec::new();
        let rd = fs::read_dir(&self.root).map_err(|e| SessionError::Io(e.to_string()))?;
        for entry in rd.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().to_string();
            if let Ok(m) = self.load_meta(&id) {
                if include_archived || !m.archived {
                    out.push(self.to_public(m));
                }
            }
        }
        out.sort_by(|a, b| b.updated_ms.cmp(&a.updated_ms));
        Ok(out)
    }

    pub fn get(
        &self,
        id: &str,
    ) -> Result<(ChatSessionMeta, Vec<ChatSessionMessage>), SessionError> {
        let meta = self.load_meta(id)?;
        let path = self.dir(id).join("messages.jsonl");
        let mut messages = Vec::new();
        if let Ok(raw) = fs::read_to_string(path) {
            for line in raw.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(m) = serde_json::from_str::<ChatSessionMessage>(line) {
                    messages.push(m);
                }
            }
        }
        Ok((self.to_public(meta), messages))
    }

    pub fn append(
        &self,
        id: &str,
        role: &str,
        content: &str,
        attachments: Vec<ChatAttachment>,
        speaker_id: Option<String>,
        speaker_name: Option<String>,
        thinking: Option<String>,
    ) -> Result<ChatSessionMessage, SessionError> {
        if role.is_empty() || content.is_empty() {
            return Err(SessionError::BadRequest("role/content requis".into()));
        }
        let mut meta = self.load_meta(id)?;
        let msg = ChatSessionMessage {
            role: role.into(),
            content: content.into(),
            ts_ms: Self::now_ms(),
            attachments,
            speaker_id,
            speaker_name,
            thinking,
        };
        let path = self.dir(id).join("messages.jsonl");
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| SessionError::Io(e.to_string()))?;
        writeln!(f, "{}", serde_json::to_string(&msg).unwrap())
            .map_err(|e| SessionError::Io(e.to_string()))?;
        meta.updated_ms = msg.ts_ms;
        self.save_meta(&meta)?;
        Ok(msg)
    }

    pub fn set_model(
        &self,
        id: &str,
        model_id: Option<String>,
    ) -> Result<ChatSessionMeta, SessionError> {
        let mut meta = self.load_meta(id)?;
        meta.model_id = model_id;
        meta.updated_ms = Self::now_ms();
        self.save_meta(&meta)?;
        Ok(self.to_public(meta))
    }

    pub fn set_mode(
        &self,
        id: &str,
        mode: ChatSessionMode,
    ) -> Result<ChatSessionMeta, SessionError> {
        let mut meta = self.load_meta(id)?;
        meta.mode = mode;
        meta.updated_ms = Self::now_ms();
        self.save_meta(&meta)?;
        Ok(self.to_public(meta))
    }

    pub fn members_add(
        &self,
        id: &str,
        member: ChatRoomMember,
    ) -> Result<ChatSessionMeta, SessionError> {
        if member.agent_id.trim().is_empty() {
            return Err(SessionError::BadRequest("agent_id requis".into()));
        }
        if member.display_name.trim().is_empty() {
            return Err(SessionError::BadRequest("display_name requis".into()));
        }
        let mut meta = self.load_meta(id)?;
        if meta.members.iter().any(|m| m.agent_id == member.agent_id) {
            return Err(SessionError::BadRequest("membre déjà présent".into()));
        }
        meta.members.push(member);
        meta.updated_ms = Self::now_ms();
        self.save_meta(&meta)?;
        Ok(self.to_public(meta))
    }

    pub fn members_remove(
        &self,
        id: &str,
        agent_id: &str,
    ) -> Result<ChatSessionMeta, SessionError> {
        if agent_id.trim().is_empty() {
            return Err(SessionError::BadRequest("agent_id requis".into()));
        }
        let mut meta = self.load_meta(id)?;
        let before = meta.members.len();
        meta.members.retain(|m| m.agent_id != agent_id);
        if meta.members.len() == before {
            return Err(SessionError::BadRequest("membre introuvable".into()));
        }
        meta.updated_ms = Self::now_ms();
        self.save_meta(&meta)?;
        Ok(self.to_public(meta))
    }

    pub fn members_list(&self, id: &str) -> Result<Vec<ChatRoomMember>, SessionError> {
        let meta = self.load_meta(id)?;
        Ok(meta.members)
    }

    pub fn rename(&self, id: &str, title: &str) -> Result<ChatSessionMeta, SessionError> {
        let mut meta = self.load_meta(id)?;
        meta.title = title.into();
        meta.updated_ms = Self::now_ms();
        self.save_meta(&meta)?;
        Ok(self.to_public(meta))
    }

    pub fn archive(&self, id: &str) -> Result<ChatSessionMeta, SessionError> {
        let mut meta = self.load_meta(id)?;
        meta.archived = true;
        meta.updated_ms = Self::now_ms();
        self.save_meta(&meta)?;
        Ok(self.to_public(meta))
    }

    pub fn delete(&self, id: &str) -> Result<(), SessionError> {
        let dir = self.dir(id);
        if !dir.exists() {
            return Err(SessionError::NotFound(id.into()));
        }
        fs::remove_dir_all(dir).map_err(|e| SessionError::Io(e.to_string()))
    }

    /// Export markdown d'une session.
    pub fn export_markdown(&self, id: &str) -> Result<String, SessionError> {
        let (meta, messages) = self.get(id)?;
        let mut out = format!("# {}\n\n", meta.title);
        for m in messages {
            let heading = match m.speaker_name.as_deref().filter(|s| !s.trim().is_empty()) {
                Some(name) => format!("## {} — {}\n\n", m.role, name),
                None => format!("## {}\n\n", m.role),
            };
            out.push_str(&heading);
            out.push_str(&format!("{}\n\n", m.content));
            if let Some(thinking) = m.thinking.as_deref().filter(|s| !s.trim().is_empty()) {
                out.push_str("### Thinking\n\n");
                out.push_str(thinking.trim());
                out.push_str("\n\n");
            }
            for att in &m.attachments {
                match att {
                    ChatAttachment::AgentRef {
                        agent_id,
                        title,
                        origin,
                    } => {
                        out.push_str(&format!(
                            "_agent: {agent_id} ({origin}) — {title}_\n\n"
                        ));
                    }
                    ChatAttachment::Image { path, prompt } => {
                        out.push_str(&format!("_image: {path}_\n\n"));
                        if !prompt.is_empty() {
                            out.push_str(&format!("_prompt: {prompt}_\n\n"));
                        }
                    }
                    ChatAttachment::Audio { path } => {
                        out.push_str(&format!("_audio: {path}_\n\n"));
                    }
                    ChatAttachment::TtsDraft { text, .. } => {
                        out.push_str(&format!("_tts draft: {text}_\n\n"));
                    }
                    ChatAttachment::Document { path, label } => {
                        out.push_str(&format!("_document: {label} ({path})_\n\n"));
                    }
                    ChatAttachment::AgentAct {
                        action,
                        args,
                        phrase,
                        ..
                    } => {
                        let label = if action.is_empty() {
                            phrase.as_str()
                        } else {
                            action.as_str()
                        };
                        out.push_str(&format!("_agent act: {label}_\n\n"));
                        if !args.is_null() && args != &serde_json::json!({}) {
                            out.push_str(&format!("_{args}_\n\n"));
                        }
                    }
                    ChatAttachment::SkillOffer {
                        label_en,
                        label_fr,
                        ..
                    } => {
                        out.push_str(&format!("_skill offer: {label_en} / {label_fr}_\n\n"));
                    }
                    _ => {}
                }
            }
        }
        Ok(out)
    }
}

fn restyle_op_body(
    body: &mut CanvasOpBody,
    color: Option<&str>,
    width: Option<f32>,
    fill: Option<bool>,
    rotation: Option<f32>,
    opacity: Option<f32>,
    dash: Option<Vec<f32>>,
    gradient: Option<Option<CanvasLinearGradient>>,
) -> Result<(), SessionError> {
    if let Some(c) = color {
        let normalized = normalize_canvas_color(c)
            .ok_or_else(|| SessionError::BadRequest("color invalide (#RRGGBB)".into()))?;
        match body {
            CanvasOpBody::Stroke { color, .. }
            | CanvasOpBody::Rect { color, .. }
            | CanvasOpBody::Ellipse { color, .. }
            | CanvasOpBody::Line { color, .. }
            | CanvasOpBody::Spline { color, .. }
            | CanvasOpBody::Path { color, .. }
            | CanvasOpBody::Fill { color, .. } => *color = normalized,
            CanvasOpBody::Erase { .. } | CanvasOpBody::Clear | CanvasOpBody::Undo => {}
        }
    }
    if let Some(w) = width {
        let w = w.clamp(0.001, 0.25);
        match body {
            CanvasOpBody::Stroke { width, .. }
            | CanvasOpBody::Rect { width, .. }
            | CanvasOpBody::Ellipse { width, .. }
            | CanvasOpBody::Line { width, .. }
            | CanvasOpBody::Spline { width, .. }
            | CanvasOpBody::Path { width, .. }
            | CanvasOpBody::Erase { width, .. } => *width = w,
            CanvasOpBody::Fill { .. } | CanvasOpBody::Clear | CanvasOpBody::Undo => {}
        }
    }
    if let Some(fill) = fill {
        match body {
            CanvasOpBody::Rect { fill: slot, .. }
            | CanvasOpBody::Ellipse { fill: slot, .. }
            | CanvasOpBody::Path { fill: slot, .. } => *slot = fill,
            _ => {}
        }
    }
    if let Some(rotation) = rotation {
        set_canvas_op_rotation(body, rotation).map_err(SessionError::BadRequest)?;
    }
    if let Some(o) = opacity {
        set_canvas_op_body_opacity(body, o);
    }
    if let Some(d) = dash {
        set_canvas_op_body_dash(body, d);
    }
    if let Some(g) = gradient {
        set_canvas_op_body_gradient(body, g);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_append_list() {
        let dir = std::env::temp_dir().join(format!("aos-sess-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = ChatSessionStore::open(&dir).unwrap();
        let m = s.create(Some("Test".into()), None).unwrap();
        s.append(&m.id, "user", "bonjour", vec![], None, None, None).unwrap();
        s.append(&m.id, "assistant", "salut", vec![], None, None, None).unwrap();
        let (meta, msgs) = s.get(&m.id).unwrap();
        assert_eq!(meta.message_count, 2);
        assert_eq!(msgs.len(), 2);
        assert_eq!(s.list(false).unwrap().len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_jsonl_without_attachments() {
        let dir = std::env::temp_dir().join(format!("aos-sess-leg-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = ChatSessionStore::open(&dir).unwrap();
        let m = s.create(Some("Legacy".into()), None).unwrap();
        let path = dir.join(&m.id).join("messages.jsonl");
        fs::write(
            &path,
            r#"{"role":"user","content":"hi","ts_ms":1}
{"role":"assistant","content":"yo","ts_ms":2}
"#,
        )
        .unwrap();
        let (_, msgs) = s.get(&m.id).unwrap();
        assert_eq!(msgs.len(), 2);
        assert!(msgs[0].attachments.is_empty());
        s.append(
            &m.id,
            "assistant",
            "agent lancé",
            vec![ChatAttachment::AgentRef {
                agent_id: "agent-1".into(),
                title: "tâche".into(),
                origin: "slash".into(),
            }],
            None,
            None,
            None,
        )
        .unwrap();
        let (_, msgs2) = s.get(&m.id).unwrap();
        assert_eq!(msgs2.len(), 3);
        assert_eq!(msgs2[2].attachments.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_meta_yaml_without_room_fields() {
        let dir = std::env::temp_dir().join(format!("aos-sess-meta-leg-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = ChatSessionStore::open(&dir).unwrap();
        let m = s.create(Some("Legacy meta".into()), None).unwrap();
        let meta_path = dir.join(&m.id).join("meta.yaml");
        fs::write(
            &meta_path,
            format!(
                r#"id: {id}
title: Legacy meta
created_ms: 1
updated_ms: 2
archived: false
"#,
                id = m.id
            ),
        )
        .unwrap();
        let (meta, _) = s.get(&m.id).unwrap();
        assert_eq!(meta.mode, ChatSessionMode::Direct);
        assert!(meta.members.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn members_add_remove_and_mode() {
        let dir = std::env::temp_dir().join(format!("aos-sess-room-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = ChatSessionStore::open(&dir).unwrap();
        let m = s.create(Some("Room".into()), None).unwrap();
        let meta = s
            .set_mode(&m.id, ChatSessionMode::Room)
            .expect("set_mode");
        assert_eq!(meta.mode, ChatSessionMode::Room);
        s.members_add(
            &m.id,
            ChatRoomMember {
                agent_id: "agent-a".into(),
                display_name: "Alpha".into(),
                persona_id: Some("p1".into()),
                joined_ms: 100,
            },
        )
        .unwrap();
        s.members_add(
            &m.id,
            ChatRoomMember {
                agent_id: "agent-b".into(),
                display_name: "Beta".into(),
                persona_id: None,
                joined_ms: 101,
            },
        )
        .unwrap();
        let members = s.members_list(&m.id).unwrap();
        assert_eq!(members.len(), 2);
        s.members_remove(&m.id, "agent-a").unwrap();
        let members = s.members_list(&m.id).unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].agent_id, "agent-b");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_with_speaker() {
        let dir = std::env::temp_dir().join(format!("aos-sess-spk-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = ChatSessionStore::open(&dir).unwrap();
        let m = s.create(Some("Speakers".into()), None).unwrap();
        s.append(
            &m.id,
            "assistant",
            "alpha says hi",
            vec![],
            Some("agent-a".into()),
            Some("Alpha".into()),
            None,
        )
        .unwrap();
        let (_, msgs) = s.get(&m.id).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].speaker_id.as_deref(), Some("agent-a"));
        assert_eq!(msgs[0].speaker_name.as_deref(), Some("Alpha"));
        let md = s.export_markdown(&m.id).unwrap();
        assert!(md.contains("## assistant — Alpha"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_markdown_includes_thinking() {
        let dir = std::env::temp_dir().join(format!("aos-sess-think-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = ChatSessionStore::open(&dir).unwrap();
        let m = s.create(Some("Think".into()), None).unwrap();
        s.append(
            &m.id,
            "assistant",
            "réponse",
            vec![],
            None,
            None,
            Some("raison interne".into()),
        )
        .unwrap();
        let md = s.export_markdown(&m.id).unwrap();
        assert!(md.contains("### Thinking"));
        assert!(md.contains("raison interne"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn canvas_apply_undo_persist_and_open() {
        let dir = std::env::temp_dir().join(format!("aos-sess-canvas-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = ChatSessionStore::open(&dir).unwrap();
        let m = s.create(Some("Canvas".into()), None).unwrap();
        assert!(!m.canvas_open);
        let (meta, doc, applied) = s
            .canvas_apply(
                &m.id,
                "human",
                CanvasOpBody::Stroke {
                    points: vec![
                        aos_proto::CanvasPoint { x: 0.1, y: 0.1 },
                        aos_proto::CanvasPoint { x: 0.5, y: 0.5 },
                    ],
                    color: "#3ee0c4".into(),
                    width: 0.02,
                    opacity: 1.0,
                    dash: vec![],
                },
            )
            .unwrap();
        assert!(meta.canvas_open);
        assert!(applied.is_some());
        assert_eq!(doc.ops.len(), 1);
        let (_, _, delta) = s.canvas_get(&m.id, Some(0)).unwrap();
        assert_eq!(delta.len(), 1);
        let (_, doc2, _) = s
            .canvas_apply(&m.id, "human", CanvasOpBody::Undo)
            .unwrap();
        assert!(doc2.ops.is_empty());
        let meta = s.canvas_set_open(&m.id, false).unwrap();
        assert!(!meta.canvas_open);
        let meta = s.canvas_set_aspect(&m.id, CanvasAspect::Landscape16x9).unwrap();
        assert_eq!(meta.canvas_aspect, CanvasAspect::Landscape16x9);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn canvas_undo_removes_last_human_op_not_agent() {
        let dir = std::env::temp_dir().join(format!("aos-sess-undo-human-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = ChatSessionStore::open(&dir).unwrap();
        let m = s.create(Some("Undo".into()), None).unwrap();
        let stroke = CanvasOpBody::Stroke {
            points: vec![
                aos_proto::CanvasPoint { x: 0.1, y: 0.1 },
                aos_proto::CanvasPoint { x: 0.2, y: 0.2 },
            ],
            color: "#3ee0c4".into(),
            width: 0.02,
            opacity: 1.0,
            dash: vec![],
        };
        s.canvas_apply(&m.id, "human", stroke.clone()).unwrap();
        s.canvas_apply(&m.id, "agent-a", stroke).unwrap();
        let (_, doc, _) = s.canvas_apply(&m.id, "human", CanvasOpBody::Undo).unwrap();
        assert_eq!(doc.ops.len(), 1);
        assert_eq!(doc.ops[0].author_id, "agent-a");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn canvas_undo_removes_last_op_of_caller() {
        let dir = std::env::temp_dir().join(format!("aos-sess-undo-agent-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = ChatSessionStore::open(&dir).unwrap();
        let m = s.create(Some("UndoAgent".into()), None).unwrap();
        let stroke = CanvasOpBody::Stroke {
            points: vec![
                aos_proto::CanvasPoint { x: 0.1, y: 0.1 },
                aos_proto::CanvasPoint { x: 0.2, y: 0.2 },
            ],
            color: "#3ee0c4".into(),
            width: 0.02,
            opacity: 1.0,
            dash: vec![],
        };
        s.canvas_apply(&m.id, "human", stroke.clone()).unwrap();
        s.canvas_apply(&m.id, "agent-a", stroke).unwrap();
        let (_, doc, _) = s.canvas_apply(&m.id, "agent-a", CanvasOpBody::Undo).unwrap();
        assert_eq!(doc.ops.len(), 1);
        assert_eq!(doc.ops[0].author_id, "human");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn canvas_edit_delete_move_and_hide_layer() {
        let dir = std::env::temp_dir().join(format!("aos-sess-edit-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = ChatSessionStore::open(&dir).unwrap();
        let m = s.create(Some("Edit".into()), None).unwrap();
        let (_, doc, applied) = s
            .canvas_apply(
                &m.id,
                "human",
                CanvasOpBody::Rect {
                    x: 0.2,
                    y: 0.2,
                    w: 0.2,
                    h: 0.2,
                    color: "#3ee0c4".into(),
                    fill: true,
                    width: 0.01,
                    rotation: 0.0,
                    opacity: 1.0,
                    dash: vec![],
                    gradient: None,
                },
            )
            .unwrap();
        let seq = applied.unwrap().seq;
        assert_eq!(doc.layers.len(), 1);
        let (_, doc) = s
            .canvas_edit(
                &m.id,
                "human",
                CanvasEdit::Move {
                    seq,
                    dx: 0.1,
                    dy: 0.0,
                },
            )
            .unwrap();
        match &doc.ops[0].body {
            CanvasOpBody::Rect { x, .. } => assert!((*x - 0.3).abs() < 1e-5),
            _ => panic!("rect"),
        }
        let (_, doc) = s
            .canvas_edit(
                &m.id,
                "human",
                CanvasEdit::LayerCreate {
                    name: Some("Roof".into()),
                    parent_id: None,
                },
            )
            .unwrap();
        assert_eq!(doc.layers.len(), 2);
        let roof = doc.layers.iter().find(|l| l.name == "Roof").unwrap().id.clone();
        let (_, doc) = s
            .canvas_edit(
                &m.id,
                "human",
                CanvasEdit::LayerSet {
                    id: roof,
                    visible: Some(false),
                    locked: None,
                    opacity: None,
                },
            )
            .unwrap();
        assert!(!doc.layers.iter().any(|l| l.name == "Roof" && l.visible));
        let (_, doc) = s
            .canvas_edit(&m.id, "human", CanvasEdit::Delete { seq })
            .unwrap();
        assert!(doc.ops.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn canvas_edit_align_and_rotate_rect() {
        let dir = std::env::temp_dir().join(format!("aos-sess-align-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = ChatSessionStore::open(&dir).unwrap();
        let m = s.create(Some("Align".into()), None).unwrap();
        let (_, _, applied) = s
            .canvas_apply(
                &m.id,
                "human",
                CanvasOpBody::Rect {
                    x: 0.40,
                    y: 0.40,
                    w: 0.20,
                    h: 0.10,
                    color: "#3ee0c4".into(),
                    fill: true,
                    width: 0.01,
                    rotation: 0.0,
                    opacity: 1.0,
                    dash: vec![],
                    gradient: None,
                },
            )
            .unwrap();
        let seq = applied.unwrap().seq;
        let (_, doc) = s
            .canvas_edit(
                &m.id,
                "human",
                CanvasEdit::Align {
                    seq,
                    to_seq: None,
                    edges: vec!["left".into()],
                },
            )
            .unwrap();
        match &doc.ops[0].body {
            CanvasOpBody::Rect { x, rotation, .. } => {
                assert!((*x - 0.10).abs() < 1e-4);
                assert!(*rotation == 0.0);
            }
            _ => panic!("rect"),
        }
        let (_, doc) = s
            .canvas_edit(
                &m.id,
                "human",
                CanvasEdit::Rotate {
                    seq,
                    rotation: 15.0,
                },
            )
            .unwrap();
        match &doc.ops[0].body {
            CanvasOpBody::Rect { rotation, .. } => assert!((*rotation - 15.0).abs() < 1e-4),
            _ => panic!("rect"),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn canvas_set_style_then_stroke_inherits_pen() {
        let dir = std::env::temp_dir().join(format!("aos-sess-pen-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = ChatSessionStore::open(&dir).unwrap();
        let m = s.create(Some("Pen".into()), None).unwrap();
        let (_, doc) = s
            .canvas_set_style(&m.id, Some("#ff4400"), Some(0.025))
            .unwrap();
        assert_eq!(doc.pen.color, "#ff4400");
        assert!((doc.pen.width - 0.025).abs() < 0.0001);

        let (_, _, applied) = s
            .canvas_apply(
                &m.id,
                "agent-a",
                CanvasOpBody::Stroke {
                    points: vec![
                        aos_proto::CanvasPoint { x: 0.1, y: 0.1 },
                        aos_proto::CanvasPoint { x: 0.3, y: 0.3 },
                    ],
                    color: String::new(),
                    width: 0.0,
                    opacity: 1.0,
                    dash: vec![],
                },
            )
            .unwrap();
        let applied = applied.expect("stroke applied");
        match applied.body {
            CanvasOpBody::Stroke { color, width, .. } => {
                assert_eq!(color, "#ff4400");
                assert!((width - 0.025).abs() < 0.0001);
            }
            other => panic!("expected stroke, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn canvas_apply_normalizes_pixel_coords() {
        let dir = std::env::temp_dir().join(format!("aos-sess-canvas-px-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = ChatSessionStore::open(&dir).unwrap();
        let m = s.create(Some("Pixel".into()), None).unwrap();
        let (_, doc, _) = s
            .canvas_apply(
                &m.id,
                "agent-a",
                CanvasOpBody::Rect {
                    x: 256.0,
                    y: 128.0,
                    w: 128.0,
                    h: 128.0,
                    color: "#3ee0c4".into(),
                    fill: true,
                    width: 0.01,
                    rotation: 0.0,
                    opacity: 1.0,
                    dash: vec![],
                    gradient: None,
                },
            )
            .unwrap();
        let op = doc.ops.last().expect("op");
        match &op.body {
            CanvasOpBody::Rect { x, y, w, h, .. } => {
                assert!(*x > 0.4 && *x < 0.6);
                assert!(*y > 0.2 && *y < 0.4);
                assert!(*w > 0.2 && *w < 0.3);
                assert!(*h > 0.2 && *h < 0.3);
            }
            other => panic!("expected rect, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn canvas_import_replaces_document() {
        let dir = std::env::temp_dir().join(format!("aos-sess-import-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = ChatSessionStore::open(&dir).unwrap();
        let m = s.create(Some("Import".into()), None).unwrap();
        let mut imported = CanvasDoc {
            session_id: m.id.clone(),
            next_seq: 2,
            pen: CanvasPenStyle::default(),
            ops: vec![CanvasOp {
                seq: 1,
                author_id: "human".into(),
                ts_ms: 1,
                layer_id: String::new(),
                body: CanvasOpBody::Line {
                    p0: aos_proto::CanvasPoint { x: 0.1, y: 0.1 },
                    p1: aos_proto::CanvasPoint { x: 0.9, y: 0.9 },
                    color: "#3ee0c4".into(),
                    width: 0.02,
                    opacity: 0.5,
                    dash: vec![0.02, 0.02],
                },
            }],
            ..Default::default()
        };
        ensure_canvas_layers(&mut imported);
        let (meta, doc) = s
            .canvas_import(&m.id, imported, Some(CanvasAspect::Portrait9x16))
            .unwrap();
        assert_eq!(meta.canvas_aspect, CanvasAspect::Portrait9x16);
        assert_eq!(doc.ops.len(), 1);
        if let CanvasOpBody::Line { opacity, dash, .. } = &doc.ops[0].body {
            assert!((*opacity - 0.5).abs() < 1e-5);
            assert_eq!(dash, &vec![0.02, 0.02]);
        } else {
            panic!("line");
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
