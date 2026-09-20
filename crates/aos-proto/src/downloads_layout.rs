//! Canonical layout for generated artefacts under `/downloads/`.
//!
//! Type-first folders keep personal-OS browsing tractable. Legacy flat paths
//! (`/downloads/foo.md`) are soft-normalized into the matching kind folder;
//! nested paths are left unchanged (no migration of existing files).

use std::path::Path;

/// Subfolder under `/downloads/` for a media/document kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadKind {
    Documents,
    Images,
    Video,
    Audio,
    Canvas,
    Fetched,
    Illustration,
}

impl DownloadKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Documents => "documents",
            Self::Images => "images",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Canvas => "canvas",
            Self::Fetched => "fetched",
            Self::Illustration => "illustration",
        }
    }
}

/// Logical directory for a kind (`/downloads/documents`).
pub fn downloads_kind_dir(kind: DownloadKind) -> String {
    format!("/downloads/{}", kind.as_str())
}

/// `/downloads/{kind}/{basename}` (basename may include subfolders).
pub fn default_download_path(kind: DownloadKind, basename: &str) -> String {
    let base = basename.trim().trim_start_matches('/');
    format!("{}/{}", downloads_kind_dir(kind), base)
}

/// Soft-normalize a path that still sits at the flat `/downloads/<file>` root.
///
/// - `/downloads/report.md` + Documents → `/downloads/documents/report.md`
/// - `/downloads/documents/report.md` → unchanged
/// - `/downloads/custom/x.md` → unchanged
/// - non-`/downloads/` paths → unchanged
pub fn normalize_download_path(path: &str, kind: DownloadKind) -> String {
    let path = path.trim();
    let Some(rest) = path.strip_prefix("/downloads/") else {
        return path.to_string();
    };
    if rest.is_empty() {
        return path.to_string();
    }
    // Already under a subfolder (typed or custom).
    if rest.contains('/') {
        return path.to_string();
    }
    default_download_path(kind, rest)
}

/// Map `files.generate` format to a download kind.
pub fn kind_for_files_format(format: &str) -> DownloadKind {
    match format.trim().to_ascii_lowercase().as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" => DownloadKind::Images,
        "mp4" | "webm" | "avi" | "mov" | "mkv" => DownloadKind::Video,
        "mp3" | "wav" | "ogg" | "flac" | "m4a" | "aac" => DownloadKind::Audio,
        _ => DownloadKind::Documents,
    }
}

/// Infer kind from a path extension (fallback Documents).
pub fn kind_for_path(path: &str) -> DownloadKind {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    kind_for_files_format(&ext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_dirs_and_defaults() {
        assert_eq!(
            downloads_kind_dir(DownloadKind::Documents),
            "/downloads/documents"
        );
        assert_eq!(
            default_download_path(DownloadKind::Images, "image-1.png"),
            "/downloads/images/image-1.png"
        );
        assert_eq!(
            default_download_path(DownloadKind::Fetched, "page.html"),
            "/downloads/fetched/page.html"
        );
    }

    #[test]
    fn normalize_relocates_flat_root_only() {
        assert_eq!(
            normalize_download_path("/downloads/report.md", DownloadKind::Documents),
            "/downloads/documents/report.md"
        );
        assert_eq!(
            normalize_download_path("/downloads/documents/report.md", DownloadKind::Documents),
            "/downloads/documents/report.md"
        );
        assert_eq!(
            normalize_download_path("/downloads/misc/x.md", DownloadKind::Documents),
            "/downloads/misc/x.md"
        );
        assert_eq!(
            normalize_download_path("/documents/notes/a.md", DownloadKind::Documents),
            "/documents/notes/a.md"
        );
    }

    #[test]
    fn format_kind_mapping() {
        assert_eq!(kind_for_files_format("md"), DownloadKind::Documents);
        assert_eq!(kind_for_files_format("PDF"), DownloadKind::Documents);
        assert_eq!(kind_for_files_format("png"), DownloadKind::Images);
        assert_eq!(kind_for_files_format("webm"), DownloadKind::Video);
        assert_eq!(kind_for_files_format("wav"), DownloadKind::Audio);
    }
}
