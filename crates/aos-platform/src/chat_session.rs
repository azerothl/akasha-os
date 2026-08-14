//! Sessions de conversation persistées (Preview PC.6).

use aos_proto::{ChatSessionMessage, ChatSessionMeta};
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
        }
    }

    pub fn create(&self, title: Option<String>) -> Result<ChatSessionMeta, SessionError> {
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
    ) -> Result<ChatSessionMessage, SessionError> {
        if role.is_empty() || content.is_empty() {
            return Err(SessionError::BadRequest("role/content requis".into()));
        }
        let mut meta = self.load_meta(id)?;
        let msg = ChatSessionMessage {
            role: role.into(),
            content: content.into(),
            ts_ms: Self::now_ms(),
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
            out.push_str(&format!("## {}\n\n{}\n\n", m.role, m.content));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_append_list() {
        let dir = std::env::temp_dir().join(format!("aos-sess-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = ChatSessionStore::open(&dir).unwrap();
        let m = s.create(Some("Test".into())).unwrap();
        s.append(&m.id, "user", "bonjour").unwrap();
        s.append(&m.id, "assistant", "salut").unwrap();
        let (meta, msgs) = s.get(&m.id).unwrap();
        assert_eq!(meta.message_count, 2);
        assert_eq!(msgs.len(), 2);
        assert_eq!(s.list(false).unwrap().len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
